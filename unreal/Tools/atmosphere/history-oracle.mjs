// Pinned-source numerical atmosphere history. No server, engine, GPU or RNG edit.
// node --import unreal/Tools/models/register.mjs <this file> --repo <repo>
//   --scratch <scratch> --inputs <json> --out <json> --fixture <owned .inl>
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { resolve, join, dirname } from 'node:path';
import { pathToFileURL } from 'node:url';
import { createHash } from 'node:crypto';
import assert from 'node:assert/strict';
const arg = (key, fallback) => { const i = process.argv.indexOf(key); return i < 0 ? fallback : process.argv[i + 1]; };
for (const key of ['--scratch', '--inputs', '--out', '--fixture']) assert.ok(arg(key), `${key} is required`);
const repo = resolve(arg('--repo', process.cwd())), ref = join(repo, 'reference/ww2-rts');
const scratch = resolve(arg('--scratch')), output = resolve(arg('--out')), fixture = resolve(arg('--fixture'));
mkdirSync(scratch, { recursive: true });
const sha = p => createHash('sha256').update(readFileSync(p)).digest('hex');
const { installShims, settle } = await import(pathToFileURL(join(repo, 'unreal/Tools/models/shims.mjs')));
installShims();
globalThis.localStorage = { getItem: () => null, setItem: () => {} };
globalThis.location = { search: '' };
const THREE = await import('three');
const imp = p => import(pathToFileURL(join(ref, p)));
const { gfx } = await imp('client/gfx.js');
const { wind, setWind } = await imp('client/wind.js');
const { mapForClient } = await imp('shared/world-layers.js');
const { planWeather, stepWeather, weatherRow } = await imp('shared/weather.js');
const source = readFileSync(join(ref, 'client/atmosphere.js'), 'utf8');
let observed = source;
function replace(old, next) {
  assert.equal(observed.split(old).length, 2, `unique source observation seam: ${old}`);
  observed = observed.replace(old, next);
}
replace("import { setMood } from './light.js';", 'const setMood = () => {};');
for (const p of ['gfx.js', 'moods.js', 'wind.js']) replace(`from './${p}'`, `from '${pathToFileURL(join(ref, 'client', p))}'`);
replace("from '../shared/weather.js'", `from '${pathToFileURL(join(ref, 'shared/weather.js'))}'`);
replace('    start, update, setWeather,', `    start, update, setWeather,
    historyState: () => ({ time: t, look: EASED.map(k => cur[k]), goal: EASED.map(k => goal[k]),
      haze: hazeNow.toArray(), hazeGoal: hazeGoal.toArray(), hazeHex: hazeNow.getHex(), hazeGoalHex: hazeGoal.getHex(),
      drift: drift.toArray(), weather: wxNow, kind: fall?.userData.kind ?? '', alpha: fallA,
      banksVisible: banks.visible, bankOpacity: banks.material.uniforms.uOpacity.value, mood: moodName }),`);
const modulePath = join(scratch, 'observed-atmosphere.mjs');
writeFileSync(modulePath, observed);
const { createAtmosphere } = await import(pathToFileURL(modulePath));
await settle();
const input = JSON.parse(readFileSync(resolve(arg('--inputs')), 'utf8'));
const definitions = [], rows = [], summaries = [], provenance = [];
const dt = Math.fround(0.05); // The native input type, below the original normal-frame 0.1 s cap.
function create(map, key, graphics, weather) {
  gfx.set(graphics); // Explicit before EVERY construction.
  wind.x = 0.55; wind.z = 0.25;
  const scene = new THREE.Scene(), camera = new THREE.PerspectiveCamera(42, 16 / 9, 1, 900);
  const renderer = { getDrawingBufferSize: v => v.set(1280, 720), getPixelRatio: () => 1 };
  const at = createAtmosphere({ scene, renderer, camera, cam: { x: map.w, y: 0, z: map.h } });
  const ground = new THREE.Mesh(new THREE.PlaneGeometry(map.w * 2, map.h * 2));
  const apron = { overlayGeometry: new THREE.BufferGeometry(), mesh: { geometry: new THREE.BufferGeometry() } };
  at.start({ map, key, ground, hAt: () => 0, weather, apron });
  return { at, start: row => at.start({ map, key, ground, hAt: () => 0, weather: row, apron }) };
}
const lookGoalKey = s => JSON.stringify([s.weather[0], ...s.wx.slice(0, 2)]);
function readInput(item) {
  const path = resolve(item.stream), messages = readFileSync(path, 'utf8').split('\n').filter(Boolean).map(l => JSON.parse(l));
  if (item.expectedSHA256) assert.equal(sha(path), item.expectedSHA256, 'immutable recipient stream');
  const start = messages.find(m => m.t === 'start'), snapshots = messages.filter(m => m.t === 's');
  assert.ok(start && snapshots.length > 0);
  const authored = JSON.parse(readFileSync(join(ref, 'maps', item.map + '.json'), 'utf8'));
  const map = mapForClient(authored), plan = planWeather('map', authored, item.map);
  assert.equal(start.weather[0], plan.now);
  assert.equal(start.map.name, authored.name);
  for (const s of snapshots) {
    assert.ok(Array.isArray(s.weather) && Array.isArray(s.wx) && s.wx.length === 4);
    // Replay the schedule with its constructor-derived plan and ordinary ticks.
    // Living weather and wind are the immutable recipient rows, never fabricated.
    const clock = { tick: s.tick, weather: { ...plan } };
    stepWeather(clock);
    assert.deepEqual(s.weather, weatherRow(clock));
  }
  provenance.push({ map: item.map, graphics: item.graphics, streamSHA256: sha(path), bytes: readFileSync(path).length,
    snapshots: snapshots.length, firstTick: snapshots[0].tick, lastTick: snapshots.at(-1).tick,
    initialWeather: start.weather, plan, sourceMapSHA256: sha(join(ref, 'maps', item.map + '.json')) });
  return { map, plan, start, snapshots };
}
function record(caseIndex, name, view, state, frames, elapsed, goalSeconds, tick, label) {
  const value = view.at.historyState();
  const row = { caseIndex, name, weather: state.weather[0], rain: state.wx[0], wet: state.wx[1], frames, dt,
    renderSeconds: elapsed, goalSeconds, tick, label, ...value };
  rows.push(row);
  return row;
}
for (const item of input.lanes) {
  const { map, plan, start, snapshots } = readInput(item);
  const index = definitions.length, name = `${item.map}-${item.graphics}-natural-history`;
  definitions.push({ name, map: item.map, graphics: item.graphics, initialWeather: start.weather[0], initialMood: map.mood ?? '' });
  const view = create(map, item.map, item.graphics, start.weather);
  const first = snapshots[0].tick, last = Math.min(item.endTick ?? snapshots.at(-1).tick, snapshots.at(-1).tick);
  const checkpoints = new Set(item.checkpoints.filter(t => t >= first && t <= last));
  checkpoints.add(last);
  let current = snapshots[0], next = 0, segmentFrames = 0, elapsed = 0, goalSeconds = 0;
  let previousKey = JSON.stringify([start.weather[0], 0, 0]), segmentKey = null;
  const initial = record(index, name, view, { weather: start.weather, wx: [0, 0, 0, 0] }, 0, 0, 0, first, 'constructor-start');
  for (let tick = first; tick <= last; tick++) {
    let state = current;
    while (next < snapshots.length && snapshots[next].tick <= tick) state = snapshots[next++];
    const key = lookGoalKey(state);
    if (segmentFrames && segmentKey !== key) {
      record(index, name, view, current, segmentFrames, elapsed, goalSeconds, tick - 1, 'before-goal-change');
      segmentFrames = 0;
    }
    current = state; segmentKey = key;
    view.at.setWeather(state.weather); view.at.setRain(state.wx[0], state.wx[1]); setWind(state.wx[2], state.wx[3]);
    if (key !== previousKey) goalSeconds = 0;
    previousKey = key;
    view.at.update(dt); elapsed += dt; goalSeconds += dt; segmentFrames++;
    if (checkpoints.has(tick)) {
      record(index, name, view, current, segmentFrames, elapsed, goalSeconds, tick, 'checkpoint');
      segmentFrames = 0;
    }
  }
  const final = view.at.historyState();
  // Reconnect/new start is a new presentation initialisation. It snaps to the
  // delivered weather; it does not reconstruct the absent frames or reset t.
  const previousTime = final.time;
  view.start(current.weather);
  const reconnected = view.at.historyState();
  assert.equal(reconnected.time, previousTime);
  assert.deepEqual(reconnected.look, reconnected.goal);
  summaries.push({ name, frames: last - first + 1, firstTick: first, lastTick: last, elapsed,
    checkpoints: rows.filter(r => r.caseIndex === index && r.label === 'checkpoint'),
    reconnect: { particleTimePreserved: true, time: reconnected.time, weather: reconnected.weather,
      lookSnapsToGoal: true, look: reconnected.look }, initialLook: initial.look });

  if (plan.next && last >= plan.at + 160) {
    const fastIndex = definitions.length, fastName = `${item.map}-${item.graphics}-fast-prefix-negative`;
    definitions.push({ name: fastName, map: item.map, graphics: item.graphics, initialWeather: start.weather[0], initialMood: map.mood ?? '' });
    const fast = create(map, item.map, item.graphics, start.weather);
    record(fastIndex, fastName, fast, { weather: start.weather, wx: [0, 0, 0, 0] }, 0, 0, 0, first, 'constructor-start');
    let lastInput = { weather: start.weather, wx: [0, 0, 0, 0] };
    for (const s of snapshots.filter(s => s.tick <= plan.at + 160)) {
      if (lookGoalKey(s) !== lookGoalKey(lastInput)) {
        fast.at.setWeather(s.weather); fast.at.setRain(s.wx[0], s.wx[1]);
        record(fastIndex, fastName, fast, s, 0, 0, 0, s.tick, 'prefix-goal-without-frame');
      }
      lastInput = s;
    }
    for (let i = 0; i < 3; i++) fast.at.update(dt);
    const after = record(fastIndex, fastName, fast, lastInput, 3, 3 * dt, 3 * dt, lastInput.tick, 'three-frames-only');
    assert.equal(after.weather, 'clear');
    assert.ok(after.look[8] > 0.9, 'RED-capable negative: Clear HUD still has fog banks after a fast prefix');
    const matched = rows.find(r => r.caseIndex === index && r.tick === plan.at + 160);
    assert.ok(matched, 'natural-history +8 s checkpoint exists');
    assert.ok(matched.look[8] < 0.05, 'matched render dt clears almost all banks');
    summaries.push({ name: fastName, frames: 3, weather: after.weather, banks: after.look[8],
      naturalBanksAtSameTick: matched.look[8], renderSeconds: after.renderSeconds, naturalRenderSeconds: matched.renderSeconds,
      rejection: 'Same recipient tick/weather with different consumed frame dt is not a temporal parity comparison.' });
  }
}
// The fixture contains source component values only. Native automation consumes
// each segment through the actual FTCAtmosphere::Update, not a duplicate easing.
const number = n => Number(n).toPrecision(17), quote = JSON.stringify;
let table = '// Generated by Tools/atmosphere/history-oracle.mjs from pinned source and natural recipient weather.\n';
table += '// Cur/goal order: haze, sun, shadow, clouds, wet, sheen, mud, cover, banks. No RNG override.\n';
table += 'struct FWorld26HistoryCase { const char* Name; const char* Map; bool Low; const char* Weather; };\n';
table += 'static const FWorld26HistoryCase World26HistoryCases[] = {\n';
for (const c of definitions) table += `    { ${quote(c.name)}, ${quote(c.map)}, ${c.graphics === 'low'}, ${quote(c.initialWeather)} },\n`;
table += '};\nstruct FWorld26HistoryStep { int Case, Frames, Tick; const char* Weather; double Rain, Wet, Dt, Seconds, GoalSeconds, Look[9], Haze[3]; bool Banks; };\n';
table += 'static const FWorld26HistoryStep World26HistorySteps[] = {\n';
for (const r of rows) table += `    { ${r.caseIndex}, ${r.frames}, ${r.tick}, ${quote(r.weather)}, ${number(r.rain)}, ${number(r.wet)}, ${number(r.dt)}, ${number(r.renderSeconds)}, ${number(r.goalSeconds)}, { ${r.look.map(number).join(', ')} }, { ${r.haze.map(number).join(', ')} }, ${r.banksVisible} },\n`;
table += '};\n';
if (process.argv.includes('--check')) assert.equal(readFileSync(fixture, 'utf8'), table, 'source fixture must regenerate exactly');
else writeFileSync(fixture, table);
const result = { format: 'tc-weather-render-history-v1', reference: 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d',
  status: 'Source/oracle only. Archived naturally constructed recipient inputs, unchanged source update bodies, deterministic supplied render dt.',
  sources: Object.fromEntries(['client/atmosphere.js', 'client/main.js', 'client/moods.js', 'shared/weather.js'].map(p => [p, sha(join(ref, p))])),
  observedModuleSHA256: sha(modulePath), inputSHA256: sha(resolve(arg('--inputs'))), definitions, provenance, summaries, rows,
  limits: 'No native engine, GPU, cooked, live-host, performance or whole-match acceptance. The numeric oracle uses a controlled common dt, not the archived native frame history. A natural tick never substitutes for rendered seconds. Reconnect starts a new presentation. Kasserine weather is Clear; its mood can emit dust.' };
mkdirSync(dirname(output), { recursive: true });
writeFileSync(output, JSON.stringify(result, null, 2) + '\n');
console.log(JSON.stringify({ cases: definitions.length, sourceFrames: summaries.reduce((n, s) => n + s.frames, 0),
  checkpointRows: rows.length, streamsVerified: provenance.length, fixture: process.argv.includes('--check') ? 'current' : 'written',
  negatives: summaries.filter(s => s.rejection).map(s => ({ name: s.name, banks: s.banks, matchedBanks: s.naturalBanksAtSameTick })) }));
