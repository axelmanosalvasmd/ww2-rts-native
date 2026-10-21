// Runs in headless Chromium (make-ground-tiles.mjs serves it). The pinned client/ground.js, unmodified, loads its
// textures and cuts them into tiles for every pixels-per-cell the game can pick (buildTiles); each material's tile at
// each size is posted back as raw RGBA. Then, for the cases asked for (?cases=a,b), it replays the oracle's start and
// cell rows (client-replica.mjs) and posts the textured canvas the browser painted, overlays included.
import * as ground from '/client/ground.js';
import { worldLayers } from '/shared/world-layers.js';
import { startState, levelChar } from '/shared/sim.js';
import { makeReplica } from '/terrain/client-replica.mjs';

const post = (name, body) => fetch('/save?name=' + encodeURIComponent(name), { method: 'POST', body });
const log = (msg) => fetch('/log', { method: 'POST', body: String(msg) });
const NAMES = ['grass', 'dirt', 'mud', 'field', 'road', 'water', 'rubble', 'shelled', 'earth'];
try {
  const look = ground.groundLook();
  await look.loading;
  const meta = { tiles: [], images: {} };
  const seen = new Set();
  for (let P = 24; P >= 2; P--) {
    // a one-row map whose createGround picks this P (World maps go down to 2)
    let w = Math.max(8, Math.floor(2048 / P));
    while (Math.floor(2048 / w) > P) w++;
    while (w > 1 && Math.floor(2048 / w) < P) w--;
    const map = { w, h: 1, rows: ['.'.repeat(w)], spawns: [], points: [], ...(P < 8 ? { world: { size: 'huge' } } : {}) };
    const gp = ground.createGround(map, null);
    if (gp.px !== P) { await log(`P ${P}: got ${gp.px}`); continue; }
    gp.paint([[...map.rows[0]]], new Uint8Array(w), [[...map.rows[0]]], [[...'.'.repeat(w)]]);
    const { tiles } = ground.groundLook();
    for (let m = 0; m < NAMES.length; m++) {
      const t = tiles[m], key = `${NAMES[m]}-${t.T}`;
      if (!t.data) { await log(`no data for ${key}`); continue; }
      if (seen.has(key)) continue;
      seen.add(key);
      meta.tiles.push({ name: NAMES[m], m, T: t.T, P, ox: t.ox, oy: t.oy });
      await post(`tile-${key}.rgba`, new Uint8Array(t.data.buffer, t.data.byteOffset, t.data.byteLength));
    }
    gp.dispose();
  }
  // the source pictures as the browser decodes them (for the record: the tiles above are what the ground uses)
  for (const name of NAMES) {
    const img = new Image(); img.src = `/client/textures/${name}.jpg`; await img.decode();
    meta.images[name] = [img.naturalWidth, img.naturalHeight];
  }
  await post('tiles.json', JSON.stringify(meta));
  const cases = new URLSearchParams(location.search).get('cases');
  if (cases) {
    const { clientStart, clientCells } = makeReplica({ ground, worldLayers, startState, levelChar });
    for (const name of cases.split(',')) {
      const r = await (await fetch(`/oracle/${name}.replay.json`)).json();
      const rows = (list) => list.map(row => row.map((v, i) => (v === null && i >= 2 ? undefined : v)));
      if (r.start.map.biome) r.start.map.biome = Uint8Array.from(r.start.map.biome);
      r.start.cells = rows(r.start.cells);
      const client = clientStart(r.start);
      const grab = async (k) => {
        const c = client.gp.canvas, d = c.getContext('2d').getImageData(0, 0, c.width, c.height).data;
        await post(`${name}.${k}.browser.rgba`, d);
        await log(`${name}.${k}: ${c.width}x${c.height} P=${client.gp.px}`);
      };
      await grab(0);
      // (JSON wrote an absent level as null: rows() puts it back, or setLevel would run)
      for (let k = 0; k < r.steps.length; k++) { clientCells(client, rows(r.steps[k])); await grab(k + 1); }
      client.gp.dispose();
    }
  }
  await fetch('/done', { method: 'POST', body: 'ok' });
} catch (e) {
  await log('error: ' + (e.stack || e));
  await fetch('/done', { method: 'POST', body: 'error' });
}
