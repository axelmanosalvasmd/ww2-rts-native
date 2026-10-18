//! Building previews composed solely from delivered state, never an authoritative Match.
use super::*;
use crate::{client_view::ClientView, game::Smoke};

/// Ordinary-map building preview using the command's placement kernel.
/// No C export or World recipient admission is added by this internal provider.
pub fn check_recipient(view: &ClientView, kind: &str, at: Point) -> Result<Placement, String> {
    let start = view.start().ok_or("placement requires a recipient start")?;
    let map = &start["map"];
    let state = view.state();
    let mode = Mode::from_kind(state["mode"]["kind"].as_str());
    if mode == Mode::World || map.get("world").is_some_and(crate::map_save::truthy) {
        return Err("World recipient placement deferred".into());
    }
    let (w, h) = (view.width() as usize, view.height() as usize);
    let n = w.checked_mul(h).ok_or("placement dimensions")?;
    let rows = map["rows"]
        .as_array()
        .ok_or("placement requires received map rows")?;
    if rows.len() != h {
        return Err("invalid placement rows".into());
    }
    let mut chars = Vec::with_capacity(n);
    let mut height = Vec::with_capacity(n);
    for (z, row) in rows.iter().enumerate() {
        let row = row
            .as_str()
            .filter(|row| row.len() == w && row.is_ascii())
            .ok_or("invalid placement row")?;
        for (x, ch) in row.bytes().enumerate() {
            let layer = |name: &str| {
                map["layers"][name][z]
                    .as_str()
                    .and_then(|row| row.as_bytes().get(x))
                    .copied()
            };
            chars.push(if ch == b'N' {
                crate::map::compose_world_cell(
                    layer("ground").unwrap_or(b'.'),
                    layer("objects").unwrap_or(b'.'),
                    false,
                )
            } else {
                ch
            });
            let level = match map["heights"][z]
                .as_str()
                .and_then(|row| row.as_bytes().get(x))
            {
                Some(&ch) => crate::map::level_of(ch).ok_or("invalid received map height")?,
                None => 0,
            };
            height.push(level);
        }
    }
    for (&cell, row) in view.cells() {
        let c = cell as usize;
        let ch = row[1]
            .as_str()
            .and_then(|s| s.as_bytes().first())
            .copied()
            .ok_or("invalid remembered placement cell")?;
        chars[c] = if ch == b'N' {
            let letter = |name: &str| {
                row[4][name]
                    .as_str()
                    .and_then(|s| s.as_bytes().first())
                    .copied()
                    .unwrap_or(b'.')
            };
            crate::map::compose_world_cell(letter("ground"), letter("object"), false)
        } else {
            ch
        };
        height[c] = row[2]
            .as_i64()
            .and_then(|v| i8::try_from(v).ok())
            .ok_or("invalid remembered height")?;
    }
    let rules = Rules::compiled().map_err(|e| e.to_string())?;
    let mut occupied = Vec::new();
    for unit in view.units() {
        let definition = rules
            .kind(&unit.kind)
            .map(|i| &rules.kinds[i])
            .ok_or("invalid received placement unit")?;
        if unit.hp > 0.0 && unit.flags & 262144 == 0 && !definition.air && !definition.structure {
            occupied.push(Point {
                x: unit.x,
                z: unit.z,
            });
        }
    }
    let flags = chars
        .iter()
        .map(|&ch| crate::map::flags(ch))
        .collect::<Vec<_>>();
    // Only line-of-sight terrain is needed. This query game has no authoritative actors or owners.
    let game = Game {
        w,
        h,
        flags,
        height: Some(height),
        chars,
        house: vec![0; n],
        ground: None,
        objects: None,
        wear: None,
        cell_hp: Vec::new(),
        trench_front: None,
        tough: None,
        wet: None,
        reveal: false,
        wrecks: Vec::new(),
        tick: 0,
        vision_tick: None,
        terrain_version: 0,
        mode: None,
        naval: map["naval"] == true,
        weather: state["weather"][0]
            .as_str()
            .and_then(|name| rules.weather_kind(name)),
        rain: state["wx"][0].as_f64().unwrap_or(0.0),
        players: Vec::new(),
        units: Default::default(),
        smokes: state["smokes"].as_array().map_or(Ok(Vec::new()), |rows| {
            rows.iter()
                .map(|row| {
                    Ok(Smoke {
                        id: 0,
                        remaining: None,
                        x: row[0].as_f64().ok_or("invalid received smoke")?,
                        z: row[1].as_f64().ok_or("invalid received smoke")?,
                        r: row[2].as_f64().ok_or("invalid received smoke")?,
                    })
                })
                .collect::<Result<Vec<_>, String>>()
        })?,
        strikes: Vec::new(),
        sections: Default::default(),
    };
    let slot = start["you"]
        .as_u64()
        .ok_or("placement requires a player seat")? as usize;
    let team = |slot: usize| start["teams"][slot].as_i64().unwrap_or(slot as i64);
    let sees = |spot: Point| {
        Ok(view.units().any(|unit| {
            let Ok(owner) = usize::try_from(unit.owner) else {
                return false;
            };
            if team(owner) != team(slot) {
                return false;
            }
            let definition = &rules.kinds[rules.kind(&unit.kind).expect("received kinds checked")];
            let base = if definition.lookout && unit.built < 1.0 {
                12.0
            } else {
                definition.vision
            };
            let reach = base
                * if definition.air {
                    1.0
                } else {
                    game.sight_mul(&rules)
                };
            math::hypot(unit.x - spot.x, unit.z - spot.z) <= reach
                && if definition.air {
                    unit.flags & 512 == 0
                } else {
                    definition.building
                        || crate::vision::los(&game, &rules, (unit.x, unit.z), (spot.x, spot.z))
                }
        }))
    };
    let nodes = state["nodes"].as_array().map_or(Ok(Vec::new()), |rows| {
        rows.iter()
            .map(|row| {
                let x = row[0].as_f64().ok_or("invalid received node")?;
                let z = row[1].as_f64().ok_or("invalid received node")?;
                let (cx, cz) = ((x / CELL).floor() - 1.0, (z / CELL).floor() - 1.0);
                if cx < 0.0 || cz < 0.0 || cx >= w as f64 || cz >= h as f64 {
                    return Err("invalid received node cell".into());
                }
                Ok(Node {
                    cell: cz as usize * w + cx as usize,
                    at: Point { x, z },
                    claimed: view
                        .units()
                        .any(|u| u.kind == "depot" && math::hypot(u.x - x, u.z - z) < 1.0),
                })
            })
            .collect::<Result<Vec<_>, String>>()
    })?;
    check_parts(
        &Terrain {
            w,
            h,
            flags: &game.flags,
            chars: &game.chars,
            height: game.height.as_deref(),
            naval: game.naval,
            occupied: &occupied,
        },
        mode,
        state["logistics"]["enabled"] == true,
        &nodes,
        None,
        None,
        kind,
        at,
        sees,
    )
}
