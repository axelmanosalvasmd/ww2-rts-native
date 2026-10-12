//! The Rust side of the vision and fog parity check, line for line what `harness/vision-check.mjs` prints,
//! and the timing runs.

use std::collections::BTreeMap;
use std::time::Instant;

use ww2_core::fog::{Fog, KeptSource};
use ww2_core::game::{Game, Team};
use ww2_core::grid::{SpatialGrid, UnitId};
use ww2_core::rules::Rules;
use ww2_core::vision::update_vision;

/// The spatial index's bucket size in the game (`shared/grid.js`).
const GRID_BUCKET: f64 = 16.0;

/// FNV-1a, 32 bits, over bytes.
pub fn fnv(bytes: &[u8]) -> String {
    let mut h: u32 = 0x811c_9dc5;
    for &b in bytes {
        h = (h ^ u32::from(b)).wrapping_mul(0x0100_0193);
    }
    format!("{h:08x}")
}

/// The kept sources in id order, laid out as `keptHash` in `harness/vision-check.mjs` lays them out.
fn kept_hash(kept: &BTreeMap<UnitId, KeptSource>) -> String {
    let mut b: Vec<u8> = Vec::new();
    for (&id, e) in kept {
        b.extend(id.to_le_bytes());
        b.extend(e.x.to_le_bytes());
        b.extend(e.z.to_le_bytes());
        b.extend(e.garrison.map_or(-1, |g| g as i32).to_le_bytes());
        match &e.seen {
            None => b.extend(u32::MAX.to_le_bytes()),
            Some(s) => {
                b.extend((s.cells.len() as u32).to_le_bytes());
                for &c in &s.cells {
                    b.extend(c.to_le_bytes());
                }
                b.extend(s.version.to_le_bytes());
                for &v in &s.bounds {
                    b.extend((v as i32).to_le_bytes());
                }
                b.extend(s.reach.to_le_bytes());
                b.extend(s.range.to_le_bytes());
                b.extend((s.smokes.len() as u32).to_le_bytes());
            }
        }
    }
    fnv(&b)
}

/// The teams in the order their first player appears.
pub fn teams_of(g: &Game) -> Vec<Team> {
    let mut teams = Vec::new();
    for p in &g.players {
        if !teams.contains(&p.team) {
            teams.push(p.team);
        }
    }
    teams
}

/// Four passes as `harness/vision-check.mjs` runs them, printing the same lines. With `threaded`, the fog
/// pass works the teams out on one thread each.
pub fn check(g: &mut Game, fog: &mut Fog, rules: &Rules, threaded: bool) -> Vec<String> {
    let grid = SpatialGrid::new(&g.units, GRID_BUCKET);
    let teams = teams_of(g);
    let mut lines = Vec::new();
    for pass in 1..=4 {
        if pass > 1 {
            g.tick += 4;
        }
        if pass == 4 {
            fog.kept.clear();
            fog.terrain = None;
        }
        update_vision(g, rules, &grid);
        lines.push(format!("pass {pass} tick {}", g.tick));
        for p in &g.players {
            let mut ids = p.visible.clone();
            ids.sort_unstable();
            let mut line = format!("visible {} {}", p.slot, ids.len());
            for id in ids {
                line.push_str(&format!(" {id}"));
            }
            lines.push(line);
        }
        if threaded {
            fog.teams_threaded(g, rules, &teams);
        }
        for &team in &teams {
            let f = fog.team(g, rules, team);
            let delta = f
                .version
                .checked_sub(1)
                .and_then(|v| f.delta(v))
                .map_or_else(|| "-".to_string(), |d| fnv(d.as_bytes()));
            lines.push(format!(
                "fog {team} vis {} explored {} version {} delta {delta}",
                fnv(&f.vis),
                fnv(&f.explored),
                f.version
            ));
        }
        lines.push(format!("kept {} {}", fog.kept.len(), kept_hash(&fog.kept)));
    }
    lines
}

fn stats(values: &[f64]) -> String {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    let p95 = sorted[(sorted.len() as f64 * 0.95).ceil() as usize - 1];
    format!("{mean:.3} {p95:.3}")
}

/// Times `n` passes with every source worked out again (as when every unit moves) and `n` with every
/// source standing still, after five untimed ones, with the fog pass on one thread and on one thread per
/// team. Prints mean and p95 milliseconds for the vision pass, the fog pass (all teams) and both.
pub fn bench(g: &mut Game, fog: &mut Fog, rules: &Rules, n: usize) -> Vec<String> {
    let grid = SpatialGrid::new(&g.units, GRID_BUCKET);
    let teams = teams_of(g);
    let mut lines = Vec::new();
    for (name, moving) in [("moving", true), ("still", false)] {
        for threaded in [false, true] {
            let (mut vision, mut fogs, mut both) = (Vec::new(), Vec::new(), Vec::new());
            for i in 0..n + 5 {
                g.tick += 4;
                if moving {
                    fog.kept.clear();
                }
                let t0 = Instant::now();
                update_vision(g, rules, &grid);
                let t1 = Instant::now();
                if threaded {
                    fog.teams_threaded(g, rules, &teams);
                } else {
                    for &team in &teams {
                        fog.team(g, rules, team);
                    }
                }
                let t2 = Instant::now();
                if i >= 5 {
                    let ms = |a: Instant, b: Instant| (b - a).as_secs_f64() * 1000.0;
                    vision.push(ms(t0, t1));
                    fogs.push(ms(t1, t2));
                    both.push(ms(t0, t2));
                }
            }
            let fog_label = if threaded { "fog-threads" } else { "fog" };
            lines.push(format!(
                "bench {name} units {} passes {n} vision {} {fog_label} {} both {} (ms mean p95)",
                g.units.len(),
                stats(&vision),
                stats(&fogs),
                stats(&both)
            ));
        }
    }
    lines
}
