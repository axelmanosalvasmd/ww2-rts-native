//! Recipient-only placement preview, matching client/availability.js:169-198 and
//! shared/sim.js:3261-3305 at e4e579a. Building kernel adapted from PR #409.
//! No authoritative Match or command admission is consulted.
use crate::{
    catalog::{Catalog, Value},
    client_view::ClientView,
    economy::Mode,
    game::{CELL, Game, LAND, MOVE, Point},
    math,
    rules::Rules,
};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    Blocked,
    NotVisible,
    Territory,
    Rail,
    Coast,
}

#[derive(Clone, Copy, Debug)]
pub struct Node {
    pub cell: usize,
    pub at: Point,
    pub claimed: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Placement {
    pub reason: Option<Reason>,
    pub at: Point,
    pub cell: Option<usize>,
    pub cells: Option<Vec<usize>>,
    pub node: Option<usize>,
}
impl Placement {
    fn denied(at: Point, reason: Reason) -> Self {
        Self {
            reason: Some(reason),
            at,
            cell: None,
            cells: None,
            node: None,
        }
    }
}

/// The authoritative and recipient adapters supply the same borrowed placement facts.
struct Terrain<'a> {
    w: usize,
    h: usize,
    flags: &'a [u16],
    chars: &'a [u8],
    height: Option<&'a [i8]>,
    naval: bool,
    occupied: &'a [Point],
}
impl Terrain<'_> {
    fn cell_of(&self, at: Point) -> Option<usize> {
        if !at.x.is_finite()
            || !at.z.is_finite()
            || at.x < 0.0
            || at.z < 0.0
            || at.x >= self.w as f64 * CELL
            || at.z >= self.h as f64 * CELL
        {
            return None;
        }
        Some((at.z / CELL) as usize * self.w + (at.x / CELL) as usize)
    }
}
fn foot_center(game: &Terrain<'_>, cell: usize, size: usize) -> Point {
    Point {
        x: (cell % game.w) as f64 * CELL + size as f64 * CELL / 2.0,
        z: (cell / game.w) as f64 * CELL + size as f64 * CELL / 2.0,
    }
}
fn footprint(game: &Terrain<'_>, cell: usize, size: usize) -> Option<Vec<usize>> {
    let (x, z) = (cell % game.w, cell / game.w);
    if x + size > game.w || z + size > game.h {
        return None;
    }
    Some(
        (z..z + size)
            .flat_map(|z| (x..x + size).map(move |x| z * game.w + x))
            .collect(),
    )
}
#[allow(clippy::too_many_arguments)]
fn check_parts(
    game: &Terrain<'_>,
    mode: Mode,
    logistics: bool,
    nodes: &[Node],
    owned_cells: Option<&BTreeSet<usize>>,
    rail_yard: Option<&BTreeSet<usize>>,
    kind: &str,
    at: Point,
    mut sees: impl FnMut(Point) -> Result<bool, String>,
) -> Result<Placement, String> {
    let fail = |reason| Placement::denied(at, reason);
    if !at.x.is_finite() || !at.z.is_finite() {
        return Ok(Placement::denied(
            Point {
                x: if at.x.is_finite() { at.x } else { 0.0 },
                z: if at.z.is_finite() { at.z } else { 0.0 },
            },
            Reason::Blocked,
        ));
    }
    let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
    let buildable = catalog
        .table("BUILDABLE")
        .map_err(|e| e.to_string())?
        .array()
        .ok_or("invalid building list")?
        .iter()
        .any(|name| name.string() == Some(kind));
    if !(buildable || kind == "hq" && (mode.is_skirmish() || mode == Mode::World))
        || kind == "supplycache" && !logistics
    {
        return Ok(fail(Reason::Blocked));
    }
    let world = mode == Mode::World;
    if world && !sees(at)? {
        return Ok(fail(Reason::NotVisible));
    }
    let count = game.w.checked_mul(game.h).ok_or("placement dimensions")?;
    if count == 0
        || game.flags.len() != count
        || game.chars.len() != count
        || game.height.as_ref().is_some_and(|rows| rows.len() != count)
    {
        return Err("invalid placement terrain".into());
    }
    let definition = catalog.unit(kind).map_err(|e| e.to_string())?;
    let size = definition
        .get("size")
        .and_then(Value::number)
        .filter(|size| size.is_finite() && *size >= 1.0 && *size <= count as f64)
        .ok_or("invalid building size")? as usize;
    let node = if kind == "depot" {
        let mut nearest = None;
        let mut distance = f64::INFINITY;
        // Strict comparison keeps Source's stable input order for equal distances.
        for (index, node) in nodes.iter().enumerate() {
            let candidate = math::hypot(node.at.x - at.x, node.at.z - at.z);
            if !node.claimed && candidate <= 8.0 && candidate < distance {
                nearest = Some(index);
                distance = candidate;
            }
        }
        if nearest.is_none() {
            for node in nodes {
                if math::hypot(node.at.x - at.x, node.at.z - at.z) <= 8.0
                    && node.cell < count
                    && !sees(foot_center(game, node.cell, size))?
                {
                    return Ok(fail(Reason::NotVisible));
                }
            }
            return Ok(fail(Reason::Blocked));
        }
        nearest
    } else {
        None
    };
    let cell = if let Some(index) = node {
        nodes[index].cell
    } else {
        let x = math::round(at.x / CELL - size as f64 / 2.0);
        let z = math::round(at.z / CELL - size as f64 / 2.0);
        if x < 0.0 || z < 0.0 || x >= game.w as f64 || z >= game.h as f64 {
            return Ok(fail(Reason::Blocked));
        }
        z as usize * game.w + x as usize
    };
    if cell >= count {
        return Err("invalid placement node cell".into());
    }
    let cells = footprint(game, cell, size);
    let mut result = Placement {
        reason: Some(Reason::Blocked),
        at: foot_center(game, cell, size),
        cell: Some(cell),
        cells,
        node,
    };
    let Some(cells) = result.cells.as_ref() else {
        return Ok(result);
    };
    if world {
        for &cell in cells {
            if !sees(Point {
                x: (cell % game.w) as f64 * CELL + CELL / 2.0,
                z: (cell / game.w) as f64 * CELL + CELL / 2.0,
            })? {
                result.reason = Some(Reason::NotVisible);
                return Ok(result);
            }
        }
    } else if !sees(result.at)? {
        result.reason = Some(Reason::NotVisible);
        return Ok(result);
    }
    let reason = if world
        && !cells
            .iter()
            .all(|cell| owned_cells.is_some_and(|owned| owned.contains(cell)))
    {
        Some(Reason::Territory)
    } else if cells
        .iter()
        .any(|cell| rail_yard.is_some_and(|yard| yard.contains(cell)))
    {
        Some(Reason::Rail)
    } else if kind != "depot"
        && nodes.iter().any(|node| {
            node.cell < count
                && footprint(game, node.cell, 2)
                    .is_some_and(|footprint| footprint.iter().any(|cell| cells.contains(cell)))
        })
    {
        Some(Reason::Blocked)
    } else if definition.get("coast").and_then(Value::boolean) == Some(true)
        && !(game.naval && on_coast(game, cells))
    {
        Some(if game.naval {
            Reason::Coast
        } else {
            Reason::Blocked
        })
    } else if !can_stamp(game, cells) {
        Some(Reason::Blocked)
    } else {
        None
    };
    result.reason = reason;
    Ok(result)
}

fn on_coast(game: &Terrain<'_>, cells: &[usize]) -> bool {
    cells.iter().any(|cell| {
        let (x, z) = ((cell % game.w) as isize, (cell / game.w) as isize);
        (-2..=2).any(|dz| {
            (-2..=2).any(|dx| {
                let (x, z) = (x + dx, z + dz);
                x >= 0
                    && z >= 0
                    && x < game.w as isize
                    && z < game.h as isize
                    && game.flags[z as usize * game.w + x as usize] & LAND == 0
            })
        })
    })
}

fn can_stamp(game: &Terrain<'_>, cells: &[usize]) -> bool {
    let (mut low, mut high) = (i8::MAX, i8::MIN);
    for &cell in cells {
        if game.flags[cell] & MOVE != 0 || b"WF=KOAQ".contains(&game.chars[cell]) {
            return false;
        }
        let height = game.height.map_or(0, |height| height[cell]);
        low = low.min(height);
        high = high.max(height);
    }
    i16::from(high) - i16::from(low) <= 1
        && !game
            .occupied
            .iter()
            .any(|at| game.cell_of(*at).is_some_and(|cell| cells.contains(&cell)))
}

/// The request and response use the prototype's placementCheck object shape.
/// World previews require retained initial terrain and rail discovery history, which this view
/// does not retain. They return an explicit error without opening running World admission.
pub fn placement_check(
    view: &ClientView,
    request: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    use serde_json::json;
    if !request.is_object() {
        return Err("placement request must be an object".into());
    }
    let kind = request["kind"]
        .as_str()
        .ok_or("placement kind must be a string")?;
    let (x, z) = (request["x"].as_f64(), request["z"].as_f64());
    let denied = |reason: &str| json!({"ok":false,"reason":reason,"x":x.unwrap_or(0.0),"z":z.unwrap_or(0.0)});
    let (Some(x), Some(z)) = (x, z) else {
        return Ok(denied("blocked"));
    };
    let at = Point { x, z };
    let start = view.start().ok_or("placement requires a recipient start")?;
    let state = view.state();
    if state.is_null() {
        return Err("placement requires a recipient snapshot".into());
    }
    let map = &start["map"];
    let mode = Mode::from_kind(state["mode"]["kind"].as_str());
    if mode == Mode::World
        || map.get("world").is_some_and(|v| match v {
            serde_json::Value::Null => false,
            serde_json::Value::Bool(v) => *v,
            serde_json::Value::Number(v) => v.as_f64() != Some(0.0),
            serde_json::Value::String(v) => !v.is_empty(),
            _ => true,
        })
    {
        return Err(
            "World placement requires retained initial terrain and railway discovery history"
                .into(),
        );
    }
    let rules = Rules::compiled().map_err(|e| e.to_string())?;
    let (game, initial_chars, initial_height) = recipient_game(view)?;
    let slot = start["you"]
        .as_u64()
        .ok_or("placement requires a recipient seat")?;
    let teams = start["teams"]
        .as_array()
        .ok_or("placement requires received teams")?;
    let team = teams.get(slot as usize).cloned().unwrap_or(json!(slot));
    let sees = |spot: Point| {
        view.units().any(|unit| {
            let Some(owner) = usize::try_from(unit.owner).ok() else {
                return false;
            };
            if teams.get(owner) != Some(&team) {
                return false;
            }
            let Some(index) = rules.kind(&unit.kind) else {
                return false;
            };
            let definition = &rules.kinds[index];
            let reach = if definition.lookout && unit.built < 1.0 {
                12.0
            } else {
                definition.vision
            };
            // snapshotUnits has no air state. placementState also supplies no weather or rain.
            math::hypot(unit.x - spot.x, unit.z - spot.z) <= reach
                && (definition.building
                    || crate::vision::los(&game, &rules, (unit.x, unit.z), (spot.x, spot.z)))
        })
    };
    for unit in view.units() {
        if rules.kind(&unit.kind).is_none() {
            return Err(format!("unknown delivered placement unit {}", unit.kind));
        }
    }
    let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
    if let Some(fort) = catalog.table("FORTS").map_err(|e| e.to_string())?.get(kind) {
        let direction = match request.get("dir") {
            None => 0.0,
            Some(value) => value
                .as_f64()
                .ok_or("placement direction must be a number")?,
        };
        let (cells, hidden) = fort_cells(
            &game,
            &initial_chars,
            &initial_height,
            fort,
            kind,
            at,
            direction,
            request.get("team").is_none(),
            sees,
        )?;
        return Ok(if hidden {
            json!({"ok":false,"reason":"notVisible","x":x,"z":z,"cells":[]})
        } else if cells.is_empty() {
            json!({"ok":false,"reason":"blocked","x":x,"z":z,"cells":[]})
        } else {
            json!({"ok":true,"x":x,"z":z,"cells":cells})
        });
    }
    let nodes = recipient_nodes(view, game.w, game.h)?;
    let occupied = view
        .units()
        .filter_map(|unit| {
            let definition = &rules.kinds[rules.kind(&unit.kind).expect("checked delivered kind")];
            (unit.hp > 0.0 && unit.flags & 262144 == 0 && !definition.air && !definition.structure)
                .then_some(Point {
                    x: unit.x,
                    z: unit.z,
                })
        })
        .collect::<Vec<_>>();
    let result = check_parts(
        &Terrain {
            w: game.w,
            h: game.h,
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
        |spot| Ok(sees(spot)),
    )?;
    let mut out = json!({"ok":result.reason.is_none(),"x":result.at.x,"z":result.at.z});
    if let Some(reason) = result.reason {
        out["reason"] = json!(match reason {
            Reason::Blocked => "blocked",
            Reason::NotVisible => "notVisible",
            Reason::Territory => "territory",
            Reason::Rail => "rail",
            Reason::Coast => "coast",
        });
    }
    if let Some(cell) = result.cell {
        out["c"] = json!(cell);
        out["cells"] = json!(result.cells);
    }
    if let Some(index) = result.node {
        let node = &nodes[index];
        out["node"] = json!({"x":node.at.x,"z":node.at.z,"c":node.cell,"depot":0});
    }
    Ok(out)
}

fn recipient_nodes(view: &ClientView, w: usize, h: usize) -> Result<Vec<Node>, String> {
    let Some(rows) = view.state()["nodes"].as_array() else {
        return Ok(Vec::new());
    };
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
        .collect()
}

fn recipient_game(view: &ClientView) -> Result<(Game, Vec<u8>, Vec<i8>), String> {
    let map = &view.start().ok_or("placement requires start")?["map"];
    let (w, h) = (view.width() as usize, view.height() as usize);
    let n = w * h;
    let rows = map["rows"]
        .as_array()
        .filter(|rows| rows.len() == h)
        .ok_or("invalid received placement rows")?;
    let mut chars = Vec::with_capacity(n);
    let mut height = Vec::with_capacity(n);
    for (y, row) in rows.iter().enumerate() {
        let row = row
            .as_str()
            .filter(|row| row.is_ascii() && row.len() == w)
            .ok_or("invalid received placement row")?;
        chars.extend(row.bytes());
        for x in 0..w {
            height.push(
                match map["heights"][y]
                    .as_str()
                    .and_then(|row| row.as_bytes().get(x))
                {
                    Some(&ch) => {
                        crate::map::level_of(ch).ok_or("invalid received placement height")?
                    }
                    None => 0,
                },
            );
        }
    }
    let initial_chars = chars.clone();
    let initial_height = height.clone();
    let mut layers = crate::map::world_layers(map).map_err(|e| e.to_string())?;
    for (&cell, row) in view.cells() {
        let c = cell as usize;
        chars[c] = row[1]
            .as_str()
            .and_then(|s| s.as_bytes().first())
            .copied()
            .ok_or("invalid received placement cell")?;
        if let Some(level) = row.get(2).filter(|v| !v.is_null()) {
            height[c] = level
                .as_i64()
                .and_then(|v| i8::try_from(v).ok())
                .ok_or("invalid received placement cell height")?;
        }
        if let Some(data) = row.get(4).filter(|v| !v.is_null()) {
            let letter = |key: &str| {
                data[key]
                    .as_str()
                    .and_then(|s| s.as_bytes().first())
                    .copied()
                    .ok_or_else(|| format!("received placement metadata requires {key}"))
            };
            layers.ground[c] = letter("ground")?;
            layers.objects[c] = letter("object")?;
        }
    }
    let flags = chars
        .iter()
        .enumerate()
        .map(|(c, &ch)| {
            crate::map::flags(if ch == b'N' {
                crate::map::compose_world_cell(layers.ground[c], layers.objects[c], false)
            } else {
                ch
            })
        })
        .collect();
    let smokes = view.state()["smokes"]
        .as_array()
        .map_or(Ok(Vec::new()), |rows| {
            rows.iter()
                .map(|row| {
                    Ok(crate::game::Smoke {
                        id: 0,
                        remaining: None,
                        x: row[0].as_f64().ok_or("invalid received smoke")?,
                        z: row[1].as_f64().ok_or("invalid received smoke")?,
                        r: row[2].as_f64().ok_or("invalid received smoke")?,
                    })
                })
                .collect::<Result<Vec<_>, String>>()
        })?;
    Ok((
        Game {
            w,
            h,
            flags,
            height: Some(height),
            house: vec![0; n],
            chars,
            ground: Some(layers.ground),
            objects: Some(layers.objects),
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
            weather: None,
            rain: 0.0,
            players: Vec::new(),
            units: Default::default(),
            smokes,
            strikes: Vec::new(),
            sections: Default::default(),
        },
        initial_chars,
        initial_height,
    ))
}

#[allow(clippy::too_many_arguments)]
fn fort_cells(
    game: &Game,
    initial: &[u8],
    initial_height: &[i8],
    fort: &Value,
    kind: &str,
    at: Point,
    direction: f64,
    no_team: bool,
    sees: impl Fn(Point) -> bool,
) -> Result<(Vec<serde_json::Value>, bool), String> {
    use serde_json::json;
    let direction = if fort.get("point").and_then(Value::boolean) == Some(true) {
        math::round(direction / std::f64::consts::FRAC_PI_2) * std::f64::consts::FRAC_PI_2
    } else {
        direction
    };
    let (sx, sz) = (math::cos(direction), math::sin(direction));
    let offsets = if kind == "nest" {
        vec![
            (0.0, 0.0, Some("T")),
            (-1.0, 1.0, Some("#")),
            (0.0, 1.0, Some("#")),
            (1.0, 1.0, Some("#")),
            (-1.0, 0.0, Some("#")),
            (1.0, 0.0, Some("#")),
        ]
    } else {
        let n = fort
            .get("n")
            .and_then(Value::number)
            .ok_or("invalid fort count")? as usize;
        let ch = fort.get("ch").and_then(Value::string);
        (0..n)
            .map(|i| (i as f64 - (n as f64 - 1.0) / 2.0, 0.0, ch))
            .collect()
    };
    let plan = offsets
        .iter()
        .map(|&(along, forward, ch)| {
            (
                game.cell_of(
                    at.x + (sx * along + sz * forward) * CELL,
                    at.z + (sz * along - sx * forward) * CELL,
                ),
                ch,
            )
        })
        .collect::<Vec<_>>();
    if plan.iter().any(|&(cell, _)| {
        cell.is_some_and(|c| {
            !sees(Point {
                x: (c % game.w) as f64 * CELL + 1.0,
                z: (c / game.w) as f64 * CELL + 1.0,
            })
        })
    }) {
        return Ok((Vec::new(), true));
    }
    let mut cells = Vec::new();
    for (cell, ch) in plan {
        let Some(c) = cell else {
            continue;
        };
        let valid = match kind {
            "fill" => {
                game.chars[c] == b'+'
                    || game.chars[c] == b'R'
                    || game.chars[c] == b'F' && initial[c] != b'F'
                    || game.chars[c] == b'.'
                        && game
                            .height
                            .as_ref()
                            .is_some_and(|height| height[c] < initial_height[c])
            }
            // placementState intentionally has no mines/mineSeen. Supplying team here cannot
            // resolve the prototype's mineKnown predicate from its recipient game.
            "demine" => {
                if game.chars[c] == b'N' && !no_team {
                    return Err(
                        "recipient demine placement requires omitted team, as in the browser"
                            .into(),
                    );
                }
                game.chars[c] == b'N'
            }
            "bridge" => game.chars[c] == b'W',
            _ => b".+RDM".contains(&game.chars[c]),
        };
        if valid && !cells.iter().any(|&(cell, _)| cell == c) {
            cells.push((c, ch));
        }
    }
    if kind == "bridge" {
        let mut pending = cells;
        cells = Vec::new();
        loop {
            let before = pending.len();
            pending.retain(|&(c, ch)| {
                let x = c % game.w;
                let neighbors = [
                    c.checked_sub(game.w),
                    c.checked_add(game.w).filter(|&v| v < game.chars.len()),
                    c.checked_sub(1).filter(|_| x > 0),
                    c.checked_add(1).filter(|_| x + 1 < game.w),
                ];
                let supported = neighbors.into_iter().flatten().any(|n| {
                    game.chars[n] == b'='
                        || !b"WF".contains(&game.ground.as_ref().map_or(game.chars[n], |g| g[n]))
                        || cells.iter().any(|&(cell, _)| cell == n)
                });
                if supported {
                    cells.push((c, ch));
                    false
                } else {
                    true
                }
            });
            if before == pending.len() {
                break;
            }
        }
    }
    Ok((
        cells
            .into_iter()
            .map(|(cell, ch)| json!([cell, ch]))
            .collect(),
        false,
    ))
}
