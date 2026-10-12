//! The Rust side of issue #2's frozen workload (`harness/sim-workload.mjs` runs the JavaScript side): a frozen
//! state stepped with what the core has ported, in the reference's order. Each tick: rebuild the spatial
//! index; on every fourth tick the vision pass; for every unit that reaches the movement block, in id order,
//! the timers it reads and the block; crowd separation and the infantry progress count; then, after a vision
//! tick, every team's fog (as a snapshot after it would ask).
//!
//! With `threads`, the vision pass and the fog pass work one team per thread (results merged in team order,
//! so the game is the same). Movement and separation stay on one thread: each unit's step reads where the
//! units before it ended up, and the spatial index's bucket order, so the order decides the outcome.

use std::time::Instant;

use ww2_core::fog::Fog;
use ww2_core::game::Game;
use ww2_core::grid::SpatialGrid;
use ww2_core::movement::{infantry_progress, move_units};
use ww2_core::rules::Rules;
use ww2_core::separation::separate_crowds;
use ww2_core::vision::{update_vision, update_vision_threaded};

use crate::check::{fnv, teams_of};

/// The spatial index's bucket size in the game (`shared/grid.js`).
const GRID_BUCKET: f64 = 16.0;

/// Milliseconds per tick for each part, and hashes of where the run ended.
pub struct Workload {
    pub step: Vec<f64>,
    pub movement: Vec<f64>,
    pub separate: Vec<f64>,
    /// Vision ticks only.
    pub vision: Vec<f64>,
    pub fog: Vec<f64>,
    /// FNV-1a over every unit's id, position, heading and speed, and over every team's fog.
    pub units_hash: String,
    pub fog_hash: String,
}

/// Mean and 95th percentile, as `sim-workload.mjs` works them out.
pub fn stats(values: &[f64]) -> (f64, f64) {
    if values.is_empty() {
        return (f64::NAN, f64::NAN);
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    let p95 = sorted[(sorted.len() as f64 * 0.95).ceil() as usize - 1];
    (mean, p95)
}

fn ms(a: Instant, b: Instant) -> f64 {
    (b - a).as_secs_f64() * 1000.0
}

/// Steps `g` for `ticks` ticks.
pub fn run(g: &mut Game, fog: &mut Fog, rules: &Rules, ticks: usize, threads: bool) -> Workload {
    let teams = teams_of(g);
    let mut grid = SpatialGrid::new(&g.units, GRID_BUCKET);
    let mut w = Workload {
        step: Vec::new(),
        movement: Vec::new(),
        separate: Vec::new(),
        vision: Vec::new(),
        fog: Vec::new(),
        units_hash: String::new(),
        fog_hash: String::new(),
    };
    for _ in 0..ticks {
        let start = Instant::now();
        grid.rebuild(&g.units);
        g.tick += 1;
        let vision_tick = g.tick % 4 == 1;
        let t0 = Instant::now();
        if vision_tick {
            if threads {
                update_vision_threaded(g, rules, &grid);
            } else {
                update_vision(g, rules, &grid);
            }
        }
        let t1 = Instant::now();
        let moved = move_units(g, rules, &mut grid);
        let t2 = Instant::now();
        separate_crowds(g, rules, &mut grid);
        let t3 = Instant::now();
        infantry_progress(g, rules, &moved);
        let stepped = Instant::now();
        if vision_tick {
            if threads {
                fog.teams_threaded(g, rules, &teams);
            } else {
                for &team in &teams {
                    fog.team(g, rules, team);
                }
            }
            w.vision.push(ms(t0, t1));
            w.fog.push(ms(stepped, Instant::now()));
        }
        w.step.push(ms(start, stepped));
        w.movement.push(ms(t1, t2));
        w.separate.push(ms(t2, t3));
    }
    let mut bytes = Vec::new();
    for u in g.units.values() {
        bytes.extend(u.id.to_le_bytes());
        for v in [u.x, u.z, u.rot, u.move_speed] {
            bytes.extend(v.to_le_bytes());
        }
    }
    w.units_hash = fnv(&bytes);
    let mut bytes = Vec::new();
    for &team in &teams {
        bytes.extend_from_slice(&fog.team(g, rules, team).vis);
    }
    w.fog_hash = fnv(&bytes);
    w
}
