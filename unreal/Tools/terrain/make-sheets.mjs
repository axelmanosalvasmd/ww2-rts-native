// Side-by-side sheets of the painted ground: the reference's canvas as the browser painted it (make-ground-tiles.mjs
// --cases, <case>.<k>.browser.rgba) | the native canvas from the harness (<case>.<k>.native.ppm, Native knowledge mode,
// the shipped tiles) | their difference times 8. Crops are centred on the busiest crater spot of the case (else the map
// centre), at the canvas's own pixels. ImageMagick does the image work.
//   node make-sheets.mjs --oracle <dir> --tiles <dir> --harness <dir> --out <dir> --cases a.k,b.k [--size 640]
import { readFileSync, existsSync, mkdirSync, writeFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { join } from 'node:path';
const arg = (n, d) => { const i = process.argv.indexOf(`--${n}`); return i > 0 ? process.argv[i + 1] : d; };
const oracle = arg('oracle'), tiles = arg('tiles'), harness = arg('harness'), out = arg('out'), size = +arg('size', 640);
mkdirSync(out, { recursive: true });
const summary = JSON.parse(readFileSync(join(oracle, 'summary.json'), 'utf8'));
const magick = (...a) => execFileSync('magick', a, { maxBuffer: 1 << 26 });
for (const spec of arg('cases').split(',')) {
  const [name, ks] = spec.split('.'), k = +ks;
  const c = summary.cases.find(x => x.name === name && x.k === k);
  const CW = c.w * c.P, CH = c.h * c.P;
  const replay = JSON.parse(readFileSync(join(oracle, name + '.replay.json'), 'utf8'));
  const W = replay.start.map.w;
  const rows = [...replay.start.cells, ...replay.steps.slice(0, k).flat()];
  const hot = rows.filter(r => r[1] === '+' || ((r[3] ?? 0) >> 5 & 3)).map(r => [r[0] % W, Math.floor(r[0] / W)]);
  let t = [c.w / 2, c.h / 2], best = -1;
  for (const [x, y] of hot) { const n = hot.filter(([a, b]) => Math.hypot(a - x, b - y) < 8).length; if (n > best) { best = n; t = [x + 0.5, y + 0.5]; } }
  const s = Math.min(size, CW, CH), x0 = Math.max(0, Math.min(CW - s, Math.round(t[0] * c.P - s / 2))), y0 = Math.max(0, Math.min(CH - s, Math.round(t[1] * c.P - s / 2)));
  const crop = `${s}x${s}+${x0}+${y0}`, tmp = join(out, `.${name}.${k}`);
  const browser = join(tiles, `${name}.${k}.browser.rgba`), native = join(harness, `${name}.${k}.native.ppm`);
  if (!existsSync(browser) || !existsSync(native)) { console.log('skip', spec); continue; }
  magick('-size', `${CW}x${CH}`, '-depth', '8', `rgba:${browser}`, '-alpha', 'off', '-crop', crop, '+repage', tmp + '-b.png');
  magick(native, '-crop', crop, '+repage', tmp + '-n.png');
  magick(tmp + '-b.png', tmp + '-n.png', '-compose', 'difference', '-composite', '-evaluate', 'multiply', '8', tmp + '-d.png');
  const file = join(out, `paint-${name}-step${k}-browser-vs-native-vs-diff8.png`);
  const label = `${name} paint ${k} (${c.w}x${c.h} cells, ${c.P} px a cell, crop ${s} px at ${x0},${y0}): reference browser | native (Native knowledge) | difference x8`;
  magick(tmp + '-b.png', tmp + '-n.png', tmp + '-d.png', '+append', '(', '-size', `${s * 3}x30`, 'xc:#202020', '-fill', 'white', '-pointsize', '16', '-annotate', '+8+21', label, ')', '-swap', '0,1', '-append', file);
  console.log(file);
}
