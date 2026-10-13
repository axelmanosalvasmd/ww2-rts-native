// Record engineering recipient fields from unchanged pinned constructors, commands and full steps.
import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import {readFileSync, writeFileSync} from 'node:fs';
import {resolve} from 'node:path';
import {fileURLToPath, pathToFileURL} from 'node:url';
import {gzipSync, gunzipSync} from 'node:zlib';

assert.equal(process.arch, 'x64');
assert.equal(process.version, 'v24.21.0');
assert.ok(process.argv.slice(2).every(arg => arg === '--check'));
const root = fileURLToPath(new URL('../', import.meta.url));
const reference = resolve(process.env.WW2_REFERENCE ?? `${root}/reference/ww2-rts`);
const pin = 'e4e579a838aabae3b1d9dadbc17a0a87147bc52d';
const git = args => execFileSync('git', ['-C', reference, ...args], {encoding: 'utf8'}).trim();
const clean = () => {
  assert.equal(git(['rev-parse', 'HEAD']), pin);
  assert.equal(git(['status', '--porcelain', '--untracked-files=no']), '');
};
clean();
const sim = await import(pathToFileURL(resolve(reference, 'shared/sim.js')));
const {rebuildGrid} = await import(pathToFileURL(resolve(reference, 'shared/grid.js')));
const data = new DataView(new ArrayBuffer(8));
const bits = value => {
  data.setFloat64(0, value, false);
  return data.getBigUint64(0, false).toString(16).padStart(16, '0');
};
const encode = value => value === undefined ? null : typeof value === 'number' ? {f64: bits(value)}
  : Array.isArray(value) ? value.map(encode)
  : value && typeof value === 'object' ? Object.fromEntries(Object.entries(value).map(([key, v]) => [key, encode(v)]))
  : value;
const point = p => ({x: bits(p.x), z: bits(p.z)});
const segment = j => ({kind: j.kind, ...point(j), dir: bits(j.dir), front: j.front === undefined ? null : bits(j.front)});
const originalRandom = Math.random;
const cases = [];
let ticks = 0;
try {
  function record(name, actions, {defaultMode = false, workers = [[0, 25.03, 25.07], [1, 29.02, 25.04], [2, 65.03, 65.07]]} = {}) {
    let draws = 0;
    Math.random = () => {draws++; return .5;};
    const map = {name: 'Engineering recipient', w: 40, h: 40, rows: Array(40).fill('.'.repeat(40)),
      spawns: [{x: 1, y: 1}, {x: 1, y: 37}, {x: 37, y: 37}], points: [{x: 20, y: 38}]};
    assert.equal(sim.validateMap(map), null);
    const options = {weather: false, tech: false, supply: false, logistics: false, ...(defaultMode ? {} : {mode: 'conquest'})};
    const g = sim.createGame(map, ['Owner', 'Ally', 'Enemy'], false, [0, 0, 1], [0, 0, 1], options);
    assert.equal(g.mode, undefined);
    const creationDraws = draws;
    const aliases = {};
    const spawns = workers.map(([owner, x, z, kind = 'engineer'], i) => {
      const u = sim.internals.spawnUnit(g, owner, kind);
      Object.assign(u, {x, z, rot: .013, aim: .023});
      aliases[`worker${i + 1}`] = u.id;
      return {owner, kind, id: u.id, x, z, rot: .013, aim: .023};
    });
    // Authored quiet scene facts, applied before any observed command or full step.
    for (const u of g.units.values()) Object.assign(u, {holdFire: true, holdPos: true, auto: false, autoRetreat: false});
    g.players.forEach(p => {p.mp = 1000;});
    rebuildGrid(g);
    sim.internals.updateVision(g);
    const setup = {map, names: ['Owner', 'Ally', 'Enemy'], factions: [0, 0, 1], teams: [0, 0, 1], options,
      nativeMode: defaultMode ? 'default' : 'conquest', seed: g.seed >>> 0, creationDraws,
      spawns, aliases, quiet: {holdFire: true, holdPos: true, auto: false, autoRetreat: false}, mp: [1000, 1000, 1000]};
    const states = [];
    const observe = () => {
      const cache = sim.snapshotCache(g);
      const recipients = g.players.map((_, slot) => {
        const snapshot = sim.snapshotFor(g, slot, g.shots, [], cache);
        assert.ok(Array.isArray(snapshot.works));
        return encode(Object.fromEntries(['works', 'units', 'plans', 'orders'].map(key => [key, snapshot[key]])));
      });
      const projects = [...(g.projects?.values() ?? [])].map(p => ({id: p.id, owner: p.owner, fort: p.fort,
        jobs: p.jobs.map(segment), crew: p.crew, active: p.active}));
      const units = [...g.units.values()].map(u => ({id: u.id, owner: u.owner, kind: u.type, hp: bits(u.hp), ...point(u),
        membership: u.entrench?.id ?? null, orders: encode(u.orders), paid: u.dig ? {kind: u.dig.kind, ...point(u.dig),
          dir: bits(u.dig.dir), cells: structuredClone(u.dig.cells), elapsed: bits(u.dig.t), originalCells: u.dig.n ?? null,
          reach: bits(u.dig.reach), project: u.dig.project?.id ?? null,
          remainingProjectJobs: u.dig.project?.jobs.length ?? null} : null}));
      states.push({tick: g.tick, recipients, projects, units, projectSequence: g.projectSeq ?? 0,
        players: g.players.map(p => ({mp: bits(p.mp), inc: bits(p.inc)})), story: encode(g.story), randomDraws: draws});
    };
    observe();
    const records = [];
    for (const input of actions) {
      const action = structuredClone(input);
      const before = draws;
      if (action.op === 'command') {
        action.command.ids = action.command.ids.map(id => aliases[id] ?? id);
        const result = sim.command(g, action.slot ?? 0, action.command) ?? null;
        assert.equal(result, action.expected ?? null, `${name}: ${JSON.stringify(action.command)}`);
        observe();
        records.push({action, result, state: states.length - 1});
      } else {
        assert.equal(action.op, 'steps');
        for (let i = 0; i < action.count; i++) {sim.step(g); ticks++; observe();}
        records.push({action, state: states.length - 1});
      }
      assert.equal(draws, before, `${name}: no random gameplay in the quiet scene`);
    }
    const raw = value => value && typeof value === 'object' && !Array.isArray(value) && value.f64 !== undefined
      ? (data.setBigUint64(0, BigInt(`0x${value.f64}`), false), data.getFloat64(0, false))
      : Array.isArray(value) ? value.map(raw) : value && typeof value === 'object'
        ? Object.fromEntries(Object.entries(value).map(([key, v]) => [key, raw(v)])) : value;
    const row = (state, slot, id) => raw(state.recipients[slot].units).find(r => r[0] === aliases[id]);
    if (name.includes('paid-works')) {
      assert.deepEqual(raw(states[3].recipients[0].works).map(r => r[5]), ['trench', 'wire']);
      assert.deepEqual(raw(states[3].recipients[2].works).map(r => r[5]), ['sandbags']);
      assert.equal(row(states[4], 0, 'worker1')[12] & 16384, 16384);
      assert.equal(row(states[4], 1, 'worker1')[12] & (2048 | 4096 | 8192 | 16384), 0);
      assert.equal(row(states[4], 1, 'worker1')[12] & 16, 16);
      assert.equal(raw(states[3].recipients[0].works)[0][2], 25.03);
      assert.notEqual(raw(states[3].recipients[0].works)[0][2], Math.round(25.03 * 10) / 10);
      assert.deepEqual(states[4].recipients, states[5].recipients);
      assert.deepEqual(states[5].recipients, states[6].recipients);
      const dead = states.at(-2);
      assert.deepEqual(raw(dead.recipients[0].works).map(r => r[5]), ['wire']);
      assert.equal(row(dead, 0, 'worker1')[7], 0);
      assert.equal(row(dead, 0, 'worker1')[12] & 16, 16);
      assert.deepEqual(raw(dead.recipients[0].plans).find(r => r[0] === aliases.worker1).slice(1, 4), [7, 25, 25.1]);
      assert.ok(!states.at(-1).units.some(u => u.id === aliases.worker1));
    }
    if (name === 'mixed-strongpoint-and-allied-crew') {
      assert.deepEqual(raw(states[1].recipients[0].works).map(r => r[5]), ['trench', 'trench', 'trench', 'trench', 'wire', 'wire']);
      assert.equal(states[2].projects[0].crew, 2);
      const after = raw(states.at(-1).recipients[0].works);
      assert.deepEqual(after.map(r => r[0]), Array(6).fill(1));
      assert.deepEqual(after.map(r => r[5]), ['trench', 'trench', 'wire', 'wire', 'trench', 'trench']);
    }
    if (name === 'queued-project-behind-paid-work') {
      assert.deepEqual(raw(states[2].recipients[0].works).map(r => r[0]), [1, 1, -1]);
      assert.deepEqual(raw(states[2].recipients[0].orders)[0].slice(2), [7, 29, 35.1]);
      assert.ok(states.some(s => s.units.find(u => u.id === aliases.worker1).paid?.project === 1));
    }
    if (name === 'last-segment-waiting-and-stop') {
      const exhausted = states.find(s => s.projects[0]?.jobs.length === 0 && s.projects[0].active === 1);
      assert.ok(exhausted);
      assert.equal(raw(exhausted.recipients[0].works)[0][0], -1);
      assert.equal(row(exhausted, 0, 'worker1')[12] & (16 | 1024), 16 | 1024);
      assert.equal(row(exhausted, 1, 'worker2')[12] & (16 | 1024), 1024);
      assert.deepEqual(raw(exhausted.recipients[1].plans).find(r => r[0] === aliases.worker2).slice(1, 4), [7, 29, 25]);
      const cleared = states.find(s => s.tick === 140);
      assert.ok(cleared.units.filter(u => [aliases.worker1, aliases.worker2].includes(u.id)).every(u => !u.paid && !u.membership));
      const vanished = states.find(s => s.tick > 160 && !s.projects.some(p => p.id === 2) && raw(s.recipients[1].orders).length);
      assert.ok(vanished, 'queued join preserves its stored anchor after the project disappears');
      assert.deepEqual(raw(vanished.recipients[1].orders)[0].slice(2), [7, 45, 25.1]);
    }
    cases.push({name, setup, records, states});
  }
  const command = (command, slot = 0, expected = null) => ({op: 'command', slot, command, expected});
  const steps = count => ({op: 'steps', count});
  const dig = (id, kind, x, z, dir = .037) => ({t: 'dig', ids: [id], kind, x, z, dir});
  const project = (ids, extra = {}) => ({t: 'entrench', ids, pattern: 'line', x: 25.03, z: 25.07, x2: 41.03, z2: 25.07, ...extra});
  const privateActions = [command(dig('worker1', 'trench', 25.03, 25.07)),
    command(dig('worker2', 'wire', 29.02, 33.04, -.061), 1), command(dig('worker3', 'sandbags', 65.03, 65.07), 2),
    command({t: 'autocast', ids: ['worker1'], on: true}),
    command(dig('worker1', 'unknown', 25, 25), 0, 'blocked'),
    command({...dig('worker1', 'trench', 25, 25), x: 'bad'}, 0, 'blocked'),
    command({t: 'autocast', ids: ['worker1'], on: false}), steps(5),
    command({t: 'destroy', ids: ['worker1']}), steps(1)];
  const privateWorkers = [[0, 25.03, 25.07, 'rifle'], [1, 29.02, 25.04], [2, 65.03, 65.07]];
  record('private-and-allied-paid-works', privateActions, {workers: privateWorkers});
  record('omitted-mode-default-paid-works', privateActions, {defaultMode: true, workers: privateWorkers});
  record('mixed-strongpoint-and-allied-crew', [command(project(['worker1'], {pattern: 'strongpoint', x: 30.03, z: 30.07, x2: 42.03, z2: 30.07})),
    command({t: 'entrench', ids: ['worker2'], join: 1}, 1),
    command(project(['worker3'], {fort: 'wire', x: 65.03, z: 65.07, x2: 73.03, z2: 65.07}), 2), steps(8)]);
  record('queued-project-behind-paid-work', [command(dig('worker1', 'trench', 25.03, 25.07)),
    command(project(['worker1'], {queue: true, x: 25.03, z: 35.07, x2: 41.03, z2: 35.07})), steps(180)], {workers: [[0, 25.03, 25.07]]});
  record('last-segment-waiting-and-stop', [command(project(['worker1'], {x2: 25.03})),
    command({t: 'entrench', ids: ['worker2'], join: 1}, 1), steps(140),
    command(project(['worker1'], {x: 45.03, z: 25.07, x2: 45.03}), 0),
    command(dig('worker2', 'trench', 29.02, 35.04), 1),
    command({t: 'entrench', ids: ['worker2'], join: 2, queue: true}, 1), steps(1),
    command({t: 'stop', ids: ['worker1']}), steps(140)]);
} finally {Math.random = originalRandom;}
clean();
const content = JSON.stringify({pin, cases}) + '\n';
const target = resolve(root, 'rust/crates/simtest/tests/fixtures/recipient-engineering.json.gz');
if (process.argv.includes('--check')) assert.equal(gunzipSync(readFileSync(target)).toString(), content);
else writeFileSync(target, gzipSync(content, {level: 9}));
console.log(`${cases.length} original engineering snapshot cases, ${ticks} full ticks, ${cases.reduce((n, c) => n + c.states.length, 0)} states`);
