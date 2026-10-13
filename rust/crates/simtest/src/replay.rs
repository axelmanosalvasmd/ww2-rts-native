//! The movement port against the reference, call by call: replays a recording from
//! `harness/movement-trace.mjs`. Code the port leaves out changes units between the calls (targeting and
//! facing, reserves, medics, timers), so whole ticks cannot match; each ported call is checked instead.
//!
//! At each tick's start every unit is loaded as recorded and the spatial index rebuilt. Then, in the
//! reference's order: a unit placed outside the movement block is placed (the index follows it); a unit
//! about to move is overwritten with its recorded state just before the block (the index follows it if it
//! moved meanwhile), runs the Rust block, and is compared with its recorded state after it, field by field.
//! Before crowd separation every unit is loaded as recorded again, then separation runs and every unit is
//! compared; then the infantry progress count runs and every unit is compared.

use std::collections::BTreeMap;

use serde_json::Value;
use ww2_core::game::{Game, Unit};
use ww2_core::grid::{SpatialGrid, UnitId};
use ww2_core::movement::{Moved, RouteRequest, infantry_progress, move_unit_with_routes};
use ww2_core::rules::Rules;
use ww2_core::separation::separate_crowds;

use crate::codec::{Result, map_entries, number, short};
use crate::load::{load_game, load_rules, unit};

/// The spatial index's bucket size in the game (`shared/grid.js`).
const GRID_BUCKET: f64 = 16.0;

/// What a replay checked, and the first difference it found.
#[derive(Debug, Default)]
pub struct Report {
    pub ticks: usize,
    /// Movement blocks run and compared.
    pub calls: usize,
    /// Unit states compared after separation and after the progress count.
    pub units: usize,
    /// Fields that changed between a unit's movement block and separation, by code the port leaves out
    /// (not mismatches): field name and how many units.
    pub outside: BTreeMap<&'static str, usize>,
    /// The first mismatch: where, which unit and field, the Rust value and the reference's.
    pub mismatch: Option<String>,
}

/// Every field of a unit as text, floats exact (Rust's shortest round-trip form, with -0 and NaN). Which
/// route array a lane scan or a clearance was made on is compared as "this unit's route" or "another".
fn view(u: &Unit) -> Vec<(&'static str, String)> {
    let same = |path: u64| {
        if path == u.path_gen {
            "this route"
        } else {
            "another route"
        }
    };
    vec![
        ("id", format!("{:?}", u.id)),
        ("type", format!("{:?}", u.kind)),
        ("owner", format!("{:?}", u.owner)),
        ("x", format!("{:?}", u.x)),
        ("z", format!("{:?}", u.z)),
        ("rot", format!("{:?}", u.rot)),
        ("garrison", format!("{:?}", u.garrison)),
        ("riding", format!("{:?}", u.riding)),
        ("air", format!("{:?}", u.air)),
        ("still", format!("{:?}", u.still)),
        ("shotAt", format!("{:?}", u.shot_at)),
        ("dust", format!("{:?}", u.dust)),
        ("built", format!("{:?}", u.built)),
        ("cells", format!("{:?}", u.cells)),
        ("hp", format!("{:?}", u.hp)),
        ("path", format!("{:?}", u.path)),
        ("reverse", format!("{:?}", u.reverse)),
        ("pace", format!("{:?}", u.pace)),
        ("attackId", format!("{:?}", u.attack)),
        ("retreating", format!("{:?}", u.retreating)),
        ("sprint", format!("{:?}", u.sprint)),
        ("supp", format!("{:?}", u.supp)),
        ("hitFrom", format!("{:?}", u.hit_from)),
        ("stuck", format!("{:?}", u.stuck)),
        ("repath", format!("{:?}", u.repath)),
        ("worldGoal", format!("{:?}", u.world_goal)),
        ("amove", format!("{:?}", u.amove)),
        ("moveSpeed", format!("{:?}", u.move_speed)),
        ("travelDir", format!("{:?}", u.travel_dir)),
        ("vx", format!("{:?}", u.vx)),
        ("vz", format!("{:?}", u.vz)),
        ("drift", format!("{:?}", u.drift)),
        ("motionBefore", format!("{:?}", u.motion_before)),
        (
            "motionClearance",
            format!(
                "{:?}",
                u.clearance
                    .as_ref()
                    .map(|c| (c.goal, c.reverse, same(c.path), c.until))
            ),
        ),
        ("traffic", format!("{:?}", u.traffic)),
        ("trafficWait", format!("{:?}", u.traffic_wait)),
        ("trafficTrail", format!("{:?}", u.traffic_trail)),
        ("trafficEntered", format!("{:?}", u.traffic_entered)),
        ("queuedBehind", format!("{:?}", u.queued_behind)),
        (
            "laneScan",
            format!(
                "{:?}",
                u.lane_scan
                    .as_ref()
                    .map(|s| (same(s.path), s.until, s.lane))
            ),
        ),
        ("ghost", format!("{:?}", u.ghost)),
        ("holdPos", format!("{:?}", u.hold_pos)),
        ("build", format!("{:?}", u.build)),
        ("dig", format!("{:?}", u.dig)),
        ("entrench", format!("{:?}", u.entrench)),
        ("board", format!("{:?}", u.board)),
        ("rail", format!("{:?}", u.rail)),
    ]
}

/// The first field where two units differ: its name, `a`'s value and `b`'s.
pub fn diff(a: &Unit, b: &Unit) -> Option<(&'static str, String, String)> {
    view(a)
        .into_iter()
        .zip(view(b))
        .find(|((_, x), (_, y))| x != y)
        .map(|((name, x), (_, y))| (name, x, y))
}

/// The recorded units, as the codec wrote them, kept up to date with each record's changed fields.
struct Recorded {
    units: BTreeMap<UnitId, Value>,
    players: usize,
    cells: usize,
}

impl Recorded {
    /// Applies one record (`[id, {changed fields}]`) and returns the unit's id.
    fn apply(&mut self, id: &Value, delta: &Value) -> Result<UnitId> {
        let id =
            number(id).ok_or_else(|| format!("expected a unit id, not {}", short(id)))? as UnitId;
        let fields = delta
            .as_object()
            .ok_or_else(|| format!("unit {id}: expected changed fields"))?;
        let unit = self
            .units
            .entry(id)
            .or_insert_with(|| Value::Object(serde_json::Map::new()));
        let object = unit.as_object_mut().expect("units are objects");
        for (k, v) in fields {
            object.insert(k.clone(), v.clone());
        }
        Ok(id)
    }

    fn apply_all(&mut self, records: Option<&Value>) -> Result<()> {
        for record in records.and_then(Value::as_array).into_iter().flatten() {
            self.apply(&record[0], &record[1])?;
        }
        Ok(())
    }

    fn unit(&self, rules: &Rules, id: UnitId) -> Result<Unit> {
        let u = self
            .units
            .get(&id)
            .ok_or_else(|| format!("unit {id} was never recorded"))?;
        unit(u, rules, self.players, self.cells).map_err(|e| format!("unit {id}: {e}"))
    }
}

/// Compares every unit with its recorded state; the first difference, if any.
fn compare_all(
    g: &Game,
    rec: &Recorded,
    rules: &Rules,
    report: &mut Report,
    stage: &str,
) -> Result<Option<String>> {
    for (&id, u) in &g.units {
        let expected = rec.unit(rules, id)?;
        report.units += 1;
        if let Some((field, ours, theirs)) = diff(u, &expected) {
            return Ok(Some(format!(
                "tick {} {stage}: unit {id} {field}: Rust {ours}, reference {theirs}",
                g.tick
            )));
        }
    }
    Ok(None)
}

/// Replays a recording from `harness/movement-trace.mjs`, stopping at the first mismatch.
pub fn replay(trace: &Value) -> Result<Report> {
    replay_with_routes(trace, &mut |_, _, u, _, _| u.repath = 1.0)
}

/// Replays movement with a caller's route implementation at the live replan point.
pub fn replay_with_routes(
    trace: &Value,
    routes: &mut impl FnMut(&Game, &Rules, &mut Unit, ww2_core::game::Point, RouteRequest),
) -> Result<Report> {
    let rules = load_rules(trace.get("tables").ok_or("the recording has no tables")?)?;
    let start = trace.get("game").ok_or("the recording has no game")?;
    let mut g = load_game(start, &rules)?.game;
    let mut rec = Recorded {
        units: BTreeMap::new(),
        players: g.players.len(),
        cells: g.w * g.h,
    };
    for (id, u) in map_entries(start.get("units").ok_or("the game has no units")?)
        .ok_or("units: expected a Map")?
    {
        rec.apply(id, u)?;
    }
    let mut report = Report::default();
    let steps = trace
        .get("steps")
        .and_then(Value::as_array)
        .ok_or("the recording has no steps")?;
    for step in steps {
        report.ticks += 1;
        rec.apply_all(step.get("start"))?;
        g.units = rec
            .units
            .keys()
            .map(|&id| rec.unit(&rules, id).map(|u| (id, u)))
            .collect::<Result<BTreeMap<_, _>>>()?;
        let mut grid = SpatialGrid::new(&g.units, GRID_BUCKET);
        g.tick = step
            .get("tick")
            .and_then(number)
            .ok_or("a step has no tick")? as i64;
        // the living ground dries or soaks at the end of each step
        g.wet = step.get("wet").and_then(number);
        let visible = step
            .get("visible")
            .and_then(Value::as_array)
            .ok_or("a step has no visible sets")?;
        for (p, ids) in g.players.iter_mut().zip(visible) {
            p.visible = ids
                .as_array()
                .ok_or("visible: expected ids")?
                .iter()
                .map(|id| {
                    number(id)
                        .map(|id| id as UnitId)
                        .ok_or("visible: expected ids")
                })
                .collect::<std::result::Result<Vec<_>, _>>()?;
            if !p.visible.is_sorted() {
                return Err("visible: not in ascending id order".into());
            }
        }
        let mut moved: Vec<Moved> = Vec::new();
        for event in step
            .get("events")
            .and_then(Value::as_array)
            .ok_or("a step has no events")?
        {
            let kind = event[0].as_str().ok_or("an event has no kind")?;
            let id = rec.apply(&event[1], &event[2])?;
            let u = rec.unit(&rules, id)?;
            grid.update(id, &u);
            g.units.insert(id, u);
            if kind == "place" {
                continue;
            }
            let supp_speed = number(&event[3]).ok_or("a move has no suppression speed")?;
            let m = move_unit_with_routes(&mut g, &rules, &mut grid, id, supp_speed, routes)
                .ok_or("the unit vanished")?;
            report.calls += 1;
            rec.apply(&event[1], &event[4])?;
            let expected = rec.unit(&rules, id)?;
            if let Some((field, ours, theirs)) = diff(&g.units[&id], &expected) {
                report.mismatch = Some(format!(
                    "tick {} movement: unit {id} {field}: Rust {ours}, reference {theirs}",
                    g.tick
                ));
                return Ok(report);
            }
            let speed = number(&event[5]).ok_or("a move has no speed")?;
            if format!("{:?}", m.speed) != format!("{speed:?}") {
                report.mismatch = Some(format!(
                    "tick {} movement: unit {id} speed: Rust {:?}, reference {speed:?}",
                    g.tick, m.speed
                ));
                return Ok(report);
            }
            moved.push(m);
        }
        // Before separation: note what code the port leaves out changed after each unit's block, then take
        // the recorded states.
        rec.apply_all(step.get("separate"))?;
        for (&id, u) in g.units.iter_mut() {
            let recorded = rec.unit(&rules, id)?;
            for ((name, ours), (_, theirs)) in view(u).into_iter().zip(view(&recorded)) {
                if ours != theirs {
                    *report.outside.entry(name).or_default() += 1;
                }
            }
            *u = recorded;
        }
        separate_crowds(&mut g, &rules, &mut grid);
        rec.apply_all(step.get("separated"))?;
        if let Some(m) = compare_all(&g, &rec, &rules, &mut report, "separation")? {
            report.mismatch = Some(m);
            return Ok(report);
        }
        infantry_progress(&mut g, &rules, &moved);
        rec.apply_all(step.get("progressed"))?;
        if let Some(m) = compare_all(&g, &rec, &rules, &mut report, "infantry progress")? {
            report.mismatch = Some(m);
            return Ok(report);
        }
    }
    Ok(report)
}
