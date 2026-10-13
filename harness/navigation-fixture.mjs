// Record actual navigationLeg outputs. Run with x64 Node 24.21.0 from the repo root:
// node harness/navigation-fixture.mjs > rust/crates/simtest/tests/fixtures/navigation.json
// WW2_REFERENCE may name another checkout of the same read-only reference pin.
import { pathToFileURL } from 'node:url';
import { resolve } from 'node:path';
if (process.arch !== 'x64' || process.version !== 'v24.21.0') throw Error('Use x64 Node v24.21.0');
const source = process.env.WW2_REFERENCE ? pathToFileURL(resolve(process.env.WW2_REFERENCE, 'shared/navigation.js')) : new URL('../reference/ww2-rts/shared/navigation.js', import.meta.url);
const { navigationLeg } = await import(source);
let seed = 0x7a931ac;
function random() {
  seed = (seed + 0x6d2b79f5) >>> 0;
  let t = seed; t = Math.imul(t ^ (t >>> 15), t | 1);
  t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
  return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
}
const pick = n => Math.floor(random() * n);
const scenarios = [];
function scenario(name, w, h, flags, height, worldKnown = true) {
  const data = { name, w, h, flags: [...flags], height: height && [...height], worldKnown, actions: [] };
  const g = { w, h, flags: Uint16Array.from(flags), height: height && Int8Array.from(height), worldKnown,
    infantryRegionVersion: 0, vehicleRegionVersion: 0, navalRegionVersion: 0,
    obstructionVersion: 0, navigationKnowledgeVersion: 0, navigationChanges: [], navigationKnowledge: {} };
  scenarios.push(data);
  return {
    query(start, goal, block = 1, chunkSize = 16, maxLegCells = 32) {
      const leg = navigationLeg(g, start, goal, block, { chunkSize, maxLegCells });
      data.actions.push({ op: 'query', start, goal, block, chunkSize, maxLegCells,
        expected: { goal: leg.goal, connected: leg.connected, limited: leg.limited, allowed: [...leg.allowed],
          containsHex: Array.from({ length: Math.ceil((w * h + 1) / 4) }, (_, k) => {
            let nibble = 0; for (let j = 0; j < 4 && k * 4 + j <= w * h; j++) if (leg.contains(k * 4 + j)) nibble |= 1 << j;
            return nibble.toString(16);
          }).join('') } });
    },
    patch(cells, mode) {
      for (const [c, flag, level] of cells) { g.flags[c] = flag; if (g.height) g.height[c] = level; }
      delete g.navigationChangedFromVersion; delete g.navigationChangedCells;
      const from = g.navigationKnowledgeVersion;
      if (['direct', 'log', 'gap', 'empty'].includes(mode)) {
        g.navigationKnowledgeVersion++;
        if (mode === 'direct') { g.navigationChangedFromVersion = from; g.navigationChangedCells = cells.map(c => c[0]); }
        if (mode === 'log') g.navigationChanges.push({ from, to: from + 1, cells: cells.map(c => c[0]) });
        if (mode === 'gap') g.navigationChanges = [];
        if (mode === 'empty') { g.navigationChangedFromVersion = from; g.navigationChangedCells = []; }
      }
      if (mode === 'region') { g.infantryRegionVersion++; g.vehicleRegionVersion++; g.navalRegionVersion++; }
      if (mode === 'obstruction') g.obstructionVersion++;
      if (mode === 'replace') { g.flags = Uint16Array.from(g.flags); if (g.height) g.height = Int8Array.from(g.height); }
      if (mode === 'knowledge') g.navigationKnowledge = {};
      data.actions.push({ op: 'patch', cells, mode });
    },
  };
}
const diagonal = scenario('one-way-diagonal-down', 2, 2, [0, 0, 0, 0], [0, 2, 2, 1]);
for (const [s, g] of [[0, 3], [3, 0], [0, 0], [1, 0]]) for (const limit of [1, 2, 5]) diagonal.query(s, g, 1, 2, limit);
const reverse = scenario('one-way-diagonal-up', 2, 2, [0, 0, 0, 0], [1, -1, -1, 0]);
reverse.query(0, 3, 1, 2, 1); reverse.query(3, 0, 1, 2, 1);
for (const [w, h] of [[1, 1], [1, 9], [9, 1], [17, 13], [129, 2]]) {
  const flat = scenario(`open-${w}x${h}`, w, h, Array(w * h).fill(0), null, w !== 129);
  for (const chunk of [0, 1, 2, 3, 16, 32]) for (const limit of [0, 1, 4, 32]) {
    flat.query(0, w * h - 1, 1, chunk, limit); flat.query(w * h - 1, 0, 1, chunk, limit);
  }
}
const ordinary = scenario('ordinary-map-all-connected-regions', 9, 9, Array(81).fill(0), Array(81).fill(0), false);
ordinary.query(0, 80, 1, 2, 1); ordinary.query(80, 0, 1, 2, 1);
const caught = scenario('newly-blocked-start', 5, 1, [1, 0, 0, 0, 1], [0, 0, 0, 0, 0]);
caught.query(0, 3, 1, 2, 1); caught.query(0, 4, 1, 2, 1); caught.query(2, 4, 1, 2, 1);
const stale = scenario('unversioned-height-fallback', 5, 1, Array(5).fill(0), Array(5).fill(0));
stale.query(0, 4, 1, 8, 32); stale.patch([[2, 0, 4]], 'none'); stale.query(0, 4, 1, 8, 32);
for (let index = 0; index < 24; index++) {
  const w = 8 + pick(18), h = 6 + pick(14), n = w * h;
  const flags = Array.from({ length: n }, () => random() < .17 ? [1, 2, 1024, 3][pick(4)] : 0);
  const height = index % 4 === 0 ? null : Array.from({ length: n }, () => index % 3 ? pick(4) - 1 : pick(2));
  const current = scenario(`seeded-${index}`, w, h, flags, height, index % 4 !== 1);
  const chunk = [2, 3, 5, 16][index % 4];
  for (let i = 0; i < 60; i++) {
    const s = pick(n), g = pick(n), block = [0, 1, 2, 3, 1024, 1025][i % 6];
    current.query(s, g, block, chunk, [1, 3, 8, 32, 999][i % 5]);
    if (i % 13 === 0) current.query(s, g, block, chunk, 3);
  }
  for (const mode of ['direct', 'direct', 'log', 'log', 'gap', 'region', 'obstruction', 'replace', 'knowledge', 'empty']) {
    const cells = Array.from({ length: 3 }, () => [pick(n), [0, 1, 2, 3, 1024][pick(5)], pick(4) - 1]);
    current.patch(mode === 'empty' ? [] : cells, mode);
    for (let i = 0; i < 12; i++) current.query(pick(n), pick(n), [1, 2, 1024][i % 3], chunk, [1, 8, 32][i % 3]);
  }
}
// More than 128 region-pair routes exercises bounded cache eviction. Knowledge
// updates skip queries so class-specific caches must bridge the change log.
const history = scenario('route-cache-and-skipped-refreshes', 32, 24, Array(768).fill(0), Array(768).fill(0));
for (let i = 0; i < 150; i++) history.query((i * 7) % 768, (767 - i * 3) % 768, 1, 2, 3);
for (let i = 0; i < 36; i++) {
  history.patch([[i + 40, i % 2, 0]], 'log');
  history.query(0, 767, i < 33 ? 2 : 1, 2, 7);
}
history.query(0, 767, 1024, 2, 7);
process.stdout.write(JSON.stringify({ reference: 'e4e579a', node: process.version, arch: process.arch, seed: '0x7a931ac', scenarios }) + '\n');
