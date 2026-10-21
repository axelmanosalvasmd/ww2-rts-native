// The verification driver's policy, from a seat's snapshot only. Ported to C++ in Client/TCTutorial.cpp (the
// -TCTutorialDrive switch). Cells as shared/tutorial.js.
const STRUCT = new Set(['hq', 'bunker', 'depot', 'barracks', 'motorpool', 'airfield']);
export function decide(s, m, CELL, me = 0) {
  const out = [], mode = s.mode; if (!mode || mode.kind !== 'tutorial' || s.winner != null) return out;
  const t = s.tick;
  const at = (x, y) => [(x + 0.5) * CELL, (y + 0.5) * CELL];
  const rows = s.units ?? [];
  const mine = rows.filter(u => u[2] === me && !STRUCT.has(u[1]) && u[7] > 0);
  const foes = rows.filter(u => u[2] !== me && !STRUCT.has(u[1]) && u[7] > 0);
  if (m.step !== mode.step) { m.step = mode.step; m.sent = -1e9; m.once = {}; m.stepAt = t; }
  const every = (n) => { if (t - m.sent < n) return false; m.sent = t; return true; };
  const spread = (ids, x, y) => ids.map((id, i) => { const [px, pz] = at(x + (i % 3) - 1, y + Math.floor(i / 3) * 2 - 1); return [id, px, pz]; });
  const near = (u, list) => list.slice().sort((a, b) => Math.hypot(a[3] - u[3], a[4] - u[4]) - Math.hypot(b[3] - u[3], b[4] - u[4]))[0];
  const ids = (list) => list.map(u => u[0]);
  const attackSeen = (who) => {
    if (!foes.length || !who.length) return false;
    const target = near(who[0], foes);
    out.push({ t: 'attack', ids: ids(who), target: target[0] });
    return true;
  };
  // each rifle squad off cooldown throws at the nearest enemy (aimed: the server walks it into range); the rest attack
  const grenades = () => {
    m.nade ??= {};
    for (const u of mine.filter(u => u[1] === 'rifle' && u[11] === 0 && !(t - (m.nade[u[0]] ?? -1e9) < 160))) { const f = near(u, foes); m.nade[u[0]] = t; out.push({ t: 'ability', ids: [u[0]], x: f[3], z: f[4] }); }
    attackSeen(mine.filter(u => !(t - (m.nade[u[0]] ?? -1e9) < 160)));
  };
  switch (mode.step) {
    case 0: if (every(100)) out.push({ t: 'move', orders: spread(ids(mine), 16, 22) }); break;
    case 1: if (every(40) && !attackSeen(mine)) out.push({ t: 'amove', orders: spread(ids(mine), 23, 22) }); break;
    case 2: if (every(100)) out.push({ t: 'move', orders: spread(ids(mine), 34, 22) }); break;
    case 3: if (!m.once.buy) { m.once.buy = 1; out.push({ t: 'buy', unit: 'at' }); } break;
    case 4: {
      if (!every(30)) break;
      const at_ = mine.filter(u => u[1] === 'at'), rest = mine.filter(u => u[1] !== 'at');
      if (!m.once.place) { m.once.place = 1; out.push({ t: 'move', orders: spread(ids(rest), 28, 26) }); }
      if (!at_.length) break; // still in training
      if (!attackSeen(at_)) out.push({ t: 'amove', orders: spread(ids(at_), 40, 22) });
      break;
    }
    case 5: {
      if (!every(30)) break;
      if (!foes.length) { out.push({ t: 'amove', orders: spread(ids(mine), 41, 22) }); break; }
      grenades();
      break;
    }
    case 6: if (!m.once.retreat && mine.length) { m.once.retreat = 1; out.push({ t: 'retreat', ids: [mine.find(u => u[1] === 'rifle')?.[0] ?? mine[0][0]] }); } break;
    case 7: if (every(40)) { const [x, z] = at(63, 17); out.push({ t: 'support', kind: 'artillery', x, z }); } break;
    case 8: {
      if (t - m.stepAt < 200 || !every(40)) break; // the barrage lands first
      if (!foes.length) { out.push({ t: 'amove', orders: spread(ids(mine), 62, 22) }); break; }
      grenades(); // a squad left in the stone house shrugs off bullets
      break;
    }
  }
  return out;
}
