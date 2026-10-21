// One regression: report independent prerequisites after the first strict error.
// Synthetic CPU rows only. No rendered weather or native runtime claim.
import test from 'node:test';
import assert from 'node:assert/strict';
import { auditWeather } from './TCWorld28WeatherAudit.mjs';

test('failed history exposes delta, lift and pacing blockers without changing any row', () => {
  const base = { start: 1, fixedClock: false, graphics: 'low', weather: 'fog', rain: 0, wet: 0,
    wind: [.55, .25], drift: [0, 0], look: Array(9).fill(0), goal: Array(9).fill(0),
    haze: [0, 0, 0], hazeGoal: [0, 0, 0] };
  const start = { ...base, event: 'start', frame: 0, engineFrame: 1, tick: 0, time: 0, dt: 0, renderSeconds: 0 };
  const frames = [{ tick: 300, dt: .4 }, { tick: 4600, dt: .05 }, { tick: 4802, dt: .05, weather: 'clear' }];
  let seconds = 0;
  const lines = ['atmos-history ' + JSON.stringify(start)];
  frames.forEach((row, i) => {
    seconds += row.dt;
    const r = { ...base, event: 'frame', frame: i + 1, engineFrame: i + 2, ...row, time: seconds, renderSeconds: seconds };
    lines.push('atmos-history ' + JSON.stringify(r));
    lines.push('atmos-view ' + JSON.stringify({ start: 1, frame: r.frame, engineFrame: r.engineFrame,
      eyeCm: [0, 0, 100], forward: [1, 0, 0], right: [0, 1, 0], up: [0, 0, 1],
      focus: [0, 0], distance: 60, yaw: 0, hFov: 60, viewport: [1280, 720] }));
  });
  const log = lines.join('\n');
  const messages = [{ t: 'start', map: { name: 'Pegasus Bridge' }, weather: ['fog'] },
    ...[300, 4600, 4800, 4802].map(tick => ({ t: 's', tick }))];
  const before = JSON.stringify(messages), result = auditWeather(log, messages);
  assert.equal(result.counts.cameraJoins, 3);
  assert.equal(result.counts.deltasAboveOriginalCap, 1);
  for (const code of ['original-delta-cap', 'exact-natural-lift-tick', 'natural-recipient-pacing'])
    assert.ok(result.blockers.some(b => b.code === code), code);
  assert.equal(result.history.firstClear.tick, 4802);
  assert.equal(result.strict.accepted, false);
  assert.equal(result.cpuPlanReady, false);
  assert.equal(result.matchedHistoryQualified, false);
  assert.equal(JSON.stringify(messages), before);
  assert.equal(lines.join('\n'), log);
});
