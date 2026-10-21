// Read-only diagnosis. Never repair, cap, subdivide or replay a failed history.
import assert from 'node:assert/strict';
import { readFileSync, writeFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { dirname, resolve, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { nativeWeatherPlan } from './TCWorld27WeatherControl.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, '../../../..');
const finite = a => Array.isArray(a) && a.every(Number.isFinite);
const vec = (a, n) => finite(a) && a.length === n;
const key = r => [r.start, r.frame, r.engineFrame].join('/');

export function auditWeather(log, messages) {
  const parseErrors = [];
  const parse = tag => log.split('\n').flatMap((line, i) => {
    const at = line.indexOf(tag + ' ');
    if (at < 0) return [];
    try { return [JSON.parse(line.slice(at + tag.length + 1))]; }
    catch (error) { parseErrors.push({ line: i + 1, tag, message: error.message }); return []; }
  });
  const rows = parse('atmos-history'), views = parse('atmos-view');
  const first = rows[0], frames = rows.filter(r => r.event === 'frame');
  const starts = rows.filter(r => r.event === 'start');
  const blockers = [];
  const check = (ok, code, detail) => { if (!ok) blockers.push({ code, ...detail }); };
  check(!parseErrors.length, 'malformed-history', { parseErrors });
  check(starts.length === 1 && first?.event === 'start' && first.start === 1 && first.frame === 0 &&
    first.time === 0 && first.renderSeconds === 0 && vec(first.drift, 2) && first.drift.every(v => v === 0),
  'natural-constructor', { starts: starts.length, first });
  check(rows.length === frames.length + 1 && frames.length > 0 && frames.every((r, i) =>
    r.start === first?.start && r.frame === i + 1 && r.graphics === first.graphics && !r.fixedClock) && !first?.fixedClock,
  'complete-ordinary-frames', { frameCount: frames.length });
  const invalidDt = frames.filter(r => !Number.isFinite(r.dt) || r.dt < 0 || r.dt > .1);
  check(!invalidDt.length, 'original-delta-cap', { count: invalidDt.length, originalMaximum: .1,
    first: invalidDt[0] ?? null, maximum: Math.max(0, ...frames.map(r => Number.isFinite(r.dt) ? r.dt : 0)) });
  const byFrame = new Map(), duplicateViews = [];
  for (const view of views) {
    if (byFrame.has(key(view))) duplicateViews.push(key(view));
    byFrame.set(key(view), view);
  }
  const missingViews = frames.filter(r => !byFrame.has(key(r))).map(r => r.frame);
  const invalidViews = frames.filter(r => {
    const v = byFrame.get(key(r));
    return v && (!['eyeCm', 'forward', 'right', 'up'].every(k => vec(v[k], 3)) || !vec(v.focus, 2) ||
      !vec(v.viewport, 2) || !v.viewport.every(n => Number.isSafeInteger(n) && n > 0) ||
      !(v.hFov > 0 && v.hFov < 180 && v.distance > 0 && Number.isFinite(v.yaw)));
  }).map(r => r.frame);
  check(!duplicateViews.length && !missingViews.length && !invalidViews.length, 'per-frame-camera',
    { duplicateViews, missingViews, invalidViews });
  let seconds = 0, maxAccumulatorError = 0, maxClockError = 0, previousTick = -1, previousEngine = -1;
  const unordered = [], invalidInputs = [];
  for (const r of frames) {
    if (!(Number.isSafeInteger(r.engineFrame) && r.engineFrame > previousEngine &&
      Number.isSafeInteger(r.tick) && r.tick >= previousTick)) unordered.push(r.frame);
    previousTick = r.tick; previousEngine = r.engineFrame;
    if (!Number.isFinite(r.rain) || !Number.isFinite(r.wet) || !vec(r.wind, 2) || !vec(r.look, 9) ||
      !vec(r.goal, 9) || !vec(r.haze, 3) || !vec(r.hazeGoal, 3)) invalidInputs.push(r.frame);
    seconds += r.dt;
    maxAccumulatorError = Math.max(maxAccumulatorError, Math.abs(seconds - r.renderSeconds));
    maxClockError = Math.max(maxClockError, Math.abs(seconds % 10000 - r.time));
  }
  check(!unordered.length, 'ordered-engine-and-recipient', { frames: unordered });
  check(!invalidInputs.length, 'complete-weather-input', { frames: invalidInputs });
  check(Number.isFinite(maxAccumulatorError) && maxAccumulatorError < 1e-8 && maxClockError < 1e-8,
    'accumulated-natural-clock', { maxAccumulatorError, maxClockError });
  const start = messages.find(m => m.t === 'start');
  const ticks = new Set([0, ...messages.filter(m => m.t === 's').map(m => m.tick)]);
  const missingTicks = [...new Set(frames.filter(r => !ticks.has(r.tick)).map(r => r.tick))];
  check(start?.map?.name === 'Pegasus Bridge' && !start.map.world && start.weather?.[0] === 'fog' && first?.weather === 'fog',
    'public-pegasus-constructor', { sourceMap: start?.map?.name, sourceWeather: start?.weather, nativeWeather: first?.weather });
  check(!missingTicks.length, 'unchanged-recipient-ticks', { missingTicks });
  const clearIndex = frames.findIndex(r => r.weather === 'clear'), clear = frames[clearIndex];
  const beginning = frames.find(r => r.tick > 0);
  const pacingError = clear && beginning ? (clear.renderSeconds - beginning.renderSeconds) - (clear.tick - beginning.tick) / 20 : null;
  check(clearIndex > 0 && clear?.tick === 4800, 'exact-natural-lift-tick', {
    expectedTick: 4800, observedTick: clear?.tick ?? null, firstClearFrame: clear?.frame ?? null,
    lastFogTick: frames[clearIndex - 1]?.tick ?? null, tick4800InRecipient: ticks.has(4800) });
  check(pacingError !== null && Math.abs(pacingError) <= 2, 'natural-recipient-pacing', {
    secondsError: pacingError, tolerance: 2, firstTick: beginning?.tick ?? null, firstClearTick: clear?.tick ?? null });
  const marks = {};
  for (const [tag, tick] of [['before', 300], ['warning', 4600]]) {
    const r = frames.find(r => r.tick >= tick && r.weather === 'fog');
    if (r) marks[tag] = { frame: r.frame, tick: r.tick, renderSeconds: r.renderSeconds };
    else blockers.push({ code: 'missing-control', tag });
  }
  for (const age of [1, 3, 8, 14, 20]) {
    const r = clear && frames.find(r => r.renderSeconds >= clear.renderSeconds + age);
    if (r) marks['after-' + age + 's'] = { frame: r.frame, tick: r.tick, renderSeconds: r.renderSeconds };
    else blockers.push({ code: 'missing-control', tag: 'after-' + age + 's' });
  }
  let strict = { accepted: false, error: null };
  try { const p = nativeWeatherPlan(log); strict = { accepted: true, frames: p.frames, seconds: p.seconds }; }
  catch (error) { strict.error = error.message; }
  return { format: 'tc-world28-weather-feasibility-v1', graphics: first?.graphics ?? null,
    inputKind: 'retained actual native CPU inputs and unchanged original recipient; fresh offline audit',
    counts: { starts: starts.length, frames: frames.length, views: views.length,
      cameraJoins: frames.length - missingViews.length, deltasAboveOriginalCap: invalidDt.length },
    history: { seconds, minimumFramesAtOriginalCap: Math.ceil(seconds / .1), maximumDt: Math.max(0, ...frames.map(r => r.dt)),
      firstTick: frames[0]?.tick ?? null, lastTick: frames.at(-1)?.tick ?? null, maxAccumulatorError, maxClockError,
      firstClear: clear ? { frame: clear.frame, tick: clear.tick, renderSeconds: clear.renderSeconds } : null,
      pacingErrorSeconds: pacingError, marks }, strict, blockers, cpuPlanReady: strict.accepted && !blockers.length,
    sourceRendered: false, nativeGpuReadbackCorrelated: false, matchedHistoryQualified: false,
    conclusion: blockers.length ? 'This actual history cannot drive an equivalent ordinary source control unchanged.' :
      'CPU prerequisites pass. Source rendering and native GPU callback correlation remain separate prerequisites.',
    prerequisite: 'Supply one newly observed natural native constructor history with every real dt <= 0.1, complete per-frame public camera, exact lift at tick 4800, natural recipient pacing and +20 s history. Then run the unchanged strict original control and correlate actual readbacks. No retiming, subdivisions, RNG or source/assertion changes.',
    limits: 'No new engine/browser run, weather acceptance, hardware benchmark or proof that a future ordinary runtime can meet these prerequisites. A .4 s input cannot be consumed as one normal source frame under the original .1 s cap.' };
}

async function main() {
  const opt = n => { const i = process.argv.indexOf('--' + n); return i < 0 ? null : process.argv[i + 1]; };
  for (const n of ['native-log', 'stream', 'out']) assert.ok(opt(n), '--' + n + ' required');
  const hash = path => createHash('sha256').update(readFileSync(path)).digest('hex');
  const receipt = path => ({ path: resolve(path), sha256: hash(path) });
  const source = join(repo, 'reference/ww2-rts/client/main.js');
  const native = join(repo, 'unreal/Source/ThreeCrossroads/Client/TCGameMode.cpp');
  const cap = readFileSync(source, 'utf8').split('\n').findIndex(l => l.includes('dt = Math.min(0.1, (now - lastT) / 1000)'));
  const ingress = readFileSync(native, 'utf8').split('\n').findIndex(l => l.includes('Atmosphere.Update(DeltaSeconds, State, Terrain'));
  assert.ok(cap >= 0 && ingress >= 0, 'pinned cap and actual native input seam must be traced anew');
  const messages = readFileSync(opt('stream'), 'utf8').split('\n').filter(Boolean).map(JSON.parse);
  const result = auditWeather(readFileSync(opt('native-log'), 'utf8'), messages);
  result.provenance = { nativeLog: receipt(opt('native-log')), recipient: receipt(opt('stream')),
    sourceCap: { ...receipt(source), line: cap + 1 }, nativeForward: { ...receipt(native), line: ingress + 1 },
    strictTool: receipt(join(here, 'TCWorld27WeatherControl.mjs')), auditor: receipt(fileURLToPath(import.meta.url)),
    reference: 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d' };
  writeFileSync(opt('out'), JSON.stringify(result, null, 2) + '\n', { flag: 'wx' });
  console.log(JSON.stringify({ graphics: result.graphics, counts: result.counts, history: result.history,
    blockers: result.blockers.map(b => b.code), cpuPlanReady: result.cpuPlanReady }));
  if (process.argv.includes('--require-ready')) assert.ok(result.cpuPlanReady,
    'actual history is not an unchanged ordinary source/native rendered-history plan');
}
if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) await main();
