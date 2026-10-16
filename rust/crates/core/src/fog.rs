//! Fog of war: the ground each team sees, one byte per cell. A port of `teamFog` in `shared/sim.js` at the
//! pin, with `teamCells`, `fogTerrain`, `fogSource`, `fogLos` and their helpers, including the kept cells
//! of sources that stand still. Its answers match the JavaScript exactly (`harness/vision-check.mjs`).
//!
//! The rule is the vision pass's applied to every cell centre: anything within 6 m, a building's or an
//! airborne plane's whole vision circle, a live recon corridor, otherwise the high-ground and garrison
//! range with line of sight; plus the cells under the enemy ground units the team sees. The shortcuts only
//! skip work that cannot change the answer.
//!
//! Not ported yet: World Conquest's list of visible cells (`f.visibleCells`).
//!
//! The JavaScript keeps these caches on the game object; here they live in `Fog`, beside the `Game`, so
//! the teams can be worked out on separate threads while the game is only read.

use std::collections::BTreeMap;

use crate::existing_observers::ObserverOverlay;
use crate::game::{CELL, Game, SIGHT, Smoke, StrikeKind, Team, Unit};
use crate::grid::UnitId;
use crate::math;
use crate::rules::Rules;
use crate::vision::{airborne, in_strip, los, or_one, step_of, vision_of, vision_range};

/// Blocks of 8 x 8 cells for the highest-ground lookup and the change stamps (`FOG_BLOCK`).
const FOG_BLOCK: i64 = 3;
/// Anything this close is seen.
const CLOSE: f64 = 6.0;

/// Tables over the terrain for the fog pass, rebuilt when the terrain changes (`g.fogTerrain`).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FogTerrain {
    /// The `Game::terrain_version` these tables describe.
    pub version: u32,
    /// Sight blockers summed over every box from the map's corner, `w + 1` by `h + 1`.
    pub sat: Vec<i32>,
    /// `w + 1`, the summed table's row length.
    pub sat_w: usize,
    /// The highest ground in each block.
    pub top: Vec<i8>,
    /// Blocks per row.
    pub bw: usize,
    /// 1 where a cell blocks sight.
    pub sight: Vec<u8>,
    /// The height levels these tables were built from.
    pub height: Option<Vec<i8>>,
    /// Per block, the terrain version at which sight last changed there.
    pub stamp: Vec<u32>,
}

impl FogTerrain {
    /// The tables for the game's terrain now (`fogTerrain`): `old` when it is current, else rebuilt, with
    /// the blocks where sight changed since `old` stamped with the new version.
    pub fn current(old: Option<FogTerrain>, g: &Game, rules: &Rules) -> FogTerrain {
        let version = g.terrain_version;
        if let Some(old) = old {
            if old.version == version {
                return old;
            }
            return Self::build(Some(old), g, rules);
        }
        Self::build(None, g, rules)
    }

    fn build(old: Option<FogTerrain>, g: &Game, rules: &Rules) -> FogTerrain {
        let (w, h, version) = (g.w, g.h, g.terrain_version);
        let sat_w = w + 1;
        let n = w * h;
        let mut sat = vec![0i32; sat_w * (h + 1)];
        let mut sight = vec![0u8; n];
        for y in 0..h {
            let mut row = 0;
            for x in 0..w {
                let c = y * w + x;
                sight[c] = u8::from(g.flags[c] & SIGHT != 0);
                row += i32::from(sight[c]);
                sat[(y + 1) * sat_w + x + 1] = sat[y * sat_w + x + 1] + row;
            }
        }
        let bw = (w >> FOG_BLOCK) + 1;
        let blocks = bw * ((h >> FOG_BLOCK) + 1);
        let mut top = vec![rules.vision.min_level; blocks];
        let height = g.height.clone();
        let (old_sight, old_height, mut stamp) = match old {
            Some(old) => (Some(old.sight), old.height, old.stamp),
            None => (None, None, vec![0u32; blocks]),
        };
        for c in 0..n {
            let b = ((c / w) >> FOG_BLOCK) * bw + ((c % w) >> FOG_BLOCK);
            let level = height.as_ref().map_or(0, |hh| hh[c]);
            if level > top[b] {
                top[b] = level;
            }
            // the blocks where sight changed: sources looking over them work their cells out again
            if let Some(old_sight) = &old_sight {
                let old_level = old_height.as_ref().map_or(0, |hh| hh[c]);
                if sight[c] != old_sight[c] || level != old_level {
                    stamp[b] = version;
                }
            }
        }
        FogTerrain {
            version,
            sat,
            sat_w,
            top,
            bw,
            sight,
            height,
            stamp,
        }
    }

    /// Sight blockers in the box between two cells, from the summed table (`sightIn`).
    fn sight_in(&self, ax: i64, ay: i64, bx: i64, by: i64) -> i32 {
        let (x0, x1) = if ax < bx { (ax, bx) } else { (bx, ax) };
        let (y0, y1) = if ay < by { (ay, by) } else { (by, ay) };
        let (s, w) = (&self.sat, self.sat_w as i64);
        let at = |i: i64| s[i as usize];
        at((y1 + 1) * w + x1 + 1) - at(y0 * w + x1 + 1) - at((y1 + 1) * w + x0) + at(y0 * w + x0)
    }
}

/// The highest value in the blocks covering a box of cells (`blockMax`), as the reference computes it:
/// -Infinity for no block, NaN when a block lies outside the list.
fn block_max<T: Copy + Into<f64>>(
    bw: usize,
    list: &[T],
    x0: i64,
    y0: i64,
    x1: i64,
    y1: i64,
) -> f64 {
    let mut top = f64::NEG_INFINITY;
    for by in (y0 >> FOG_BLOCK)..=(y1 >> FOG_BLOCK) {
        for bx in (x0 >> FOG_BLOCK)..=(x1 >> FOG_BLOCK) {
            let i = by * bw as i64 + bx;
            let v = usize::try_from(i)
                .ok()
                .and_then(|i| list.get(i))
                .map_or(f64::NAN, |&v| v.into());
            top = math::max(top, v);
        }
    }
    top
}

/// `Math.hypot(dx, dz) <= r` (or `< r` when strict), from the squared distance `q` except within a hair of
/// the edge (`within`).
fn within(q: f64, dx: f64, dz: f64, r: f64, strict: bool) -> bool {
    let r2 = r * r;
    if q < r2 * (1.0 - 1e-9) {
        return true;
    }
    if q > r2 * (1.0 + 1e-9) {
        return false;
    }
    let d = math::hypot(dx, dz);
    if strict { d < r } else { d <= r }
}

/// How far a source's fog reaches (`fogReach`).
fn fog_reach(g: &Game, rules: &Rules, u: &Unit) -> f64 {
    let kind = &rules.kinds[u.kind];
    if u.air.is_some() {
        return kind.vision;
    }
    let building = if kind.building {
        vision_of(g, rules, u)
    } else {
        0.0
    };
    math::max(math::max(CLOSE, vision_range(g, rules, u)), building)
}

/// The smoke clouds within a source's reach (`smokesNear`).
fn smokes_near<'g>(g: &'g Game, u: &Unit, reach: f64) -> Vec<&'g Smoke> {
    g.smokes
        .iter()
        .filter(|s| math::hypot(s.x - u.x, s.z - u.z) < reach + s.r)
        .collect()
}

/// A ground source standing still keeps the cells it saw, until it moves, its range changes, sight changes
/// in its box or the smoke clouds near it change (`g.fogSources`).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct KeptSource {
    pub x: f64,
    pub z: f64,
    pub garrison: Option<usize>,
    /// `None` until the source has stood still for one pass.
    pub seen: Option<KeptCells>,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct KeptCells {
    /// The cells it saw, in row order.
    pub cells: Vec<u32>,
    /// The terrain version they were worked out at.
    pub version: u32,
    /// The box of cells it looked over: x0, y0, x1, y1.
    pub bounds: [i64; 4],
    /// The smoke clouds near it then, by identity.
    pub smokes: Vec<u32>,
    pub reach: f64,
    pub range: f64,
}

/// One team's fog (`g.fog.get(team)`).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TeamFog {
    /// The vision pass this fog follows (`g.visionTick ?? -1`); `None` before the first.
    pub key: Option<i64>,
    /// Goes up when `vis` changes.
    pub version: u32,
    /// 1 where the team sees the cell now.
    pub vis: Vec<u8>,
    /// The buffer the next pass fills (the two swap, so a pass allocates nothing).
    pub next: Vec<u8>,
    /// 1 where the team has ever seen the cell.
    pub explored: Vec<u8>,
    /// Packed changes to `vis`, by the version they start from, in insertion order (`deltas`).
    pub deltas: Vec<(u32, String)>,
}

impl TeamFog {
    pub fn new(cells: usize) -> TeamFog {
        TeamFog {
            key: None,
            version: 0,
            vis: vec![0; cells],
            next: vec![0; cells],
            explored: vec![0; cells],
            deltas: Vec::new(),
        }
    }

    /// The packed change to `vis` from `version`, if one is kept.
    pub fn delta(&self, version: u32) -> Option<&str> {
        self.deltas
            .iter()
            .find(|(v, _)| *v == version)
            .map(|(_, d)| d.as_str())
    }

    /// Works this team's fog out again and records what changed (the body of `teamFog`).
    fn refresh(
        &mut self,
        g: &Game,
        rules: &Rules,
        terrain: &FogTerrain,
        kept: &mut BTreeMap<UnitId, KeptSource>,
        team_key: (Team, i64),
        overlay: ObserverOverlay<'_, '_>,
    ) {
        let (team, key) = team_key;
        self.next.fill(0);
        team_cells(g, rules, terrain, kept, team, &mut self.next, overlay);
        self.key = Some(key);
        // one sweep: the cells that flipped since the last version (packed as packRuns does) and the
        // explored ones
        let n = self.vis.len();
        let mut codes = String::new();
        let (mut last, mut on) = (0, false);
        for c in 0..n {
            let v = self.next[c];
            if (v != self.vis[c]) != on {
                put_run(&mut codes, c - last);
                last = c;
                on = !on;
            }
            self.explored[c] |= v;
        }
        if on {
            put_run(&mut codes, n - last);
        }
        if !codes.is_empty() {
            std::mem::swap(&mut self.vis, &mut self.next);
            self.deltas.clear();
            self.deltas.push((self.version, codes));
            self.version += 1;
        }
    }
}

/// URL-safe base64 digits for the packed runs (`RUN_DIGITS`).
const RUN_DIGITS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// One run length as a little-endian base-32 varint (`putRun`).
fn put_run(codes: &mut String, mut v: usize) {
    while v >= 32 {
        codes.push(char::from(RUN_DIGITS[32 | (v & 31)]));
        v /= 32;
    }
    codes.push(char::from(RUN_DIGITS[v]));
}

/// The fog caches: terrain tables, kept sources and every team's fog.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Fog {
    pub terrain: Option<FogTerrain>,
    pub kept: BTreeMap<UnitId, KeptSource>,
    pub teams: BTreeMap<Team, TeamFog>,
}

impl Fog {
    pub(crate) fn validate_shape(&self, game: &Game) -> Result<(), String> {
        let n = game.w.checked_mul(game.h).ok_or("invalid fog dimensions")?;
        let sat_w = game.w.checked_add(1).ok_or("invalid fog dimensions")?;
        let sat_len = sat_w
            .checked_mul(game.h.checked_add(1).ok_or("invalid fog dimensions")?)
            .ok_or("invalid fog dimensions")?;
        let bw = (game.w >> FOG_BLOCK) + 1;
        let blocks = bw
            .checked_mul((game.h >> FOG_BLOCK) + 1)
            .ok_or("invalid fog dimensions")?;
        if self
            .teams
            .values()
            .any(|f| f.vis.len() != n || f.next.len() != n || f.explored.len() != n)
            || self.terrain.as_ref().is_some_and(|t| {
                t.sight.len() != n
                    || t.sat_w != sat_w
                    || t.sat.len() != sat_len
                    || t.bw != bw
                    || t.top.len() != blocks
                    || t.stamp.len() != blocks
                    || t.height.as_ref().is_some_and(|h| h.len() != n)
            })
            || self.kept.values().any(|k| {
                !k.x.is_finite()
                    || !k.z.is_finite()
                    || k.garrison.is_some_and(|c| c >= n)
                    || k.seen.as_ref().is_some_and(|s| {
                        !s.range.is_finite()
                            || !s.reach.is_finite()
                            || s.cells.iter().any(|c| *c as usize >= n)
                    })
            })
        {
            return Err("invalid owned fog cache shape".into());
        }
        Ok(())
    }
    /// A team's fog for the last vision pass, worked out the first time it is asked for after that pass
    /// (`teamFog`).
    pub fn team(&mut self, g: &Game, rules: &Rules, team: Team) -> &TeamFog {
        self.team_with_observer(g, rules, team, None)
    }
    /// The draft contributes only to a missing or stale team cache. Stage these
    /// component changes until publication; an existing key is never invalidated.
    pub fn team_with_observer(
        &mut self,
        g: &Game,
        rules: &Rules,
        team: Team,
        observer: Option<crate::staged_observer::StagedObserver<'_>>,
    ) -> &TeamFog {
        let overlay = observer.map_or(ObserverOverlay::None, |s| ObserverOverlay::Added(s.unit()));
        self.team_with_overlay(g, rules, team, overlay)
    }

    /// Existing exits replace their canonical sight rows only during a cold pass.
    /// A same-key team cache retains the source's original frame unchanged.
    pub(crate) fn team_with_overlay(
        &mut self,
        g: &Game,
        rules: &Rules,
        team: Team,
        overlay: ObserverOverlay<'_, '_>,
    ) -> &TeamFog {
        let key = g.vision_tick.unwrap_or(-1);
        let n = g.w * g.h;
        let fresh = self.teams.get(&team).is_some_and(|f| f.key == Some(key));
        if !fresh {
            self.prepare(g, rules, overlay);
            let terrain = self.terrain.as_ref().expect("prepared");
            let f = self.teams.entry(team).or_insert_with(|| TeamFog::new(n));
            f.refresh(g, rules, terrain, &mut self.kept, (team, key), overlay);
        }
        self.teams.entry(team).or_insert_with(|| TeamFog::new(n))
    }

    /// Like `team` for every team in `teams`, one thread per team. Each unit's kept cells belong to its
    /// own team's pass, so the teams share nothing they write; the results are merged back in team order
    /// and equal what `team` gives one team after another.
    pub fn teams_threaded(&mut self, g: &Game, rules: &Rules, teams: &[Team]) {
        let key = g.vision_tick.unwrap_or(-1);
        let n = g.w * g.h;
        let mut todo: Vec<Team> = Vec::new();
        for &team in teams {
            let fresh = self.teams.get(&team).is_some_and(|f| f.key == Some(key));
            if !fresh && !todo.contains(&team) {
                todo.push(team);
            }
        }
        if todo.is_empty() {
            return;
        }
        self.prepare(g, rules, ObserverOverlay::None);
        let mut parts: Vec<BTreeMap<UnitId, KeptSource>> = vec![BTreeMap::new(); todo.len()];
        let mut rest = BTreeMap::new();
        for (id, e) in std::mem::take(&mut self.kept) {
            let team = g.team_of(&g.units[&id]);
            match todo.iter().position(|&t| t == team) {
                Some(i) => parts[i].insert(id, e),
                None => rest.insert(id, e),
            };
        }
        let mut fogs: Vec<TeamFog> = todo
            .iter()
            .map(|team| self.teams.remove(team).unwrap_or_else(|| TeamFog::new(n)))
            .collect();
        let terrain = self.terrain.as_ref().expect("prepared");
        std::thread::scope(|scope| {
            for ((fog, kept), &team) in fogs.iter_mut().zip(parts.iter_mut()).zip(&todo) {
                scope.spawn(move || {
                    fog.refresh(g, rules, terrain, kept, (team, key), ObserverOverlay::None)
                });
            }
        });
        for ((fog, kept), team) in fogs.into_iter().zip(parts).zip(todo) {
            self.teams.insert(team, fog);
            rest.extend(kept);
        }
        self.kept = rest;
    }

    /// Brings the terrain tables up to date and forgets the kept cells of units that are gone (the start of
    /// `teamCells`).
    fn prepare(&mut self, g: &Game, rules: &Rules, overlay: ObserverOverlay<'_, '_>) {
        self.terrain = Some(FogTerrain::current(self.terrain.take(), g, rules));
        self.kept.retain(|id, _| {
            g.units.contains_key(id) || overlay.added().is_some_and(|u| u.id == *id)
        });
    }
}

/// Fills `vis` (all zeros coming in) with 1 where the team sees the cell now (`teamCells`).
fn team_cells(
    g: &Game,
    rules: &Rules,
    f: &FogTerrain,
    kept: &mut BTreeMap<UnitId, KeptSource>,
    team: Team,
    vis: &mut [u8],
    overlay: ObserverOverlay<'_, '_>,
) {
    let (w, h) = (g.w as i64, g.h as i64);
    let mut moving: Vec<&Unit> = Vec::new();
    // A ground source standing still since the last pass keeps its seen cells until sight changes in its
    // box (a terrain edit or a smoke cloud coming or going) or its range does (rain). Moving ones skip the
    // cells the team already sees.
    for u in overlay.units(g) {
        if g.team_of(u) != team || (u.air.is_some() && !airborne(u)) {
            continue;
        }
        if u.air.is_some() {
            moving.push(u);
            continue;
        }
        let still = kept
            .get(&u.id)
            .is_some_and(|e| e.x == u.x && e.z == u.z && e.garrison == u.garrison);
        if !still {
            let fresh = KeptSource {
                x: u.x,
                z: u.z,
                garrison: u.garrison,
                seen: None,
            };
            kept.insert(u.id, fresh);
            moving.push(u);
            continue;
        }
        let e = kept.get_mut(&u.id).expect("kept above");
        if let Some(s) = &e.seen {
            let [x0, y0, x1, y1] = s.bounds;
            if s.range == vision_range(g, rules, u)
                && block_max(f.bw, &f.stamp, x0, y0, x1, y1) <= f64::from(s.version)
                && same_smokes(&s.smokes, &smokes_near(g, u, s.reach))
            {
                for &c in &s.cells {
                    vis[c as usize] = 1;
                }
                continue;
            }
        }
        let mut cells = Vec::new();
        let source = fog_source(g, rules, f, vis, u, Some(&mut cells));
        e.seen = Some(KeptCells {
            cells,
            version: f.version,
            bounds: source.bounds,
            smokes: source.smokes,
            reach: source.reach,
            range: source.range,
        });
    }
    for u in moving {
        fog_source(g, rules, f, vis, u, None);
    }
    let v = &rules.vision;
    let reach = math::hypot(v.recon_len, v.recon_width) / 2.0;
    for s in &g.strikes {
        if !s.live || s.kind != StrikeKind::Recon || g.players[s.owner].team != team {
            continue;
        }
        let y_from = math::max(0.0, ((s.z - reach) / CELL).floor()) as i64;
        let y_to = math::min((h - 1) as f64, ((s.z + reach) / CELL).floor()) as i64;
        let x_from = math::max(0.0, ((s.x - reach) / CELL).floor()) as i64;
        let x_to = math::min((w - 1) as f64, ((s.x + reach) / CELL).floor()) as i64;
        for y in y_from..=y_to {
            for x in x_from..=x_to {
                let c = (y * w + x) as usize;
                if vis[c] == 0
                    && in_strip(
                        s,
                        (x as f64 + 0.5) * CELL,
                        (y as f64 + 0.5) * CELL,
                        v.recon_len,
                        v.recon_width,
                    )
                {
                    vis[c] = 1;
                }
            }
        }
    }
    // the cells under the enemy ground units the team sees, so nothing it is shown stands on fogged ground
    let seen = g
        .players
        .iter()
        .find(|p| p.team == team)
        .map(|p| &p.visible);
    for id in seen.into_iter().flatten() {
        let Some(t) = g.units.get(id).map(|u| overlay.replace(u)) else {
            continue;
        };
        if t.air.is_some() {
            continue;
        }
        match &t.cells {
            Some(cells) => {
                for &c in cells {
                    if let Some(cell) = vis.get_mut(c as usize) {
                        *cell = 1;
                    }
                }
            }
            None => {
                if let Some(c) = g.cell_of(t.x, t.z) {
                    vis[c] = 1;
                }
            }
        }
    }
}

/// Identity comparison of the smoke clouds a kept source saw with the ones near it now (`sameList`).
fn same_smokes(kept: &[u32], now: &[&Smoke]) -> bool {
    kept.len() == now.len() && kept.iter().zip(now).all(|(&id, s)| id == s.id)
}

/// What `fog_source` reports about the source, for its kept cells.
struct SourceBox {
    bounds: [i64; 4],
    smokes: Vec<u32>,
    reach: f64,
    range: f64,
}

/// Marks what one source sees (`fogSource`). With `list`, every cell of its circle is worked out and the
/// seen ones are pushed there; without, cells the team already sees are skipped.
fn fog_source(
    g: &Game,
    rules: &Rules,
    f: &FogTerrain,
    vis: &mut [u8],
    u: &Unit,
    mut list: Option<&mut Vec<u32>>,
) -> SourceBox {
    let kind = &rules.kinds[u.kind];
    let (w, h) = (g.w as i64, g.h as i64);
    let (air, building) = (u.air.is_some(), kind.building);
    let vision = vision_of(g, rules, u);
    let (sx, sz) = (u.x, u.z);
    let range = vision_range(g, rules, u);
    let reach = fog_reach(g, rules, u);
    let x0 = math::max(0.0, ((sx - reach) / CELL).floor()) as i64;
    let x1 = math::min((w - 1) as f64, ((sx + reach) / CELL).floor()) as i64;
    let y0 = math::max(0.0, ((sz - reach) / CELL).floor()) as i64;
    let y1 = math::min((h - 1) as f64, ((sz + reach) / CELL).floor()) as i64;
    let ux = (sx / CELL).floor() as i64;
    let uy = (sz / CELL).floor() as i64;
    let inside = ux >= 0 && uy >= 0 && ux < w && uy < h;
    let la = g.level_at(sx, sz);
    let src_top = if g.height.is_some() && x0 <= x1 && y0 <= y1 {
        block_max(f.bw, &f.top, x0, y0, x1, y1)
    } else {
        f64::from(rules.vision.min_level)
    };
    let smokes = smokes_near(g, u, reach);
    for y in y0..=y1 {
        // only the stretch of the row that can be within reach (one spare cell each side)
        let pz = (y as f64 + 0.5) * CELL;
        let dz = sz - pz;
        let span = math::max(0.0, reach * reach - dz * dz).sqrt();
        let xa = math::max(x0 as f64, ((sx - span) / CELL).floor() - 1.0) as i64;
        let xb = math::min(x1 as f64, ((sx + span) / CELL).floor() + 1.0) as i64;
        for x in xa..=xb {
            let c = (y * w + x) as usize;
            if vis[c] != 0 && list.is_none() {
                continue;
            }
            let px = (x as f64 + 0.5) * CELL;
            let dx = sx - px;
            let q = dx * dx + dz * dz;
            let seen = if air {
                within(q, dx, dz, vision, false)
            } else {
                within(q, dx, dz, CLOSE, true)
                    || (building && within(q, dx, dz, vision, false))
                    || (within(q, dx, dz, range, false)
                        && if inside {
                            fog_los(
                                g,
                                rules,
                                f,
                                (sx, sz),
                                (ux, uy),
                                (x, y),
                                la,
                                src_top,
                                &smokes,
                            )
                        } else {
                            los(g, rules, (sx, sz), (px, pz))
                        })
            };
            if seen {
                vis[c] = 1;
                if let Some(list) = list.as_deref_mut() {
                    list.push(c as u32);
                }
            }
        }
    }
    SourceBox {
        bounds: [x0, y0, x0.max(x1), y0.max(y1)],
        smokes: smokes.iter().map(|s| s.id).collect(),
        reach,
        range,
    }
}

/// `los` from a source to a cell centre with numbers only, and the smoke clouds already narrowed down to
/// the source's reach (`fogLos`). Unlike `los`, woods do not stop it: the reference's fog walk checks only
/// the sight bit.
#[allow(clippy::too_many_arguments)]
fn fog_los(
    g: &Game,
    rules: &Rules,
    f: &FogTerrain,
    (sx, sz): (f64, f64),
    (ux, uy): (i64, i64),
    (x, y): (i64, i64),
    la: i8,
    src_top: f64,
    smokes: &[&Smoke],
) -> bool {
    let px = (x as f64 + 0.5) * CELL;
    let pz = (y as f64 + 0.5) * CELL;
    let (w, h) = (g.w as i64, g.h as i64);
    let start = i32::from(f.sight[(uy * w + ux) as usize]);
    let end = i32::from(f.sight[(y * w + x) as usize]);
    // The grid walk only visits cells inside the box between its ends, and skips its first and last cell.
    // A long diagonal's box is wide, so the two halves' boxes get a second look (unless the midpoint sits
    // on a cell edge).
    if f.sight_in(ux, uy, x, y) - start - end > 0 {
        let (mx, mz) = ((sx + px) / 2.0, (sz + pz) / 2.0);
        let (mcx, mcy) = ((mx / CELL).floor() as i64, (mz / CELL).floor() as i64);
        if mx % CELL == 0.0
            || mz % CELL == 0.0
            || f.sight_in(ux, uy, mcx, mcy) - start > 0
            || f.sight_in(mcx, mcy, x, y) - end > 0
        {
            // clear(g, sx, sz, px, pz, SIGHT, false), unrolled
            let (dx, dz) = (px - sx, pz - sz);
            let (step_x, step_y) = (step_of(dx), step_of(dz));
            let tdx = if dx != 0.0 {
                CELL / dx.abs()
            } else {
                f64::INFINITY
            };
            let tdy = if dz != 0.0 {
                CELL / dz.abs()
            } else {
                f64::INFINITY
            };
            let (mut cx, mut cy) = (ux, uy);
            let mut tx = if dx != 0.0 {
                (if step_x > 0 {
                    (cx + 1) as f64 * CELL - sx
                } else {
                    sx - cx as f64 * CELL
                }) / dx.abs()
            } else {
                f64::INFINITY
            };
            let mut ty = if dz != 0.0 {
                (if step_y > 0 {
                    (cy + 1) as f64 * CELL - sz
                } else {
                    sz - cy as f64 * CELL
                }) / dz.abs()
            } else {
                f64::INFINITY
            };
            let mut k = w + h;
            while k > 0 && !(cx == x && cy == y) {
                if cy == y || (cx != x && tx < ty) {
                    tx += tdx;
                    cx += step_x;
                } else {
                    ty += tdy;
                    cy += step_y;
                }
                if cx == x && cy == y {
                    break;
                }
                if cx < 0
                    || cy < 0
                    || cx >= w
                    || cy >= h
                    || g.flags[(cy * w + cx) as usize] & SIGHT != 0
                {
                    return false;
                }
                k -= 1;
            }
        }
    }
    let (x0, x1) = if ux < x { (ux, x) } else { (x, ux) };
    let (y0, y1) = if uy < y { (uy, y) } else { (y, uy) };
    // same test as los
    if !smokes.is_empty() && math::hypot(px - sx, pz - sz) >= rules.vision.smoke_sight {
        // segHits, unrolled
        let (dx, dz) = (px - sx, pz - sz);
        let l2 = or_one(dx * dx + dz * dz);
        for s in smokes {
            let t = math::max(
                0.0,
                math::min(1.0, ((s.x - sx) * dx + (s.z - sz) * dz) / l2),
            );
            if math::hypot(sx + dx * t - s.x, sz + dz * t - s.z) < s.r {
                return false;
            }
        }
    }
    let Some(height) = &g.height else {
        return true;
    };
    // no ground in the box above the lower eye's own level: the hills can't block
    let lb = height[(y * w + x) as usize];
    let low = f64::from(la.min(lb));
    if src_top <= low || block_max(f.bw, &f.top, x0, y0, x1, y1) <= low {
        return true;
    }
    // hillsClear's samples, read straight from the height grid (both ends are on the map, so every sample
    // is)
    let l = rules.vision.level_height;
    let ya = f64::from(la) * l + rules.vision.eye;
    let yb = f64::from(lb) * l + rules.vision.eye;
    let (dx, dz) = (px - sx, pz - sz);
    let n = (math::hypot(dx, dz) / (CELL / 2.0)).ceil();
    let mut i = 1.0;
    while i < n {
        let t = i / n;
        let at = ((sz + dz * t) / CELL).floor() * w as f64 + ((sx + dx * t) / CELL).floor();
        let ground = usize::try_from(at as i64)
            .ok()
            .filter(|_| at >= 0.0)
            .and_then(|i| height.get(i))
            .map_or(f64::NAN, |&v| f64::from(v));
        if ground * l > ya + (yb - ya) * t {
            return false;
        }
        i += 1.0;
    }
    true
}
