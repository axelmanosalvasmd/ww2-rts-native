// Test-only served comparison copy. Never write the result into reference/.
// Stop automatic atmosphere updates, then replay captured inputs through the
// original start/setWeather/setRain/update bodies before the final draw.
import assert from 'node:assert/strict';
export function instrumentAtmosphereHistory(source) {
  assert.ok(source.includes('t = (t + dt) % 10000;'), 'natural constructor/update clock required; do not combine with TC_LOOK_TIME');
  const seam = '  return {\n    start, update, setWeather,';
  assert.equal(source.split(seam).length, 2, 'pinned atmosphere API seam');
  return source.replace(seam, `  let tcHistoryStart = null, tcHistoryStarts = 0, tcHistoryReplayed = false;
  return {
    start(args) { tcHistoryStart = args; tcHistoryStarts++; start(args); },
    update: () => {}, // Comparison copy only: no unrecorded animation-loop dt.
    setWeather,
    historyState() {
      return { weather: wxNow, time: t, drift: drift.toArray(), look: EASED.map(k => cur[k]),
        goal: EASED.map(k => goal[k]), haze: hazeNow.toArray(), hazeGoal: hazeGoal.toArray(),
        kind: fall?.userData.kind ?? '', alpha: fallA, banksVisible: banks.visible,
        bankSiteHeights: bankAt.map(s => s.y) };
    },
    replayHistory(rows) {
      if (!tcHistoryStart || tcHistoryStarts !== 1 || tcHistoryReplayed) throw Error('Fresh source construction and one history replay required');
      if (!Array.isArray(rows) || rows.length < 2 || rows[0].event !== 'start' || rows[0].frame !== 0)
        throw Error('Complete start and consumed frame history required');
      const first = rows[0];
      if (!Number.isSafeInteger(first.start) || first.start < 1 || !Number.isFinite(first.time) ||
          !Array.isArray(first.drift) || first.drift.length !== 2 || !first.drift.every(Number.isFinite) ||
          !Object.hasOwn(LOOK, first.weather)) throw Error('Complete start and consumed frame history required');
      if (first.graphics !== (gfx.low ? 'low' : 'high')) throw Error('Source and history graphics lane differ');
      if (first.time !== t || first.drift.some((v, i) => Math.abs(v - drift.toArray()[i]) > 1e-12))
        throw Error('Constructor clock/drift differs; supply the preceding history instead of setting clocks');
      if (tcHistoryStart.weather?.[0] !== first.weather) throw Error('Source and native constructor weather differ');
      // Keep the original start's sites, flights and fall. A second start here
      // would sample the final terrain and replace the initial presentation.
      tcHistoryReplayed = true;
      let frames = 0, seconds = 0;
      for (const row of rows.slice(1)) {
        if (row.event !== 'frame' || row.start !== first.start || row.frame !== ++frames || row.fixedClock)
          throw Error('Missing/reordered frame, multiple starts or a frozen comparison clock');
        if (!Number.isFinite(row.dt) || row.dt < 0 || !Number.isFinite(row.renderSeconds) ||
            !Number.isFinite(row.rain) || !Number.isFinite(row.wet) || row.rain < 0 || row.rain > 1 || row.wet < 0 || row.wet > 1 ||
            !Object.hasOwn(LOOK, row.weather) || !Array.isArray(row.wind) || row.wind.length !== 2 || !row.wind.every(Number.isFinite))
          throw Error('Invalid frame input');
        if (row.dt > 0.1) throw Error('Native delta exceeds source normal-frame cap; retain as a caller diagnostic, never clamp it silently');
        if (row.graphics !== first.graphics) throw Error('Source and history graphics lane differ');
        this.setWeather([row.weather]); this.setRain(row.rain, row.wet);
        wind.x = row.wind[0]; wind.z = row.wind[1];
        update(row.dt); seconds += row.dt;
        if (Math.abs(seconds - row.renderSeconds) > 1e-8) throw Error('Consumed dt does not cover the supplied history');
      }
      return { frames, seconds, ...this.historyState() };
    },`);
}

// A raw native log is optional input to the lead's immutable capture driver.
// Partial/reordered histories are rejected by replayHistory, never warmed using
// a guessed tick, a wall-clock sleep or a fabricated weather state.
export function atmosphereHistoryRows(log) {
  return log.split('\n').filter(l => l.includes('atmos-history ')).map(l => JSON.parse(l.slice(l.indexOf('atmos-history ') + 14)));
}
