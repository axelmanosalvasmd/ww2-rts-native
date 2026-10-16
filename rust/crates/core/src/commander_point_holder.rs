//! Point holders fortify the approach shown by their delivered enemy spawns.
use super::*;
use crate::game::{CELL, TRENCH};

fn cells(observation: &Observation, at: Point, radius: f64) -> Vec<usize> {
    let input = observation.input();
    let mut result = Vec::new();
    let min_x = ((at.x - radius) / CELL).floor() as isize;
    let min_z = ((at.z - radius) / CELL).floor() as isize;
    for z in min_z..=((at.z + radius) / CELL) as isize {
        for x in min_x..=((at.x + radius) / CELL) as isize {
            if x >= 0 && z >= 0 && x < input.width as isize && z < input.height as isize {
                result.push(z as usize * input.width + x as usize);
            }
        }
    }
    result
}

#[allow(clippy::too_many_arguments)]
pub(super) fn plan(
    observation: &Observation,
    context: &Context,
    unit: &UnitObservation,
    point: &crate::commander_ai::PointObservation,
    enemies: &[&UnitObservation],
    armor_seen: bool,
    budget: f64,
) -> Result<Option<Value>, String> {
    let input = observation.input();
    let me = &input.players[input.slot];
    let Some(foe) = crate::sort::first_by(
        input.players.iter().filter(|p| p.team != me.team),
        |a, b| dist(a.spawn, point.at) - dist(b.spawn, point.at),
    ) else {
        return Ok(None);
    };
    let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
    let cfg = catalog.table("CFG").map_err(|e| e.to_string())?;
    let forts = catalog.table("FORTS").map_err(|e| e.to_string())?;
    let cost = |kind| forts.get(kind).map_or(0.0, |d| num(d, "cost"));
    let radius = observation.point_radius();
    let length = dist(foe.spawn, point.at);
    let length = if length == 0.0 { 1.0 } else { length };
    let fx = (foe.spawn.x - point.at.x) / length;
    let fz = (foe.spawn.z - point.at.z) / length;
    let toward = |distance| Point {
        x: point.at.x + fx * distance,
        z: point.at.z + fz * distance,
    };
    let ahead = toward(radius + 7.0);
    let mines = budget >= cost("mines") + 60.0
        && context
            .mines
            .iter()
            .filter(|m| dist(**m, point.at) <= radius + 10.0)
            .count()
            < 2;
    let trench = budget >= num(cfg, "digCost") + 60.0
        && cells(observation, point.at, radius + 4.0)
            .iter()
            .filter(|&&c| input.flags[c] & TRENCH != 0)
            .count()
            < 6;
    let wire = budget >= cost("wire") * 2.0 + 60.0
        && cells(observation, ahead, 6.0)
            .iter()
            .filter(|&&c| input.chars[c] == b'X')
            .count()
            < 4;
    let traps = armor_seen
        && budget >= cost("traps") * 2.0 + 60.0
        && cells(observation, ahead, 8.0)
            .iter()
            .filter(|&&c| input.chars[c] == b'Y')
            .count()
            < 4;
    let free = !unit.dig
        && !unit.entrench
        && cfg
            .get("fortBuilders")
            .and_then(Rule::array)
            .is_some_and(|builders| {
                builders
                    .iter()
                    .any(|k| k.string() == Some(unit.kind.as_str()))
            });
    let work = free
        && !enemies.iter().any(|e| dist(e.at, point.at) < 40.0)
        && (mines || trench || wire || traps);
    if !work
        && !unit.garrison
        && boolean(
            catalog.unit(&unit.kind).map_err(|e| e.to_string())?,
            "garrisons",
        )
    {
        let mut house = None;
        let mut best = f64::INFINITY;
        for cell in cells(observation, point.at, radius + 3.0) {
            if input.chars[cell] != b'B' {
                continue;
            }
            let at = Point {
                x: (cell % input.width) as f64 * CELL + CELL / 2.0,
                z: (cell / input.width) as f64 * CELL + CELL / 2.0,
            };
            if dist(at, point.at) < best {
                best = dist(at, point.at);
                house = Some(at);
            }
        }
        if let Some(at) = house {
            return Ok(Some(
                json!({"t":"garrison","ids":[unit.id],"x":at.x,"z":at.z}),
            ));
        }
    }
    if unit.garrison {
        return Ok(work.then(|| json!({"t":"move","orders":[[unit.id,point.at.x,point.at.z]]})));
    }
    let direction = math::atan2(fz, fx) + std::f64::consts::FRAC_PI_2;
    if free && mines {
        let at = toward(radius + 2.0);
        return Ok(Some(
            json!({"t":"dig","ids":[unit.id],"kind":"mines","x":at.x,"z":at.z,"dir":direction}),
        ));
    }
    if free && trench {
        let at = toward(6.0);
        return Ok(Some(
            json!({"t":"entrench","ids":[unit.id],"pattern":if budget >= 600.0 {"strongpoint"} else {"arc"},"x":point.at.x,"z":point.at.z,"x2":at.x,"z2":at.z}),
        ));
    }
    if free && (wire || traps) {
        let (fort, distance, half) = if wire {
            ("wire", radius + 5.0, 7.0)
        } else {
            ("traps", radius + 10.0, 8.0)
        };
        let at = toward(distance);
        return Ok(Some(
            json!({"t":"entrench","ids":[unit.id],"pattern":"line","fort":fort,"x":at.x + fz * half,"z":at.z - fx * half,"x2":at.x - fz * half,"z2":at.z + fx * half}),
        ));
    }
    Ok(
        (!unit.dig && !unit.entrench && !matches!(unit.cover, 1 | 2))
            .then(|| json!({"t":"cover","ids":[unit.id]})),
    )
}
