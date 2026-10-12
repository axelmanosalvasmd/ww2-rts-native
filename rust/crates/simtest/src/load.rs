//! Builds the core's rules and game state from the harness's JSON: `harness/tables.mjs` for the rules, and a
//! game state from `harness/sim-state.mjs` or `harness/vision-fixture.mjs` (written by
//! `harness/state-codec.mjs`). Only the fields the vision and fog passes read are taken.

use std::collections::BTreeMap;

use serde_json::Value;
use ww2_core::fog::{Fog, FogTerrain, KeptCells, KeptSource, TeamFog};
use ww2_core::game::{
    CELL, Game, MOVE, ModeKind, PlaneState, Player, SIGHT, Section, Smoke, Strike, StrikeKind,
    TRENCH, Unit, WOOD,
};
use ww2_core::grid::UnitId;
use ww2_core::rules::{Rules, UnitKind, VisionRules, WeatherKind};

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
        ("trench", TRENCH),
        ("wood", WOOD),
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
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(Rules {
        kinds,
        vision,
        weather,
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

fn unit(u: &Value, rules: &Rules, players: usize, cells: usize) -> Result<Unit> {
    let id: UnitId = int(num(u, "id")?, "id")?;
    let type_name = text(u, "type")?;
    let kind = rules
        .kind(type_name)
        .ok_or_else(|| format!("unit {id}: unknown type {type_name}"))?;
    let owner: usize = int(num(u, "owner")?, "owner")?;
    if owner >= players {
        return Err(format!("unit {id}: owner {owner} has no player"));
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
        shot_at: given(u, "shotAt")
            .map(|v| number(v).ok_or("shotAt: expected a number"))
            .transpose()?,
        dust: given(u, "dust")
            .map(|v| number(v).ok_or("dust: expected a number"))
            .transpose()?,
        built: compared(u, "built")?,
        cells: footprint,
        heavy: !rules.kinds[kind].infantry,
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
    let game = Game {
        w,
        h,
        flags,
        height,
        house,
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
