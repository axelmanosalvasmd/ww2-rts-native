// node --test unreal/Tools/compare/scene-reference.test.mjs: the native shot lines scene-reference.mjs replays
import test from 'node:test';
import assert from 'node:assert/strict';
import { parseShots, browserYaw } from './scene-reference.mjs';

const line = (k, name, rest) => `[2026.10.14-19.43.00:134][810]LogTC: orders check ${k}/30: screenshot /x/live/scene-gq-1/native/${name}.png | camera (130.00, 64.20) yaw -149.32 distance 60.00 m | tick 629; selected ${rest}; own garrisoned 2`;

test('parses each native shot with its camera, tick and selection', () => {
  const shots = parseShots([line(21, 'scene-queue-markers', '[1]'), 'LogTC: orders check 22/30: log | tick 629; selected [1]', line(26, 'scene-garrison-roofbar', '[2,3]'), line(27, 'empty', '[]')].join('\n'));
  assert.equal(shots.length, 3);
  assert.deepEqual(shots[0], { name: 'scene-queue-markers', x: 130, z: 64.2, yaw: -149.32, dist: 60, tick: 629, selected: [1] });
  assert.deepEqual(shots[1].selected, [2, 3]);
  assert.deepEqual(shots[2].selected, []);
});

test('a shot line without the camera (before the lead patch) is not replayed', () => {
  assert.equal(parseShots('LogTC: orders check 3/49: screenshot /x/a.png | tick 90; selected [1,2,3]').length, 0);
});

test('the browser yaw is -(native yaw + 90) degrees (tour-reference.mjs)', () => {
  assert.equal(browserYaw(-90), -0);
  assert.ok(Math.abs(browserYaw(-135) - Math.PI / 4) < 1e-12);
  // camera.js puts the eye at target + (sin yaw, cos yaw): it must look the native way, (cos Y, sin Y) in (x, z)
  for (const y of [-149.32, -90, 0, 37]) {
    const b = browserYaw(y), r = y * Math.PI / 180;
    assert.ok(Math.abs(-Math.sin(b) - Math.cos(r)) < 1e-12 && Math.abs(-Math.cos(b) - Math.sin(r)) < 1e-12);
  }
});
