//! Builds the core's rules and game state from the harness's JSON: `harness/tables.mjs` for the rules, and a
//! game state from `harness/sim-state.mjs`, `harness/vision-fixture.mjs` or `harness/movement-trace.mjs`
//! (written by `harness/state-codec.mjs`). Only the fields the ported passes read are taken.
//!
//! Movement fields follow the reference's own reading of a missing value: a field it reads with `??` gets
//! that fallback, a number it reads without one is NaN (which compares and adds as `undefined` does), and a
//! field it reads only for its truth is a `bool`. A state cut down for the vision passes, with no movement
//! fields or terrain letters, loads as units with no route on open ground.

use std::collections::BTreeMap;

use serde_json::Value;
use ww2_core::fog::{Fog, FogTerrain, KeptCells, KeptSource, TeamFog};
use ww2_core::game::{
    CELL, COVER, Clearance, FORD, Game, LAND, LaneScan, MOVE, MUD, ModeKind, PlaneState, Player,
    Point, Pose, ROAD, SHOAL, SIGHT, Section, Smoke, Strike, StrikeKind, TRENCH, Traffic, Unit,
    VBLOCK, WIRE, WOOD, Wreck,
};
use ww2_core::grid::UnitId;
use ww2_core::rules::{
    LaneRules, MovementRules, Rules, TrafficRules, UnitKind, VehicleProfile, VisionRules,
    WeatherKind,
};

use crate::codec::{
    Result, compared, field, given, map_entries, num, num_or, number, numbers, set_items, short,
    truthy,
};

fn text<'a>(v: &'a Value, key: &str) -> Result<&'a str> {
    given(v, key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{key}: expected text in {}", short(v)))
}

fn flag(v: &Value, key: &str) -> bool {
    truthy(field(v, key))
}

fn int<T: TryFrom<i64>>(v: f64, what: &str) -> Result<T> {
    if v.fract() != 0.0 {
        return Err(format!("{what}: {v} is not a whole number"));
    }
    T::try_from(v as i64).map_err(|_| format!("{what}: {v} is out of range"))
}

fn ints<T: TryFrom<i64>>(v: Option<&Value>, what: &str) -> Result<Vec<T>> {
    let items = v
        .and_then(numbers)
        .ok_or_else(|| format!("{what}: expected an array of numbers"))?;
    items.into_iter().map(|x| int(x, what)).collect()
}

/// One whole number per map cell.
fn per_cell<T: TryFrom<i64>>(v: Option<&Value>, n: usize, what: &str) -> Result<Vec<T>> {
    let items: Vec<T> = ints(v, what)?;
    if items.len() == n {
        Ok(items)
    } else {
        Err(format!("{what}: {} cells, not {n}", items.len()))
    }
}

/// The tables `harness/tables.mjs` dumps. Fails if the reference's cell size or flag bits differ from the
/// core's constants.
pub fn load_rules(t: &Value) -> Result<Rules> {
    if num(t, "cell")? != CELL {
        return Err(format!(
            "the reference's CELL is {}, the core's {CELL}",
            num(t, "cell")?
        ));
    }
    let flags = given(t, "flags").ok_or("flags: missing")?;
    for (key, bits) in [
        ("move", MOVE),
        ("sight", SIGHT),
        ("cover", COVER),
        ("trench", TRENCH),
        ("ford", FORD),
        ("wire", WIRE),
        ("vblock", VBLOCK),
        ("road", ROAD),
        ("mud", MUD),
        ("wood", WOOD),
        ("land", LAND),
        ("shoal", SHOAL),
    ] {
        if num(flags, key)? != f64::from(bits) {
            return Err(format!(
                "flag {key}: the reference's bit is {}, the core's {bits}",
                num(flags, key)?
            ));
        }
    }
    let kinds = given(t, "units")
        .and_then(Value::as_array)
        .ok_or("units: expected an array")?
        .iter()
        .map(|u| {
            Ok(UnitKind {
                name: text(u, "name")?.to_string(),
                vision: num(u, "vision")?,
                lookout: flag(u, "lookout"),
                camo: flag(u, "camo"),
                structure: flag(u, "structure"),
                building: flag(u, "building"),
                infantry: flag(u, "infantry"),
                hull: num_or(u, "hull", 0.0)?,
                speed: num(u, "speed")?,
                radius: num(u, "radius")?,
                naval: flag(u, "naval"),
                deep: flag(u, "deep"),
                air: flag(u, "air"),
                rail: flag(u, "rail"),
                weapon: flag(u, "weapon"),
                ability_speed: given(u, "abilitySpeed").and_then(number),
                vehicle: given(u, "vehicle").map(profile).transpose()?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let v = given(t, "vision").ok_or("vision: missing")?;
    let house_vision = given(v, "houseVision")
        .and_then(Value::as_array)
        .ok_or("houseVision: expected an array")?
        .iter()
        .map(|h| {
            if h.is_null() {
                Ok(None)
            } else {
                number(h).map(Some).ok_or("houseVision: expected numbers")
            }
        })
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let vision = VisionRules {
        camo_range: num(v, "camoRange")?,
        dust_seen: num(v, "dustSeen")?,
        air_see_range: num(v, "airSeeRange")?,
        trench_hide: num(v, "trenchHide")?,
        smoke_sight: num(v, "smokeSight")?,
        level_height: num(v, "levelHeight")?,
        eye: num(v, "eye")?,
        min_level: int(num(v, "minLevel")?, "minLevel")?,
        wood_sight: num(v, "woodSight")?,
        high_ground_vision: num(v, "highGroundVision")?,
        garrison_vision: num(v, "garrisonVision")?,
        house_vision,
        rain_sight: num(v, "rainSight")?,
        recon_len: num(v, "reconLen")?,
        recon_width: num(v, "reconWidth")?,
    };
    let weather = given(t, "weather")
        .and_then(Value::as_array)
        .ok_or("weather: expected an array")?
        .iter()
        .map(|w| {
            Ok(WeatherKind {
                name: text(w, "name")?.to_string(),
                sight: num(w, "sight")?,
                rains: flag(w, "rains"),
                inf: num(w, "inf")?,
                veh: num(w, "veh")?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(Rules {
        kinds,
        vision,
        movement: movement_rules(t)?,
        weather,
    })
}

fn pair(v: &Value, key: &str) -> Result<[f64; 2]> {
    let items = given(v, key)
        .and_then(numbers)
        .ok_or_else(|| format!("{key}: expected two numbers"))?;
    items
        .try_into()
        .map_err(|_| format!("{key}: expected two numbers"))
}

fn profile(v: &Value) -> Result<VehicleProfile> {
    Ok(VehicleProfile {
        acceleration: num(v, "acceleration")?,
        reverse_acceleration: num(v, "reverseAcceleration")?,
        braking: num(v, "braking")?,
        reverse_speed: num(v, "reverseSpeed")?,
        hull_turn: num(v, "hullTurn")?,
    })
}

/// The movement numbers (`movement` and `tick` in `harness/tables.mjs`).
fn movement_rules(t: &Value) -> Result<MovementRules> {
    let m = given(t, "movement").ok_or("movement: missing")?;
    let tr = given(m, "traffic").ok_or("movement.traffic: missing")?;
    let lane = given(m, "lane").ok_or("movement.lane: missing")?;
    let terrain_hp = given(m, "terrainHp")
        .and_then(Value::as_object)
        .ok_or("terrainHp: expected an object")?
        .iter()
        .map(|(letter, hp)| match (letter.as_bytes(), number(hp)) {
            ([ch], Some(hp)) => Ok((*ch, hp)),
            _ => Err(format!("terrainHp: bad entry {letter}")),
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(MovementRules {
        tick: num(t, "tick")?,
        retreat_speed: num(m, "retreatSpeed")?,
        slope: pair(m, "slope")?,
        ford_speed: pair(m, "fordSpeed")?,
        wet_ford: num(m, "wetFord")?,
        wet_ground: num(m, "wetGround")?,
        wire_speed: num(m, "wireSpeed")?,
        trench_speed: num(m, "trenchSpeed")?,
        trench_rally: num(m, "trenchRally")?,
        trench_arc: num(m, "trenchArc")?,
        wood_speed: num(m, "woodSpeed")?,
        wood_cover: num(m, "woodCover")?,
        road_speed: num(m, "roadSpeed")?,
        mud_speed: pair(m, "mudSpeed")?,
        churn: num(m, "churn")?,
        reverse_speed: num(m, "reverseSpeed")?,
        house_hp: given(m, "houseHp")
            .and_then(numbers)
            .ok_or("houseHp: expected numbers")?,
        terrain_hp,
        corner_bulge: num(m, "cornerBulge")?,
        traffic: TrafficRules {
            horizon: num(tr, "horizon")?,
            wait: num(tr, "wait")?,
            replan: num(tr, "replan")?,
            age: num(tr, "age")?,
            max_yield: num(tr, "maxYield")?,
            ghost: num(tr, "ghost")?,
            ghost_time: num(tr, "ghostTime")?,
        },
        lane: LaneRules {
            ahead: num(lane, "ahead")?,
            margin: num(lane, "margin")?,
            hold: num(lane, "hold")?,
        },
    })
}

fn mode_kind(name: &str) -> Result<ModeKind> {
    Ok(match name {
        "annihilation" => ModeKind::Annihilation,
        "assault" => ModeKind::Assault,
        "classic" => ModeKind::Classic,
        "horde" => ModeKind::Horde,
        "tutorial" => ModeKind::Tutorial,
        "world" => ModeKind::World,
        other => return Err(format!("unknown mode kind {other}")),
    })
}

fn plane_state(name: &str) -> Result<PlaneState> {
    Ok(match name {
        "base" => PlaneState::Base,
        "rearm" => PlaneState::Rearm,
        "out" => PlaneState::Out,
        "station" => PlaneState::Station,
        "home" => PlaneState::Home,
        other => return Err(format!("unknown plane state {other}")),
    })
}

fn garrison(v: &Value, key: &str) -> Result<Option<usize>> {
    let g = num_or(v, key, -1.0)?;
    Ok(if g >= 0.0 { Some(int(g, key)?) } else { None })
}

/// A number the reference reads without a fallback: missing is NaN (as `undefined` in arithmetic and
/// comparisons), `null` is 0, a boolean 0 or 1.
fn js_num(v: &Value, key: &str) -> Result<f64> {
    match field(v, key) {
        None => Ok(f64::NAN),
        Some(Value::Null) => Ok(0.0),
        Some(Value::Bool(b)) => Ok(f64::from(u8::from(*b))),
        Some(x) => number(x).ok_or_else(|| format!("{key}: expected a number")),
    }
}

fn point(v: &Value) -> Result<Point> {
    Ok(Point {
        x: num(v, "x")?,
        z: num(v, "z")?,
    })
}

fn pose(v: &Value) -> Result<Pose> {
    Ok(Pose {
        x: num(v, "x")?,
        z: num(v, "z")?,
        rot: num(v, "rot")?,
    })
}

/// A point field that is present and not null (`u.worldGoal` and the like are read for their truth).
fn opt_point(u: &Value, key: &str) -> Result<Option<Point>> {
    given(u, key).map(point).transpose()
}

fn points(v: Option<&Value>, what: &str) -> Result<Vec<Point>> {
    match v {
        None => Ok(Vec::new()),
        Some(v) => v
            .as_array()
            .ok_or_else(|| format!("{what}: expected an array"))?
            .iter()
            .map(point)
            .collect(),
    }
}

/// The unit's route is the array `laneScan.path` or `motionClearance.path` holds: the trace records that as
/// `samePathLane` and `samePathClearance`. Saved states never share it (decoding makes separate arrays).
fn same_path(u: &Value, key: &str) -> bool {
    given(u, key).and_then(Value::as_bool) == Some(true)
}

/// The route array a loaded unit holds; 0 stands for any other.
const LOADED_PATH: u64 = 1;

fn traffic(v: &Value) -> Result<Traffic> {
    Ok(Traffic {
        goal: point(given(v, "goal").ok_or("traffic.goal: missing")?)?,
        legs: points(given(v, "legs"), "traffic.legs")?,
        passing: flag(v, "passing"),
        by: int(num(v, "by")?, "traffic.by")?,
        until: num(v, "until")?,
    })
}

fn lane_scan(v: &Value, same: bool) -> Result<LaneScan> {
    let lane = match given(v, "lane") {
        None => None,
        Some(l) => Some((
            point(given(l, "entry").ok_or("lane.entry: missing")?)?,
            point(given(l, "exit").ok_or("lane.exit: missing")?)?,
        )),
    };
    Ok(LaneScan {
        path: if same { LOADED_PATH } else { 0 },
        until: num(v, "until")?,
        lane,
    })
}

fn clearance(v: &Value, same: bool) -> Result<Clearance> {
    Ok(Clearance {
        goal: pose(given(v, "goal").ok_or("motionClearance.goal: missing")?)?,
        reverse: flag(v, "reverse"),
        path: if same { LOADED_PATH } else { 0 },
        until: num(v, "until")?,
    })
}

/// One unit of a state file (or of a movement trace).
pub fn unit(u: &Value, rules: &Rules, players: usize, cells: usize) -> Result<Unit> {
    let id: UnitId = int(num(u, "id")?, "id")?;
    let type_name = text(u, "type")?;
    let kind = rules
        .kind(type_name)
        .ok_or_else(|| format!("unit {id}: unknown type {type_name}"))?;
    let owner: usize = int(num(u, "owner")?, "owner")?;
    if owner >= players {
        return Err(format!("unit {id}: owner {owner} has no player"));
    }
    for key in ["operative", "convoy", "logistics"] {
        if flag(u, key) {
            return Err(format!(
                "unit {id}: has {key}, which the core does not move yet"
            ));
        }
    }
    let air = if truthy(field(u, "air")) {
        let air = field(u, "air").expect("truthy");
        Some(plane_state(text(air, "state")?)?)
    } else {
        None
    };
    let footprint = match field(u, "cells") {
        Some(c) if truthy(Some(c)) => {
            let list: Vec<u32> = ints(Some(c), "cells")?;
            if list.iter().any(|&c| c as usize >= cells) {
                return Err(format!("unit {id}: a footprint cell lies off the map"));
            }
            Some(list)
        }
        _ => None,
    };
    let some_num = |key: &str| -> Result<Option<f64>> {
        given(u, key)
            .map(|v| number(v).ok_or_else(|| format!("{key}: expected a number")))
            .transpose()
    };
    let attack = truthy(field(u, "attackId"));
    Ok(Unit {
        id,
        kind,
        owner,
        x: num(u, "x")?,
        z: num(u, "z")?,
        rot: num_or(u, "rot", 0.0)?,
        garrison: garrison(u, "garrison")?,
        riding: truthy(field(u, "riding")),
        air,
        still: compared(u, "still")?,
        shot_at: some_num("shotAt")?,
        dust: some_num("dust")?,
        built: compared(u, "built")?,
        cells: footprint,
        heavy: !rules.kinds[kind].infantry,
        hp: js_num(u, "hp")?,
        path: points(given(u, "path"), "path")?,
        path_gen: LOADED_PATH,
        reverse: opt_point(u, "reverse")?,
        pace: js_num(u, "pace")?,
        attack,
        retreating: flag(u, "retreating"),
        sprint: js_num(u, "sprint")?,
        supp: js_num(u, "supp")?,
        hit_from: opt_point(u, "hitFrom")?,
        stuck: js_num(u, "stuck")?,
        repath: js_num(u, "repath")?,
        world_goal: opt_point(u, "worldGoal")?,
        amove: opt_point(u, "amove")?,
        move_speed: num_or(u, "moveSpeed", 0.0)?,
        travel_dir: some_num("travelDir")?,
        vx: some_num("vx")?,
        vz: some_num("vz")?,
        drift: flag(u, "drift"),
        motion_before: given(u, "motionBefore").map(pose).transpose()?,
        clearance: given(u, "motionClearance")
            .map(|v| clearance(v, same_path(u, "samePathClearance")))
            .transpose()?,
        traffic: given(u, "traffic").map(traffic).transpose()?,
        traffic_wait: num_or(u, "trafficWait", 0.0)?,
        traffic_trail: match given(u, "trafficTrail") {
            None => Vec::new(),
            Some(t) => t
                .as_array()
                .ok_or("trafficTrail: expected an array")?
                .iter()
                .map(pose)
                .collect::<Result<Vec<_>>>()?,
        },
        traffic_entered: flag(u, "trafficEntered"),
        queued_behind: int(num_or(u, "queuedBehind", 0.0)?, "queuedBehind")?,
        lane_scan: given(u, "laneScan")
            .map(|v| lane_scan(v, same_path(u, "samePathLane")))
            .transpose()?,
        ghost: num_or(u, "ghost", 0.0)?,
        hold_pos: flag(u, "holdPos"),
        build: flag(u, "build"),
        dig: flag(u, "dig"),
        entrench: flag(u, "entrench"),
        board: int(num_or(u, "board", 0.0)?, "board")?,
        rail: flag(u, "rail"),
    })
}

/// A game state and the fog caches it was saved with.
pub struct Loaded {
    pub game: Game,
    pub fog: Fog,
}

/// The game in a state file (its `game` field). The units must be in ascending id order, which is the
/// reference's Map order when ids only grow (`Game::units`).
pub fn load_game(g: &Value, rules: &Rules) -> Result<Loaded> {
    let w: usize = int(num(g, "w")?, "w")?;
    let h: usize = int(num(g, "h")?, "h")?;
    let n = w * h;
    let flags: Vec<u16> = per_cell(field(g, "flags"), n, "flags")?;
    let height: Option<Vec<i8>> = match given(g, "height") {
        None => None,
        Some(v) => Some(per_cell(Some(v), n, "height")?),
    };
    let house: Vec<u8> = per_cell(field(g, "house"), n, "house")?;
    let players = given(g, "players")
        .and_then(Value::as_array)
        .ok_or("players: expected an array")?
        .iter()
        .map(|p| {
            let visible = match given(p, "visible") {
                None => Vec::new(),
                Some(v) => set_items(v)
                    .ok_or("visible: expected a Set")?
                    .iter()
                    .map(|id| {
                        number(id)
                            .ok_or_else(|| "visible: expected ids".to_string())
                            .and_then(|id| int(id, "visible"))
                    })
                    .collect::<Result<Vec<UnitId>>>()?,
            };
            // the vision pass fills it in the units' order; `Player::sees` relies on that
            if !visible.is_sorted() {
                return Err("visible: not in ascending id order".to_string());
            }
            Ok(Player {
                slot: int(num(p, "slot")?, "slot")?,
                team: int(num(p, "team")?, "team")?,
                visible,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let mut units = BTreeMap::new();
    let mut last = None;
    for (key, u) in
        map_entries(field(g, "units").ok_or("units: missing")?).ok_or("units: expected a Map")?
    {
        let unit = unit(u, rules, players.len(), n)?;
        if number(key) != Some(f64::from(unit.id)) {
            return Err(format!("unit {} is filed under {}", unit.id, short(key)));
        }
        if last.is_some_and(|last| unit.id <= last) {
            return Err(format!(
                "unit {} comes after unit {}: the units are not in ascending id order",
                unit.id,
                last.unwrap_or(0)
            ));
        }
        last = Some(unit.id);
        units.insert(unit.id, unit);
    }
    let smokes = given(g, "smokes")
        .and_then(Value::as_array)
        .ok_or("smokes: expected an array")?
        .iter()
        .enumerate()
        .map(|(i, s)| {
            Ok(Smoke {
                id: i as u32,
                x: num(s, "x")?,
                z: num(s, "z")?,
                r: num(s, "r")?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let strikes = given(g, "strikes")
        .and_then(Value::as_array)
        .ok_or("strikes: expected an array")?
        .iter()
        .map(|s| {
            let live = flag(s, "live");
            let recon = given(s, "kind").and_then(Value::as_str) == Some("recon");
            if !(live && recon) {
                // the passes never read the rest of a strike that is over or is no recon flight
                return Ok(Strike {
                    live,
                    kind: StrikeKind::Other,
                    owner: 0,
                    x: f64::NAN,
                    z: f64::NAN,
                    dir: f64::NAN,
                });
            }
            let owner: usize = int(num(s, "owner")?, "strike owner")?;
            if owner >= players.len() {
                return Err(format!("strike owner {owner} has no player"));
            }
            Ok(Strike {
                live,
                kind: StrikeKind::Recon,
                owner,
                x: num(s, "x")?,
                z: num(s, "z")?,
                dir: num(s, "dir")?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let mut sections = BTreeMap::new();
    if let Some(cells) = given(g, "structuralCells") {
        for (key, s) in map_entries(cells).ok_or("structuralCells: expected a Map")? {
            let c: u32 = int(
                number(key).ok_or("structuralCells: expected cell keys")?,
                "structural cell",
            )?;
            let failed = given(s, "state").and_then(Value::as_str) == Some("failed");
            sections.insert(
                c,
                Section {
                    failed,
                    hp: num(s, "hp")?,
                    max_hp: given(s, "maxHp").and_then(number),
                },
            );
        }
    }
    let mode = match given(g, "mode").and_then(|m| given(m, "kind")) {
        None => None,
        Some(kind) => Some(mode_kind(kind.as_str().ok_or("mode.kind: expected text")?)?),
    };
    let weather = given(g, "weather")
        .and_then(|w| given(w, "now"))
        .and_then(Value::as_str)
        .and_then(|name| rules.weather_kind(name));
    let rain = match given(g, "wx") {
        None => 0.0,
        Some(wx) => num_or(wx, "rain", 0.0)?,
    };
    let floats = |key: &str| -> Result<Option<Vec<f32>>> {
        match given(g, key) {
            None => Ok(None),
            Some(v) => {
                let items = numbers(v).ok_or_else(|| format!("{key}: expected numbers"))?;
                if items.len() != n {
                    return Err(format!("{key}: {} cells, not {n}", items.len()));
                }
                // the reference keeps these as 32-bit floats, which the file holds exactly
                Ok(Some(items.into_iter().map(|x| x as f32).collect()))
            }
        }
    };
    let wrecks = match given(g, "wrecks") {
        None => Vec::new(),
        Some(list) => list
            .as_array()
            .ok_or("wrecks: expected an array")?
            .iter()
            .map(|w| {
                Ok(Wreck {
                    id: None,
                    cell: w
                        .get("c")
                        .and_then(serde_json::Value::as_u64)
                        .map(|c| c as usize),
                    x: num(w, "x")?,
                    z: num(w, "z")?,
                    moving: flag(w, "motion"),
                })
            })
            .collect::<Result<Vec<_>>>()?,
    };
    let game = Game {
        w,
        h,
        flags,
        height,
        house,
        chars: letters(given(g, "chars"), n, "chars")?.unwrap_or_else(|| vec![b'.'; n]),
        ground: letters(given(g, "ground"), n, "ground")?,
        objects: letters(given(g, "objects"), n, "objects")?,
        wear: floats("wear")?,
        cell_hp: floats("cellHp")?.unwrap_or_else(|| vec![0.0; n]),
        trench_front: floats("trenchFront")?,
        tough: given(g, "tough")
            .map(|t| numbers(t).ok_or("tough: expected numbers"))
            .transpose()?,
        wet: given(g, "wx").map(|wx| js_num(wx, "wet")).transpose()?,
        reveal: flag(g, "reveal"),
        wrecks,
        tick: int(num(g, "tick")?, "tick")?,
        vision_tick: given(g, "visionTick")
            .map(|t| {
                number(t)
                    .ok_or("visionTick: expected a number")
                    .map(|t| t as i64)
            })
            .transpose()?,
        terrain_version: int(num_or(g, "terrainVersion", 0.0)?, "terrainVersion")?,
        mode,
        naval: flag(g, "naval"),
        weather,
        rain,
        players,
        units,
        smokes,
        strikes,
        sections,
    };
    let fog = load_fog(g, &game)?;
    Ok(Loaded { game, fog })
}

/// Terrain letters, one per cell: an array of one-letter strings as the game keeps them, a string, or a typed
/// array of character codes (`harness/movement-trace.mjs` writes them so, as runs).
fn letters(v: Option<&Value>, n: usize, what: &str) -> Result<Option<Vec<u8>>> {
    let Some(v) = v else { return Ok(None) };
    let items: Vec<u8> = match v {
        Value::String(s) => s.bytes().collect(),
        Value::Object(_) => ints(Some(v), what)?,
        Value::Array(a) => a
            .iter()
            .map(|c| match c.as_str().map(str::as_bytes) {
                Some([b]) => Ok(*b),
                _ => Err(format!("{what}: expected one letter per cell")),
            })
            .collect::<Result<Vec<_>>>()?,
        _ => return Err(format!("{what}: expected letters")),
    };
    if items.len() != n {
        return Err(format!("{what}: {} cells, not {n}", items.len()));
    }
    Ok(Some(items))
}

/// The fog caches saved with the game: `fogTerrain`, `fogSources` and `fog`.
fn load_fog(g: &Value, game: &Game) -> Result<Fog> {
    let n = game.w * game.h;
    let terrain = match given(g, "fogTerrain") {
        None => None,
        Some(t) => Some(FogTerrain {
            version: int(num(t, "version")?, "fogTerrain.version")?,
            sat: ints(field(t, "sat"), "fogTerrain.sat")?,
            sat_w: int(num(t, "W")?, "fogTerrain.W")?,
            top: ints(field(t, "top"), "fogTerrain.top")?,
            bw: int(num(t, "bw")?, "fogTerrain.bw")?,
            sight: ints(field(t, "sight"), "fogTerrain.sight")?,
            height: match given(t, "height") {
                None => None,
                Some(v) => Some(ints(Some(v), "fogTerrain.height")?),
            },
            stamp: ints(field(t, "stamp"), "fogTerrain.stamp")?,
        }),
    };
    // A kept source remembers the smoke clouds near it by identity; the codec does not keep identity, so
    // the clouds a saved source remembers match none of the game's (as in the reference after decoding).
    let mut foreign_smoke = game.smokes.len() as u32;
    let mut kept = BTreeMap::new();
    if let Some(sources) = given(g, "fogSources") {
        for (key, e) in map_entries(sources).ok_or("fogSources: expected a Map")? {
            let id: UnitId = int(
                number(key).ok_or("fogSources: expected unit ids")?,
                "fogSources id",
            )?;
            let seen = match given(e, "cells") {
                None => None,
                Some(cells) => {
                    let cells: Vec<u32> = ints(Some(cells), "kept cells")?;
                    if cells.iter().any(|&c| c as usize >= n) {
                        return Err(format!("kept source {id}: a cell lies off the map"));
                    }
                    let bounds: Vec<i64> = ints(field(e, "box"), "kept box")?;
                    let smokes = given(e, "smokes")
                        .and_then(Value::as_array)
                        .ok_or("kept smokes: expected an array")?;
                    let smokes = smokes
                        .iter()
                        .map(|_| {
                            foreign_smoke += 1;
                            foreign_smoke
                        })
                        .collect();
                    Some(KeptCells {
                        cells,
                        version: int(num(e, "version")?, "kept version")?,
                        bounds: bounds
                            .try_into()
                            .map_err(|_| "kept box: expected four numbers")?,
                        smokes,
                        reach: num(e, "reach")?,
                        range: num(e, "range")?,
                    })
                }
            };
            kept.insert(
                id,
                KeptSource {
                    x: num(e, "x")?,
                    z: num(e, "z")?,
                    garrison: garrison(e, "garrison")?,
                    seen,
                },
            );
        }
    }
    let mut teams = BTreeMap::new();
    if let Some(fog) = given(g, "fog") {
        for (key, f) in map_entries(fog).ok_or("fog: expected a Map")? {
            let team = int(number(key).ok_or("fog: expected team keys")?, "fog team")?;
            let deltas = match given(f, "deltas") {
                None => Vec::new(),
                Some(d) => map_entries(d)
                    .ok_or("deltas: expected a Map")?
                    .into_iter()
                    .map(|(v, s)| {
                        let v = int(
                            number(v).ok_or("deltas: expected version keys")?,
                            "delta version",
                        )?;
                        let s = s.as_str().ok_or("deltas: expected text")?.to_string();
                        Ok((v, s))
                    })
                    .collect::<Result<Vec<_>>>()?,
            };
            let key = given(f, "key")
                .map(|k| {
                    number(k)
                        .ok_or("fog key: expected a number")
                        .map(|k| k as i64)
                })
                .transpose()?;
            let tf = TeamFog {
                key,
                version: int(num(f, "version")?, "fog version")?,
                vis: per_cell(field(f, "vis"), n, "fog vis")?,
                next: per_cell(field(f, "next"), n, "fog next")?,
                explored: per_cell(field(f, "explored"), n, "fog explored")?,
                deltas,
            };
            teams.insert(team, tf);
        }
    }
    Ok(Fog {
        terrain,
        kept,
        teams,
    })
}
