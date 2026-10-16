// Pure IF-08 helper values from the pinned prototype, using only received map documents.
import assert from 'node:assert/strict';
import { readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { fileURLToPath, pathToFileURL } from 'node:url';
const root = fileURLToPath(new URL('../', import.meta.url));
const ref = `${root}/reference/ww2-rts`;
const pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
assert.equal(process.version, 'v26.11.1');
assert.equal(process.arch, 'x64');
assert.equal(execFileSync('git', ['rev-parse', 'HEAD'], {cwd: ref, encoding: 'utf8'}).trim(), pin);
execFileSync('git', ['diff', '--exit-code', 'HEAD', '--', 'shared', 'maps'], {cwd: ref});
const sim = await import(pathToFileURL(`${ref}/shared/sim.js`));
const world = await import(pathToFileURL(`${ref}/shared/world-layers.js`));
const territory = await import(pathToFileURL(`${ref}/shared/world-territories.js`));
const weather = await import(pathToFileURL(`${ref}/shared/weather.js`));
const rle = values => {
  const out = [];
  for (const value of values) {
    const last = out.at(-1);
    if (last && last[0] === value) last[1]++;
    else out.push([value, 1]);
  }
  return out;
};
const layerValue = map => {
  const l = world.worldLayers(map);
  return Object.fromEntries(Object.entries(l).map(([key, value]) => [key, value instanceof Map ? [...value] : rle(value)]));
};
const blocks = (map, known = null, seeds = null) => {
  const chars = map.rows.join(''), out = [];
  sim.houseKindsWhere(c => chars[c] === 'B' && (!known || known.includes(c)), chars.length, map.w,
    map.buildings ?? [], seeds, (cells, kind) => out.push({cells, kind}));
  return out;
};
const files = readdirSync(`${root}/rust/crates/server/src/maps`).filter(file => file.endsWith('.json')).sort();
assert.equal(files.length, 33);
const maps = files.map(file => {
  const map = JSON.parse(readFileSync(`${root}/rust/crates/server/src/maps/${file}`, 'utf8'));
  const original = JSON.parse(readFileSync(`${ref}/maps/${file}`, 'utf8'));
  assert.deepEqual(map, world.mapForClient(original));
  const key = file.slice(0, -5), mask = [...weather.roadMask(map)];
  const track = mask.flatMap((bit, c) => bit ? [c] : []).slice(0, 8);
  const stations = map.spawns.slice(0, 2).map(s => s.y * map.w + s.x);
  // These are known membership runs over each map's dimensions, independent of hidden geography.
  const runs = [[0, 0, map.w], [1, 0, 2], [1, map.w - 2, map.w], [2, 1, map.w - 1]];
  const points = [{x:0,z:0}, {x:4,z:2}, {x:2,z:4}, {x:map.w*2,z:0}, {x:-0.01,z:0}, {x:0,z:6}];
  const plan = weather.mapWeather(map, key);
  return {key, layers:layerValue(map), houses:blocks(map),
    levels:map.heights ? rle([...map.heights.join('')].map(sim.levelOf)) : null,
    roads:rle(mask), mood:weather.mapMood(map,key),
    weather:{now:plan.now,next:plan.next??null,at:plan.next?plan.at*20:0}, effects:weather.weatherEffects(plan.now),
    track,stations,yard:[...sim.railYard(map.w,track,stations)],
    runs,edges:territory.territoryEdges(runs),points,inside:points.map(at=>territory.insideKnownRegion({runs},at))};
});
const houseMap = {w:4,h:3,rows:['.BB.','BBBB','.BB.'],buildings:[
  {x:1,y:0,kind:'brick house'}, {x:2,y:2,kind:'wooden shed'}, {x:0,y:1,kind:'unknown'}]};
const houseCases = [
  {map:houseMap,known:null,seeds:null},
  {map:houseMap,known:null,seeds:[10,1,10]},
  {map:houseMap,known:[1,2,4,7,9,10],seeds:[10,1,4,7]},
  {map:{w:4,h:2,rows:['...B','B...']},known:null,seeds:null},
  {map:houseMap,known:null,seeds:[]},
].map(c=>({...c,expected:blocks(c.map,c.known,c.seeds)}));
const layerCases = [
  {w:6,h:1,rows:['D=NBR+']},
  {w:7,h:1,worldVersion:2,rows:['NBRNQAK'],layers:{ground:['D..+.M.'],objects:['NBR..AK'],mines:['...N...'],
    materials:[{c:0,material:'steel'},{c:1,material:'wood'},{c:3,material:'road'}],
    groundMaterials:[{c:0,material:'road'},{c:1,material:'soil'}],
    objectMaterials:[{c:0,material:'stone'},{c:1,material:'stone'}],mineMaterials:[{c:0,material:'wood'}]}},
].map(map=>({map,expected:layerValue(map)}));
const regionCases = [
  {region:{runs:[],bounds:[0,0,10,10]},at:{x:1,z:1},cell:2},
  {region:{bounds:[-2,-3,4,5]},at:{x:-2,z:-3},cell:2},
  {region:{bounds:[-2,-3,4,5]},at:{x:4,z:0},cell:2},
  {region:{},at:{x:0,z:0},cell:2},
  {region:{runs:[[1,2,4]]},at:{x:6,z:3},cell:3},
  {region:{runs:[[0,0,1]]},at:{x:-0.01,z:0},cell:2},
].map(c=>({...c,expected:!!territory.insideKnownRegion(c.region,c.at,c.cell)}));
const edgeCases = [[],[[0,0,2]],[[0,0,5],[1,1,2],[1,3,4]],[[1,3,4],[0,0,5],[1,1,2]]]
  .map(runs=>({runs,expected:territory.territoryEdges(runs)}));
const levels = [...'01234ab'].map(ch=>({ch,level:sim.levelOf(ch),encoded:sim.levelChar(sim.levelOf(ch))}));
const compositions = [];
for (const ground of '.D+MWF') for (const object of '.BH#TXY=RNAQKO') for (const mine of [false,true])
  compositions.push([ground,object,mine,world.composeWorldCell(ground,object,mine)]);
const output = `${JSON.stringify({reference:pin,node:process.version,arch:process.arch,maps,houseCases,layerCases,regionCases,edgeCases,levels,compositions,railDirs:sim.RAIL_DIRS})}\n`;
const path = `${root}/rust/crates/core/tests/fixtures/interface-map-helpers.json`;
if(process.argv.includes('--check')) assert.equal(readFileSync(path,'utf8'),output);
else writeFileSync(path,output);
console.log(`IF-08: ${maps.length} authored received maps and focused helper branches`);
