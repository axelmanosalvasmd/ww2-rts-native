// the oracle's case JSON -> the harness's text input (relief-harness.cpp)
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
const [dir, name, out] = process.argv.slice(2);
const load = (n) => JSON.parse(readFileSync(join(dir, `${n}.json`), 'utf8'));
const meta = load(name), n = meta.w * meta.h;
const lines = (m) => [m.levels.join(' '), m.grid, m.roads, m.known ?? '1'.repeat(n), m.frozen.join(' ')];
const text = [`${meta.w} ${meta.h} ${meta.low ? 1 : 0} ${meta.base ? 1 : 0}`, ...lines(meta)];
if (meta.base) {
  const base = load(meta.base);
  // relief.js keeps the frozen lines of the base (createWaterLevels runs once)
  text.push(...lines(base), `${meta.edits.length} ${meta.edits.map((e) => e[0]).join(' ')}`);
}
writeFileSync(out, text.join('\n') + '\n');
