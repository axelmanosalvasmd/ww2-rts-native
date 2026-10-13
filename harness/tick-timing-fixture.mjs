// Execute the pinned timing history/controller and polling loop with explicit host clocks.
import assert from 'node:assert/strict';
import { readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { loadReference, REFERENCE_PIN } from './map-assets.mjs';

assert.equal(process.arch, 'x64');
assert.equal(process.version, 'v24.21.0');
assert.ok(process.argv.slice(2).every(a => a === '--check'));
const root = fileURLToPath(new URL('../', import.meta.url));
const reference = resolve(process.env.WW2_REFERENCE ?? `${root}/reference/ww2-rts`);
await loadReference(reference);
const { createTickMeter, recordTick, tickStats } = await import(pathToFileURL(`${reference}/tickmeter.js`));
const { TICK } = await import(pathToFileURL(`${reference}/shared/sim.js`));
const meters = [];
function meterTrace(name, initialNow, actions) {
  const meter = createTickMeter({ now: initialNow });
  const steps = actions.map(({ now, sample }) => {
    assert.equal(recordTick(meter, sample, now), meter.snapEvery);
    return { now, sample, expected: { snapEvery: meter.snapEvery, nextCheck: meter.nextCheck, lowSince: meter.lowSince, stats: tickStats(meter) } };
  });
  meters.push({ name, initialNow, steps });
}
const sample = (tick, sent = true, phases = {}) => ({ tick, ...phases, sent });
const at = (now, tick, sent = true, phases = {}) => ({ now, sample: sample(tick, sent, phases) });
const fill = (n, now, tick) => Array.from({ length: n }, () => at(now, tick));
meterTrace('minimum-sent-samples-and-overload-cap', 0, [
  ...Array.from({ length: 9 }, (_, i) => at(i, 60)),
  at(1000, 90, false), at(1000, 60), at(1999, 60), at(2000, 60), at(3000, 60),
]);
meterTrace('budget-equality-and-strict-overload', 0, [
  ...fill(10, 0, 40), at(1000, 40), at(2000, 40.000001), at(3000, 41),
]);
meterTrace('sustained-recovery-ten-seconds-and-floor', 0, [
  ...fill(10, 0, 60), at(1000, 60), at(2000, 60),
  ...fill(50, 3500, 23), at(4000, 23),
  ...Array.from({ length: 10 }, (_, i) => at(5000 + i * 1000, 23)),
  at(15000, 23), at(23000, 23), at(24000, 23), at(25000, 23), at(35000, 23),
]);
meterTrace('recovery-equality-does-not-start-low-window', 200, [
  ...fill(10, 200, 60), at(1200, 60),
  ...fill(50, 2200, 24), at(3200, 24), at(13200, 24),
]);
meterTrace('recovery-interruption-resets-low-window', 0, [
  ...fill(10, 0, 60), at(1000, 60),
  ...fill(50, 2000, 23), at(3000, 23),
  ...fill(50, 4000, 24), at(5000, 24),
  ...fill(50, 6000, 23), at(7000, 23), at(16000, 23), at(17000, 23),
]);
meterTrace('sent-false-snapshot-phases-and-duration-defaults', 25, [
  at(25, 12, true, { step: 4, think: 2, snapshot: 6, snapshotBuild: 3, snapshotStringify: 3 }),
  at(26, 99, false, { snapshot: 900, snapshotBuild: 800, snapshotStringify: 700 }),
  at(27, 10, true), at(28, 8, false, { step: 8 }), at(1025, 7, false),
]);
meterTrace('percentile-rank-and-both-ring-wraps', 500, Array.from({ length: 205 }, (_, i) => ({
  now: 500 + i,
  sample: sample(i + 1, true, { step: (i % 7) + 0.5, think: 205 - i, snapshot: i * 0.25, snapshotBuild: i % 11, snapshotStringify: i % 3 }),
})));
meterTrace('unsent-tick-ring-wrap-and-backwards-host-clock', 5000, [
  ...Array.from({ length: 201 }, (_, i) => at(5000 + i, i + 1, i === 0)),
  at(4999, 2, false), at(6000, 3, false),
]);

const server = readFileSync(`${reference}/server.js`, 'utf8');
const begin = 'const TICK_NS = BigInt(Math.round(TICK * 1e9));';
const end = '// The forwarding process: game connections go to the room';
assert.equal(server.split(begin).length, 2);
assert.equal(server.split(end).length, 2);
const start = server.indexOf(begin), stop = server.indexOf(end, start);
assert.ok(stop > start);
const original = server.slice(start, stop);
assert.equal(original.split('export const loop = ').length, 2);
// Remove only the module export declaration so the original closure can run in isolation.
const polling = original.replace('export const loop = ', 'const loop = ');
const clocks = [];
function clockTrace(name, initialNow, times) {
  let now = BigInt(initialNow), callback, ticks = 0;
  const state = new Function('TICK', 'process', 'PRIMARY', 'setInterval', 'diag', 'tickRooms', `${polling}\nreturn {due:()=>dueAt,tickNs:TICK_NS};`)(
    TICK, { hrtime: { bigint: () => now } }, false,
    (fn, delay) => { assert.equal(delay, 10); callback = fn; return 1; },
    { lateBy: value => assert.ok(value >= 0) }, () => ticks++,
  );
  const steps = times.map(time => {
    const before = state.due();
    ticks = 0; now = BigInt(time); callback();
    const due = state.due();
    const dropped = due !== before + BigInt(ticks) * state.tickNs;
    assert.ok(ticks <= 4);
    return { now: time, expected: { ticks, due: Number(due), dropped } };
  });
  clocks.push({ name, initialNow, steps });
}
clockTrace('immediate-zero-poll-and-exact-boundaries', 0, [0, 1, 49_999_999, 50_000_000, 50_000_001, 99_999_999, 100_000_000]);
clockTrace('partial-catch-up-without-drop', 100_000_000, [100_000_000, 225_000_000, 299_999_999, 300_000_000, 499_999_999]);
clockTrace('one-second-stall-cap-drop-and-resume', 0, [1_000_000_000, 1_000_000_001, 1_049_999_999, 1_050_000_000, 1_100_000_000]);
clockTrace('four-tick-cap-without-drop', 0, [199_999_999, 200_000_000, 400_000_000, 450_000_000]);
clockTrace('four-tick-cap-exact-drop-boundary', 0, [200_000_000, 249_999_999, 250_000_000]);
clockTrace('backwards-host-time-does-not-rewind-due', 500_000_000, [499_999_999, 400_000_000, 500_000_000, 450_000_000, 550_000_000]);
clockTrace('nonzero-initial-clock-and-long-stall', 123_456_789, [123_456_789, 173_456_788, 173_456_789, 2_000_000_000, 2_050_000_000]);
await loadReference(reference);
const output = JSON.stringify({ format: 1, reference: REFERENCE_PIN, node: process.version, arch: process.arch, meters, clocks }) + '\n';
const destination = `${root}/rust/crates/server/tests/fixtures/tick-timing.json`;
if (process.argv.includes('--check')) assert.equal(readFileSync(destination, 'utf8'), output);
else writeFileSync(destination, output);
console.log(`${process.argv.includes('--check') ? 'Checked' : 'Recorded'} ${meters.length} meter traces/${meters.reduce((n,t)=>n+t.steps.length,0)} samples and ${clocks.length} clock traces/${clocks.reduce((n,t)=>n+t.steps.length,0)} polls`);
