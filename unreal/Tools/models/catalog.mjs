// Every model the reference client renders, built by the reference's own code. Each entry names the module and
// function that builds it and the arguments, so the manifest can say where every exported file came from.
//
// Looks: a unit's paint is client/main.js look(slot) = { ...FACTIONS[faction], color: COLORS[slot] ?? 0xdddddd }.
// The faction fixes uniform and vehicle paint; the slot fixes the owner color. Both tables are read from the pinned
// client/main.js source text (it is a browser entry point and cannot be imported), and checked for shape.
import { readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

export function readLooks(reference) {
  const main = readFileSync(join(reference, 'client/main.js'), 'utf8');
  const factions = [...main.matchAll(/\{ name: '([^']+)', uniform: (0x[0-9a-f]{6}), vehicle: (0x[0-9a-f]{6}),/g)].map((m) => ({ name: m[1], uniform: Number(m[2]), vehicle: Number(m[3]) }));
  const colors = main.match(/const COLORS = \[([^\]]+)\];/)?.[1].split(',').map((s) => Number(s.trim()));
  const fallback = Number(main.match(/color: COLORS\[slot\] \?\? (0x[0-9a-f]{6})/)?.[1]);
  if (factions.length !== 4 || factions.map((f) => f.name).join() !== 'USA,Germany,USSR,UK') throw new Error('client/main.js FACTIONS did not parse as USA, Germany, USSR, UK');
  if (colors?.length !== 6 || colors.some((c) => !Number.isInteger(c))) throw new Error('client/main.js COLORS did not parse as six colors');
  if (!Number.isInteger(fallback)) throw new Error('client/main.js look() fallback color did not parse');
  return { factions, colors, fallback };
}

// The look variants the exporter builds. 'base' is the faction's own paint with the reference's no-slot owner color
// (the exported COLOR_0). slot0..5 are the six player colors (to check the tint attributes against). o1, o2, v2 and
// v3 are probes for the per-vertex owner and vehicle tint weights: mid greys, so no part turns near-black (which
// would switch unit-models.js darkSteel() to gunmetal) and no clamp in a builder is hit by a black or white paint.
export const PROBE = { lo: 0x404040, hi: 0xc0c0c0, mid: 0x808080 };
export function lookFor(name, looks) {
  const own = (fac) => looks.factions[fac];
  const make = (owner, vehicle) => (fac) => ({ ...own(fac), names: {}, color: owner, vehicle: vehicle ?? own(fac).vehicle });
  if (name === 'base') return { name, owner: looks.fallback, f: make(looks.fallback) };
  const slot = name.match(/^slot(\d)$/);
  if (slot) return { name, owner: looks.colors[+slot[1]], f: make(looks.colors[+slot[1]]) };
  if (name === 'o1') return { name, owner: PROBE.lo, f: make(PROBE.lo) };
  if (name === 'o2') return { name, owner: PROBE.hi, f: make(PROBE.hi) };
  if (name === 'v2') return { name, owner: PROBE.lo, vehicle: PROBE.hi, f: make(PROBE.lo, PROBE.hi) };
  if (name === 'v3') return { name, owner: PROBE.lo, vehicle: PROBE.mid, f: make(PROBE.lo, PROBE.mid) };
  throw new Error(`unknown look ${name}`);
}
export const LOOK_NAMES = ['base', 'slot0', 'slot1', 'slot2', 'slot3', 'slot4', 'slot5', 'o1', 'o2', 'v2', 'v3'];

// sets parsed from client/unit-models.js (module-private there), used only to describe which builder buildModel()
// dispatches each type to; the export itself always calls buildModel()
function readDispatch(reference) {
  const src = readFileSync(join(reference, 'client/unit-models.js'), 'utf8');
  const set = (name) => { const m = src.match(new RegExp(`const ${name} = new Set\\(\\[([^\\]]+)\\]\\)`)); if (!m) throw new Error(`unit-models.js ${name} did not parse`); return new Set([...m[1].matchAll(/'([^']+)'/g)].map((x) => x[1])); };
  const uk = src.match(/const UK_TANKS = \{ ([^}]+) \}/)?.[1];
  if (uk !== 'churchill, medium: cromwell') throw new Error('unit-models.js UK_TANKS changed');
  return { UK_MODELS: set('UK_MODELS'), BUILDINGS: set('BUILDINGS'), BLOCKS: set('BLOCKS'), UK_TANKS: { churchill: 'client/models/churchill.js churchill(look)', medium: 'client/models/cromwell.js cromwell(look)' } };
}

export async function loadReference(reference) {
  const imp = (p) => import(pathToFileURL(join(reference, p)).href);
  const THREE = await import('three');
  const sim = await imp('shared/sim.js');
  const um = await imp('client/unit-models.js');
  const surfaces = await imp('client/surfaces.js');
  const structures = await imp('client/structures.js');
  const props = await imp('client/props.js');
  const foliage = await imp('client/foliage.js');
  const aircraft = await imp('client/aircraft.js');
  const planes = await imp('client/models/planes.js');
  const wheeled = await imp('client/models/wheeled.js');
  const medium = await imp('client/models/armor-medium.js');
  const lightheavy = await imp('client/models/armor-lightheavy.js');
  const infantry = await imp('client/models/infantry.js');
  const geom = await imp('client/models/geom.js');
  const trackMotion = await imp('client/track-motion.js');
  const modelTextures = await imp('client/model-textures.js');
  const squadMen = await imp('shared/squad-men.js');
  const authored = await imp('client/models/infantry-authored-data.js');
  // main.js wires these two at startup (client/main.js setSurfaces(surface), setBuildings(buildingModel))
  um.setSurfaces(surfaces.surface);
  um.setBuildings(structures.buildingModel);
  return { THREE, sim, um, surfaces, structures, props, foliage, aircraft, planes, wheeled, medium, lightheavy, infantry, geom, trackMotion, modelTextures, squadMen, authored: authored.default, dispatch: readDispatch(reference) };
}

// What buildModel() (client/unit-models.js) hands each unit type to, mirroring its if-chain (documentation only).
function dispatchOf(R, type, fac, look) {
  const { UK_MODELS, BUILDINGS, UK_TANKS } = R.dispatch, own = fac;
  if (fac === 3 && !UK_MODELS.has(type)) fac = 0;
  const out = (builder, modules, note) => ({ builder, modules, faction: fac, ownFaction: own, ...(note ? { note } : {}) });
  if (R.wheeled.isWheeled(type, fac)) return out(`wheeledModel('${type}', ${fac}, look)`, ['client/models/wheeled.js']);
  if (type === 'flakpos') return out(`sandbagRing() + gunModel('flakpos', ${fac}, look)`, ['client/models/guns.js']);
  if (BUILDINGS.has(type)) return out(`buildingModel('${type}', look)${type === 'gate' ? " + buildingModel('gatedoor', look)" : ''}`, ['client/structures.js']);
  if (['supplycache', 'armory', 'shipyard', 'worldbase'].includes(type)) return out(`buildModel() inline parts ('${type}')`, ['client/unit-models.js']);
  if (['lcvp', 'gunboat', 'destroyer'].includes(type)) return out(`bakedNavalModel(navalFinished, '${type}', ${own}, look)`, ['client/models/blender-baked.js', 'client/models/naval-finished-data.js'], 'Blender-finished payload (format ww2-blender-naval-v1)');
  if (type === 'kaiju') return out('kaiju(v, root, key)', ['client/unit-models.js']);
  const uk = (fac === 3 || type === 'churchill') && UK_TANKS[type];
  if (R.medium.isArmorMedium(type, fac) && !uk) return out(`buildArmorMedium(v, root, '${type}', ${fac}, look)`, ['client/models/armor-medium.js', 'client/models/track-data.js']);
  if (uk) return out(UK_TANKS[type], [UK_TANKS[type].split(' ')[0], 'client/models/track-data.js']);
  if (R.lightheavy.lightHeavy(type, fac, look)) return out(`lightHeavy('${type}', ${fac}, look)`, ['client/models/armor-lightheavy.js', 'client/models/track-data.js']);
  return out(`soldier(figure, ${own}, i, look) per SLOTS['${type}'] + gunModel('${type}', ${fac}, look)`, ['client/models/infantry.js', 'client/models/infantry-authored.js', 'client/models/infantry-authored-data.js', 'client/models/guns.js', 'shared/squad-men.js']);
}

// Posture and gait morph targets of a soldier, in the order client/unit-models.js poseMorphs() stores them and
// animate() weights them: kneel, prone, retreat (POSES 1-3), then the gait frames gaitWeights() writes at offsets
// 0 (walk, 8), 8 (aim, 4), 12 (crouch, 4), 16 (crawl, 4) and 20 (dig, 4; only on squads that dig).
export function soldierMorphNames(n) {
  const names = ['kneel', 'prone', 'retreat'];
  for (const [kind, k] of [['walk', 8], ['aim', 4], ['crouch', 4], ['crawl', 4], ['dig', 4]]) for (let i = 0; i < k; i++) names.push(`${kind}_${i}`);
  if (n !== 23 && n !== 27) throw new Error(`unexpected soldier morph count ${n}`);
  return names.slice(0, n);
}

// The unit roles buildModel() leaves on v, as node names
function unitNames(v, root) {
  const names = new Map([[root, 'root']]);
  const set = (o, n) => { if (o && !names.has(o)) names.set(o, n); };
  v.mounts?.forEach((m, i) => set(m, `mount_${i}`));
  set(v.turret, 'turret'); set(v.body, 'body'); set(v.hull, 'hull'); set(v.tail, 'tail'); set(v.door, 'door'); set(v.crew, 'crew');
  v.legs?.forEach((l, i) => set(l, `leg_${i}`));
  if (v.squad) {
    v.models.forEach((man, i) => {
      set(man, `man_${i}`); set(man.userData.pose, `man_${i}_pose`);
      man.userData.hi.forEach((m, k) => set(m, `man_${i}_near${k ? k : ''}`)); man.userData.lo.forEach((m, k) => set(m, `man_${i}_far${k ? k : ''}`));
    });
    if (v.squad.guns) { set(v.squad.guns.hi, 'gun_near'); set(v.squad.guns.lo, 'gun_far'); }
  }
  // the groups buildModel() keeps in local variables: a vehicle's hull (baked as key|hull) and a squad's gun holder
  let k = 0;
  for (const c of root.children) if (!names.has(c)) set(c, v.squad ? (k++ ? `gun_${k - 1}` : 'gun') : (k++ ? `hull_${k - 1}` : 'hull'));
  return names;
}

// fxTip (barrel tip for client/fx.js) as an empty child node, in the turret's frame when there is one
function addTip(R, v, root, names) {
  if (!v.fxTip) return null;
  const tip = new R.THREE.Object3D(); tip.position.set(...v.fxTip);
  (v.turret ?? root).add(tip); names.set(tip, 'fx_tip');
  return [...v.fxTip];
}

// a reference map variant for the structure and prop builders
function mapVariants(R, reference) {
  const dir = join(reference, 'maps'), out = [];
  for (const file of readdirSync(dir).sort()) {
    const map = JSON.parse(readFileSync(join(dir, file), 'utf8'));
    if (!Array.isArray(map.rows)) continue;
    out.push({ file: `maps/${file}`, map });
  }
  return out;
}

// The asset list. Each entry: { id, category, faction, lookDependent, source: { module, call, modules }, build(look) }
// where build returns { root (Object3D), names (Map), instances ('bake' | 'prototype'), extras, morphNames }.
export function catalog(R, reference) {
  const { THREE, sim, um } = R, list = [];
  const UNITS = sim.UNITS;
  for (const type of Object.keys(UNITS)) {
    const def = UNITS[type];
    if (def.air || type === 'airfield') continue; // client/aircraft.js (below)
    for (const fac of [0, 1, 2, 3]) list.push({
      id: `units/${type}/${type}-f${fac}`, category: 'unit', type, faction: fac, lookDependent: true,
      source: { module: 'client/unit-models.js', call: `buildModel(v, root, look(slot), ${fac}, UNITS['${type}'])` },
      build(look) {
        const f = look.f(fac), root = new THREE.Group(), v = { type, root, models: [], turret: null };
        um.buildModel(v, root, f, fac, def);
        const names = unitNames(v, root), fxTip = addTip(R, v, root, names);
        const extras = { type, faction: fac, dispatch: dispatchOf(R, type, fac, f), ...(fxTip ? { fxTip } : {}) };
        if (v.squad) extras.squad = { men: v.models.length, moveFire: v.squad.moveFire, slotScale: R.squadMen.SLOT_SCALE, manScale: R.dispatch.BLOCKS.has(type) ? 1.1 : 1.35, figure: type === 'medic' ? 'engineer' : type, kits: v.models.map((m) => m.userData.index) };
        if (v.traverse) extras.traverse = v.traverse;
        return { root, names, instances: 'bake', extras, morphNames: soldierMorphNames };
      },
    });
  }
  // Riders: client/unit-models.js setRiders(v, n, look, fac) kneels n squads (RIDERS men each) on a carrier's deck, as
  // one baked mesh per n, in the chassis frame (client/main.js calls it for UNITS with riders). Exported as three
  // alternative nodes riders_1..3: the reference shows exactly the one for the number of squads aboard.
  for (const type of Object.keys(UNITS)) {
    if (!UNITS[type].riders) continue;
    for (const fac of [0, 1, 2, 3]) list.push({
      id: `units/${type}/${type}-riders-f${fac}`, category: 'unit-riders', type, faction: fac, lookDependent: true,
      source: { module: 'client/unit-models.js', call: `setRiders(v, n, look(slot), ${fac}) for n = 1, 2, 3 after buildModel(v, root, look(slot), ${fac}, UNITS['${type}'])`, modules: ['client/unit-models.js', 'client/models/infantry.js'] },
      build(look) {
        const f = look.f(fac), root = new THREE.Group(), v = { type, root, models: [], turret: null };
        um.buildModel(v, root, f, fac, UNITS[type]);
        const out = new THREE.Group(), names = new Map([[out, 'root']]), seats = [];
        for (const n of [1, 2, 3]) {
          um.setRiders(v, n, f, fac);
          const g = v.riders; g.removeFromParent(); out.add(g); names.set(g, `riders_${n}`);
          if (n > 1) g.visible = false;
          v.ridersShown = 0; v.riders = null;
        }
        return { root: out, names, instances: 'bake', extras: { type, faction: fac, ridersPerSquad: R.um.RIDERS, frame: 'the vehicle root (vehicleBody() chassis group, identity)', alternatives: 'show exactly one riders_N for N squads aboard (N = flags >> CARGO_SHIFT & 3); riders_2 and riders_3 are hidden here' } };
      },
    });
  }
  // Planes: client/aircraft.js createAviation().preview(), the same instance() every plane unit and support run uses
  const roles = R.planes.ROLES;
  for (const role of roles) for (const fac of [0, 1, 2, 3]) list.push({
    id: `planes/${role}/${role}-f${fac}`, category: 'plane', faction: fac, lookDependent: true,
    source: { module: 'client/aircraft.js', call: `createAviation(ctx).preview('${role}', owner, 0, 0, 0, { fac: ${fac} })`, modules: ['client/aircraft.js', 'client/models/planes.js'] },
    build(look) {
      const world = new THREE.Group();
      const av = R.aircraft.createAviation({ hAt: () => 0, UNITS, SUPPORT: sim.SUPPORT, units: new Map(), altitude: 20, world: () => world, facOf: (o) => o, colorOf: () => look.f(fac).color, vehicleOf: () => look.f(fac).vehicle, me: () => fac });
      const { body, props, m } = av.preview(role, fac, 0, 0, 0, { fac });
      const names = new Map([[body, 'root'], [body.children[0], 'airframe']]);
      // the propeller groups start at a random turn (Math.random() in instance()); exported at rest
      props.forEach((p, i) => { p.rotation.x = 0; names.set(p, `prop_${i}`); names.set(p.children[0], `prop_${i}_blades`); });
      names.set(body.children[body.children.length - 1], 'prop_discs');
      body.removeFromParent();
      const extras = { role, faction: fac, name: R.planes.PLANE_NAMES[fac]?.[role], span: m.span, len: m.len, tail: m.tail, nose: m.nose, belly: !!m.belly, metal: !!m.metal, props: m.props.map((p) => ({ x: p.x, y: p.y, z: p.z, r: p.r, n: p.n })), shadowOutline: m.polys, propSpin: 'client/aircraft.js spins each prop_N about +x at 22-26 rad/s' };
      return { root: body, names, instances: 'bake', extras, labels: [[body.children[0].material, m.metal ? 'aircraft.js METAL_MAT' : 'aircraft.js BODY_MAT'], [props[0]?.children[0].material, 'aircraft.js BLADE_MAT'], [body.children[body.children.length - 1].material, 'aircraft.js DISC_MAT']] };
    },
  });
  list.push({
    id: 'planes/bomb', category: 'plane', faction: null, lookDependent: false,
    source: { module: 'client/aircraft.js', call: "createAviation(ctx).preview('attacker', owner, 0, 0, 0, { fac: 0, bomb: true }): the bomb mesh (BOMB_GEO)", modules: ['client/aircraft.js'] },
    build(look) {
      const world = new THREE.Group();
      const av = R.aircraft.createAviation({ hAt: () => 0, UNITS, SUPPORT: sim.SUPPORT, units: new Map(), altitude: 20, world: () => world, facOf: (o) => o, colorOf: () => look.f(0).color, vehicleOf: () => look.f(0).vehicle, me: () => 0 });
      const { body, m } = av.preview('attacker', 0, 0, 0, 0, { fac: 0, bomb: true });
      if (m.belly) throw new Error('the fac 0 attacker carries no external bomb');
      const bomb = body.children[body.children.length - 1], at = bomb.position.toArray(), scale = bomb.scale.x;
      const root = new THREE.Group(); bomb.position.set(0, 0, 0); root.add(bomb);
      return { root, names: new Map([[root, 'root'], [bomb, 'bomb']]), instances: 'bake', extras: { attachedAt: at, attachedScale: scale, note: 'client/aircraft.js hangs it under dive-bombing support planes without a belly bay; the node keeps its 1.4 scale' }, labels: [[bomb.material, 'aircraft.js BODY_MAT']] };
    },
  });
  for (const fac of [0, 1, 2, 3]) list.push({
    id: `structures/airfield/airfield-f${fac}`, category: 'structure', faction: fac, lookDependent: true,
    source: { module: 'client/aircraft.js', call: 'createAviation(ctx).buildAirfield(v, root, owner)', modules: ['client/aircraft.js'] },
    build(look) {
      const world = new THREE.Group(), root = new THREE.Group(), v = { type: 'airfield', root, models: [] };
      const av = R.aircraft.createAviation({ hAt: () => 0, UNITS, SUPPORT: sim.SUPPORT, units: new Map(), altitude: 20, world: () => world, facOf: () => fac, colorOf: () => look.f(fac).color, vehicleOf: () => look.f(fac).vehicle, me: () => fac });
      av.buildAirfield(v, root, fac);
      return { root, names: new Map([[root, 'root'], [v.body, 'body']]), instances: 'bake', extras: { faction: fac }, labels: [[v.body.children[0].material, 'aircraft.js FIELD_MAT']] };
    },
  });
  // HQ dressing: client/main.js buildHQ() adds sandbagRing(CFG.reinforceRadius + 0.8) and hqCamp(look, tent)
  const ringRadius = sim.CFG.reinforceRadius + 0.8;
  list.push({
    id: 'structures/hq-sandbag-ring/hq-sandbag-ring', category: 'structure', faction: null, lookDependent: false,
    source: { module: 'client/structures.js', call: `sandbagRing(${ringRadius})`, modules: ['client/structures.js'] },
    build() { const root = new THREE.Group(), ring = R.structures.sandbagRing(ringRadius); root.add(ring); return { root, names: new Map([[root, 'root'], [ring, 'bags']]), instances: 'bake', extras: { radius: ringRadius, note: 'one InstancedMesh in the reference, merged here with each bag\'s instance color as its vertex color' } }; },
  });
  for (const tent of [true, false]) for (const fac of [0, 1, 2, 3]) list.push({
    id: `structures/hq-camp${tent ? '' : '-flagpole'}/hq-camp${tent ? '' : '-flagpole'}-f${fac}`, category: 'structure', faction: fac, lookDependent: true,
    source: { module: 'client/structures.js', call: `hqCamp(look, ${tent})`, modules: ['client/structures.js'] },
    build(look) { const root = new THREE.Group(), camp = R.structures.hqCamp(look.f(fac), tent); root.add(camp); return { root, names: new Map([[root, 'root'], [camp, 'camp']]), instances: 'bake', extras: { faction: fac, tent } }; },
  });
  // Map pieces: client/structures.js buildStructures() draws one InstancedMesh per KINDS entry. Each kind's shared
  // geometry and material is the asset; where and how big each piece stands, and its per-instance tint, is the
  // layout code in structures.js (not model data). The kinds are collected by running buildStructures() over the
  // reference maps, as drawn, then with alternate house cells shot to rubble (ruins), with every cell at damage
  // stage 2, with map-drawn '#' walls treated as player-built (sandbag walls), and with a 'mine head' landmark on
  // each map's first house block.
  let pieces = null;
  const collectPieces = () => {
    if (pieces) return pieces;
    pieces = new Map();
    for (const { file, map } of mapVariants(R, reference)) {
      const w = map.rows[0].length, h = map.rows.length;
      const firstB = map.rows.join('').indexOf('B'), mine = firstB >= 0 ? [{ x: firstB % w, y: Math.floor(firstB / w), kind: 'mine head' }] : [];
      const variants = [
        ['as drawn', map.rows, null, map.buildings],
        ['alternate house cells destroyed to rubble', map.rows, null, map.buildings, map.rows.map((r, y) => [...r].map((ch, x) => (ch === 'B' && (x + y) % 2 === 0 ? 'R' : ch)).join(''))],
        ['all cells at damage stage 2', map.rows, new Uint8Array(w * h).fill(2 << 3), map.buildings],
        ["map '#' walls as player-built", map.rows.map((r) => r.replaceAll('#', '.')), null, map.buildings],
        ["'mine head' landmark", map.rows, null, [...(map.buildings ?? []), ...mine]],
      ];
      for (const [variant, orig, cells, buildings, live = map.rows] of variants) {
        const group = new THREE.Group();
        R.structures.buildStructures(group, live.map((r) => [...r]), orig, () => 0, cells, buildings, null);
        for (const im of group.children) if (!pieces.has(im.name)) pieces.set(im.name, { mesh: im, first: `${file} (${variant})` });
      }
    }
    return pieces;
  };
  const KIND_NAMES = readFileSync(join(reference, 'client/structures.js'), 'utf8').match(/const KINDS = \{([\s\S]*?)\n\};/)[1].matchAll(/^\s{2}(\w+): \{ geo:/gm);
  for (const [, kind] of KIND_NAMES) list.push({
    id: `pieces/${kind}`, category: 'piece', faction: null, lookDependent: false, kind,
    source: { module: 'client/structures.js', call: `buildStructures(group, grid, orig, hAt, cells, buildings) -> InstancedMesh '${kind}' (KINDS.${kind})`, modules: ['client/structures.js'] },
    build() {
      const p = collectPieces().get(kind);
      if (!p) throw new Error(`no reference map variant draws structure kind '${kind}'`);
      const root = new THREE.Group(), mesh = new THREE.Mesh(p.mesh.geometry, p.mesh.material);
      mesh.castShadow = p.mesh.castShadow; root.add(mesh);
      return { root, names: new Map([[root, 'root'], [mesh, kind]]), instances: 'prototype', extras: { kind, firstSeen: p.first, instanceTint: 'structures.js put(): each instance has a linear RGB color (a grey number or [r, g, b]) that multiplies the material', castShadow: p.mesh.castShadow, pickable: !!p.mesh.raycast && p.mesh.raycast.toString() !== '() => {}' } };
    },
  });
  // Foliage: client/foliage.js treeGeometry() and leafGeometry(), drawn instanced by props.js and structures.js
  for (const kind of ['broadleaf', 'pine', 'poplar']) list.push({
    id: `foliage/${kind}`, category: 'foliage', faction: null, lookDependent: false,
    source: { module: 'client/foliage.js', call: `treeGeometry('${kind}') with barkMaterial() and leafMaterial()`, modules: ['client/foliage.js'] },
    build() {
      const t = R.foliage.treeGeometry(kind), root = new THREE.Group();
      const bark = new THREE.Mesh(t.bark, R.foliage.barkMaterial()), leaves = new THREE.Mesh(t.leaves, R.foliage.leafMaterial());
      root.add(bark, leaves);
      return { root, names: new Map([[root, 'root'], [bark, 'bark'], [leaves, 'leaves']]), instances: 'prototype', extras: { kind } };
    },
  });
  for (const kind of ['bush', 'hedge', 'grass', 'crop']) list.push({
    id: `foliage/${kind}`, category: 'foliage', faction: null, lookDependent: false,
    source: { module: 'client/foliage.js', call: `leafGeometry('${kind}') with leafMaterial()`, modules: ['client/foliage.js'] },
    build() {
      const root = new THREE.Group(), leaves = new THREE.Mesh(R.foliage.leafGeometry(kind), R.foliage.leafMaterial());
      root.add(leaves);
      return { root, names: new Map([[root, 'root'], [leaves, 'leaves']]), instances: 'prototype', extras: { kind } };
    },
  });
  // Props: client/props.js createProps() draws one or two InstancedMeshes per kind. The tree, bush, grass and crop
  // kinds draw the foliage.js geometries above (checked by identity in the manifest); the rest are props.js's own.
  let propMeshes = null;
  const collectProps = () => {
    if (propMeshes) return propMeshes;
    propMeshes = new Map();
    for (const { file, map } of mapVariants(R, reference)) {
      const parent = new THREE.Group();
      R.props.createProps({ map, grid: map.rows.map((r) => [...r]), hAt: () => 0, parent });
      parent.traverse((o) => { if (o.isInstancedMesh && o.name && !propMeshes.has(o.name)) propMeshes.set(o.name, { mesh: o, first: file }); });
    }
    return propMeshes;
  };
  for (const kind of ['rocks', 'fence', 'haystack', 'supplies']) list.push({
    id: `props/${kind}`, category: 'prop', faction: null, lookDependent: false, kind,
    source: { module: 'client/props.js', call: `createProps({ map, grid, hAt, parent }) -> InstancedMesh '${kind}'`, modules: ['client/props.js', 'client/surfaces.js'] },
    build() {
      const p = collectProps().get(kind);
      if (!p) throw new Error(`no reference map draws prop kind '${kind}'`);
      const root = new THREE.Group(), mesh = new THREE.Mesh(p.mesh.geometry, p.mesh.material);
      mesh.castShadow = p.mesh.castShadow; root.add(mesh);
      return { root, names: new Map([[root, 'root'], [mesh, kind]]), instances: 'prototype', extras: { kind, firstSeen: p.first, instanceTint: 'props.js: per-instance color multiplies the material' } };
    },
  });
  return { list, collectProps, collectPieces };
}
