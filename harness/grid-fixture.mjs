// Records a log of spatial index operations and the answers `shared/grid.js` gives at the pin, for the
// Rust port's test (rust/crates/core/tests/grid.rs). Run from the repo root:
//   node harness/grid-fixture.mjs > rust/crates/core/tests/fixtures/grid.txt
import { SpatialGrid, setHeavy } from '../reference/ww2-rts/shared/grid.js';

// mulberry32, as shared/ai-schedule.js seeds Math.random
function mulberry32(seed) {
  let s = seed >>> 0;
  return () => {
    s = (s + 0x6D2B79F5) >>> 0;
    let t = s;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}
// FNV-1a over each id as 4 little-endian bytes
function fnv(ids) {
  let h = 0xcbf29ce484222325n;
  for (const id of ids) for (let k = 0; k < 4; k++) { h ^= BigInt((id >>> (8 * k)) & 255); h = (h * 0x100000001b3n) & 0xffffffffffffffffn; }
  return h.toString(16).padStart(16, '0');
}

const random = mulberry32(0x5eed61d), pick = n => Math.floor(random() * n);
const SIDE = 512, units = new Map(), lines = [];
const out = line => lines.push(line);
setHeavy(u => u.heavy);
let nextId = 1;
const add = () => {
  const u = { id: nextId++, x: random() * SIDE, z: random() * SIDE, owner: pick(4), heavy: random() < 0.3 };
  units.set(u.id, u);
  out(`add ${u.id} ${u.x} ${u.z} ${u.owner} ${u.heavy ? 1 : 0}`);
  return u;
};
for (let i = 0; i < 300; i++) add();
const grid = new SpatialGrid(units);
out('new 16');
const preds = { all: () => true, odd: u => u.id % 2 === 1, owner0: u => u.owner === 0, owner2: u => u.owner === 2 };
const predNames = Object.keys(preds);
const someUnit = () => { const ids = [...units.keys()]; return units.get(ids[pick(ids.length)]); };
for (let step = 0; step < 2000; step++) {
  const r = random();
  if (r < 0.45) {
    const u = someUnit(), far = random() < 0.1;
    u.x = Math.min(SIDE - 0.01, Math.max(0, u.x + (random() - 0.5) * (far ? 120 : 6)));
    u.z = Math.min(SIDE - 0.01, Math.max(0, u.z + (random() - 0.5) * (far ? 120 : 6)));
    grid.update(u);
    out(`move ${u.id} ${u.x} ${u.z}`);
  } else if (r < 0.52) {
    // moved without telling the index, as some movers do until the next rebuild
    const u = someUnit();
    u.x = Math.min(SIDE - 0.01, Math.max(0, u.x + (random() - 0.5) * 20));
    u.z = Math.min(SIDE - 0.01, Math.max(0, u.z + (random() - 0.5) * 20));
    out(`teleport ${u.id} ${u.x} ${u.z}`);
  } else if (r < 0.56) {
    const u = add();
    if (random() < 0.5) { grid.update(u); out(`update ${u.id}`); }
  } else if (r < 0.59) {
    const u = someUnit();
    units.delete(u.id);
    out(`del ${u.id}`);
  } else if (r < 0.62) {
    grid.rebuild(units);
    out('rebuild');
  } else {
    const kind = ['inOrder', 'unordered', 'heavy', 'has', 'hasHeavy', 'radius', 'nearest', 'ownedBy'][pick(8)];
    const at = { x: random() * SIDE, z: random() * SIDE }, radius = 2 + random() * 60, pred = predNames[pick(predNames.length)], match = preds[pred];
    if (kind === 'ownedBy') { const owner = pick(4), ids = grid.ownedBy(owner).map(u => u.id); out(`q ownedBy ${owner} ${ids.length} ${fnv(ids)}`); continue; }
    if (kind === 'has' || kind === 'hasHeavy') { out(`q ${kind} ${at.x} ${at.z} ${radius} ${pred} ${grid[kind](at, radius, match) ? 1 : 0}`); continue; }
    if (kind === 'nearest') { out(`q nearest ${at.x} ${at.z} ${radius} ${pred} ${grid.nearest(at, radius, match)?.id ?? 0}`); continue; }
    const ids = grid[kind](at, radius, match).map(u => u.id);
    out(`q ${kind} ${at.x} ${at.z} ${radius} ${pred} ${ids.length} ${fnv(ids)}`);
  }
}
console.log(`# shared/grid.js at the reference pin, recorded by harness/grid-fixture.mjs on node ${process.version} (v8 ${process.versions.v8})`);
console.log(lines.join('\n'));
