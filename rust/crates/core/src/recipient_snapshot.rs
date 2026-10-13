//! Internal represented Conquest projections. This is not a complete `s` message
//! or a network contract. Unsupported canonical fields remain explicit.
use crate::{
    catalog::Catalog,
    combat,
    economy::{Mode, Technology},
    game::{CELL, Unit},
    match_runtime::Match,
    math, terrain, vision,
    weather::{WeatherRow, WeatherRules},
};
use serde_json::{Map, Value, json};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SnapshotCapability {
    Air,
    Engineering,
    GateState,
    Operatives,
    CargoCount,
    Plans,
    Production,
    MovementOutcomes,
    ArmyAndSupport,
    WrecksAndFoundMines,
    Orders,
    Terrain,
    Fog,
    ShotsAndFlights,
    TeamEffects,
    Scenario,
    Logistics,
    Points,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SnapshotError {
    UnsupportedMode,
    InvalidRecipient,
    MissingState {
        unit: u32,
        owner: &'static str,
    },
    Unsupported {
        unit: u32,
        capability: SnapshotCapability,
    },
    InvalidNumber,
    Rules,
}

/// Only implemented fields, kept separate from the capability list so callers
/// cannot accidentally serialize a partial result as a complete snapshot.
#[derive(Clone, Debug, PartialEq)]
pub struct RecipientProjection {
    pub fields: Map<String, Value>,
    pub missing: Vec<SnapshotCapability>,
}

/// Capture immutable rows for one send pass after canonical state changes.
/// The Rust borrow prevents state mutation until every recipient is projected.
pub struct SnapshotCache<'a> {
    state: &'a Match,
    rows: Vec<Value>,
}
impl<'a> SnapshotCache<'a> {
    pub fn new(state: &'a Match) -> Result<Self, SnapshotError> {
        if state.options.mode != Mode::Conquest
            || state.world.game.mode.is_some()
            || state.world.geography().is_some()
        {
            return Err(SnapshotError::UnsupportedMode);
        }
        let g = &state.world.game;
        let cells = g.w.checked_mul(g.h).ok_or(SnapshotError::Rules)?;
        if g.tick < 0 {
            return Err(SnapshotError::InvalidNumber);
        }
        if g.w == 0
            || g.h == 0
            || g.flags.len() != cells
            || g.chars.len() != cells
            || g.house.len() != cells
        {
            return Err(SnapshotError::Rules);
        }
        if g.players.len() != state.start.economy.players.len()
            || g.players.iter().enumerate().any(|(slot, p)| {
                p.slot != slot
                    || !(0..6).contains(&p.team)
                    || p.visible.windows(2).any(|ids| ids[0] >= ids[1])
            })
        {
            return Err(SnapshotError::InvalidRecipient);
        }
        for u in g.units.values() {
            if u.kind >= state.rules.kinds.len() || u.owner >= g.players.len() {
                return Err(SnapshotError::Rules);
            }
            finite(&[u.x, u.z])?;
        }
        let rows = state
            .world
            .game
            .units
            .values()
            .map(|u| unit_row(state, u))
            .collect::<Result<_, _>>()?;
        Ok(Self { state, rows })
    }
    pub fn recipient(&self, slot: usize) -> Result<RecipientProjection, SnapshotError> {
        let g = &self.state.world.game;
        if slot >= g.players.len() || slot >= self.state.start.economy.players.len() {
            return Err(SnapshotError::InvalidRecipient);
        }
        let seen = |id| {
            let u = g.units.get(&id);
            g.reveal && u.is_some()
                || u.is_some_and(|u| g.allied(u.owner, slot))
                || g.players[slot].sees(id)
                    && u.is_none_or(|u| u.air.is_none() || vision::airborne(u))
        };
        let units = self
            .rows
            .iter()
            .filter(|row| seen(row[0].as_u64().unwrap() as u32))
            .map(|row| {
                let mut row = row.as_array().unwrap().clone();
                if !seen(row[9].as_u64().unwrap() as u32) {
                    row[9] = json!(0);
                }
                if row[2].as_u64() != Some(slot as u64) {
                    row[11] = json!(0);
                    row[12] =
                        json!(row[12].as_u64().unwrap() & (2047 | 32768 | (7 << 20) | 262144));
                }
                Value::Array(row)
            })
            .collect::<Vec<_>>();
        let mut fields = economy_fields(self.state, slot)?;
        fields.insert("units".into(), json!(units));
        Ok(RecipientProjection {
            fields,
            missing: vec![
                SnapshotCapability::Production,
                SnapshotCapability::MovementOutcomes,
                SnapshotCapability::ArmyAndSupport,
                SnapshotCapability::WrecksAndFoundMines,
                SnapshotCapability::Plans,
                SnapshotCapability::Orders,
                SnapshotCapability::Terrain,
                SnapshotCapability::Fog,
                SnapshotCapability::ShotsAndFlights,
                SnapshotCapability::TeamEffects,
                SnapshotCapability::Scenario,
                SnapshotCapability::Logistics,
                SnapshotCapability::Points,
            ],
        })
    }
}

fn r(n: f64) -> f64 {
    math::round(n * 10.0) / 10.0
}
fn finite(values: &[f64]) -> Result<(), SnapshotError> {
    if values
        .iter()
        .all(|n| n.is_finite() && (n * 10.0).is_finite())
    {
        Ok(())
    } else {
        Err(SnapshotError::InvalidNumber)
    }
}
fn unit_row(state: &Match, u: &Unit) -> Result<Value, SnapshotError> {
    let g = &state.world.game;
    let k = state.rules.kinds.get(u.kind).ok_or(SnapshotError::Rules)?;
    let unsupported = |capability| SnapshotError::Unsupported {
        unit: u.id,
        capability,
    };
    if u.air.is_some() || k.air {
        return Err(unsupported(SnapshotCapability::Air));
    }
    if u.dig || u.entrench || u.build {
        return Err(unsupported(SnapshotCapability::Engineering));
    }
    let def = Catalog::compiled()
        .map_err(|_| SnapshotError::Rules)?
        .unit(&k.name)
        .map_err(|_| SnapshotError::Rules)?;
    if def.get("gate").is_some() {
        return Err(unsupported(SnapshotCapability::GateState));
    }
    let control = state
        .commands
        .units
        .get(&u.id)
        .ok_or(SnapshotError::MissingState {
            unit: u.id,
            owner: "control",
        })?;
    if control.operative {
        return Err(unsupported(SnapshotCapability::Operatives));
    }
    let ability = state
        .abilities
        .get(u.id)
        .ok_or(SnapshotError::MissingState {
            unit: u.id,
            owner: "abilities",
        })?;
    let recovery = state
        .recovery
        .get(u.id)
        .ok_or(SnapshotError::MissingState {
            unit: u.id,
            owner: "recovery",
        })?;
    let history = state.combat.get(u.id).ok_or(SnapshotError::MissingState {
        unit: u.id,
        owner: "combat",
    })?;
    if history.cargo.len() > 3 {
        return Err(unsupported(SnapshotCapability::CargoCount));
    }
    let firing = state.firing.get(u.id).ok_or(SnapshotError::MissingState {
        unit: u.id,
        owner: "firing",
    })?;
    finite(&[
        u.x,
        u.z,
        u.rot,
        control.aim,
        u.hp,
        u.supp,
        ability.cooldown,
        ability.buff,
        recovery.reinforcement,
        history.xp,
        u.built.unwrap_or(1.0),
    ])?;
    let mut flags = 0u64;
    if u.riding {
        flags |= 262144;
    }
    if !history.cargo.is_empty() {
        flags |= 524288 | (history.cargo.len() as u64) << 23;
    }
    if u.retreating {
        flags |= 1;
    }
    if ability.buff > 0.0 {
        flags |= 2;
    }
    if ability.ap {
        flags |= 4;
    }
    if recovery.reinforcement > 0.0
        || recovery
            .heal_at
            .is_some_and(|at| g.tick.saturating_sub(at) < 12)
    {
        flags |= 8;
    }
    if let Some(cell) = u.garrison {
        flags |= 32 | u64::from(*g.house.get(cell).ok_or(SnapshotError::Rules)?) << 20;
    }
    if u.amove.is_some() {
        flags |= 64;
    }
    if k.camo
        && u.still.is_some_and(|v| v >= 3.0)
        && g.tick as f64 - u.shot_at.unwrap_or(-1e9) >= 80.0
    {
        flags |= 256;
    }
    if vision::dusty(g, u) {
        flags |= 32768;
    }
    if control.hold_fire {
        flags |= 2048;
    }
    if u.hold_pos {
        flags |= 4096;
    }
    if control.auto_retreat {
        flags |= 8192;
    }
    if control.auto_cast {
        flags |= 16384;
    }
    let cover = if vision::in_trench(g, &state.rules, u) {
        2
    } else if terrain::in_cover(g, &state.rules, u) {
        1
    } else if k.infantry && near_cover(state, u)? {
        3
    } else {
        0
    };
    let mut row = vec![
        json!(u.id),
        json!(k.name),
        json!(u.owner),
        json!(r(u.x)),
        json!(r(u.z)),
        json!(r(u.rot)),
        json!(r(control.aim)),
        json!(u.hp.ceil()),
        json!(math::round(u.supp)),
        json!(firing.target_id),
        json!(cover),
        json!(math::max(0., ability.cooldown.ceil())),
        json!(flags),
        json!(combat::veterancy(&k.name, history.xp).map_err(|_| SnapshotError::Rules)?),
        json!(u.built.unwrap_or(1.0)),
    ];
    if k.ground_vehicle() {
        let tail = [
            u.move_speed,
            u.vx.unwrap_or(0.),
            u.vz.unwrap_or(0.),
            u.travel_dir.unwrap_or(u.rot),
        ];
        finite(&tail)?;
        row.extend(tail.into_iter().map(|n| json!(r(n))));
    }
    Ok(Value::Array(row))
}
fn near_cover(state: &Match, u: &Unit) -> Result<bool, SnapshotError> {
    let g = &state.world.game;
    let x = (u.x / CELL).floor();
    let z = (u.z / CELL).floor();
    if x < i64::MIN as f64 || x >= i64::MAX as f64 || z < i64::MIN as f64 || z >= i64::MAX as f64 {
        return Err(SnapshotError::InvalidNumber);
    }
    let width = i64::try_from(g.w).map_err(|_| SnapshotError::Rules)?;
    let c = (z as i64)
        .checked_mul(width)
        .and_then(|c| c.checked_add(x as i64))
        .ok_or(SnapshotError::InvalidNumber)?;
    let at = |offset: i64| {
        c.checked_add(offset)
            .and_then(|c| usize::try_from(c).ok())
            .and_then(|c| g.chars.get(c))
            .copied()
    };
    if [1, -1, g.w as i64, -(g.w as i64)]
        .iter()
        .any(|d| at(*d).is_some_and(|ch| b"B#RHKQ".contains(&ch)))
    {
        return Ok(true);
    }
    if [
        g.w as i64 + 1,
        g.w as i64 - 1,
        -(g.w as i64) + 1,
        -(g.w as i64) - 1,
    ]
    .iter()
    .any(|d| at(*d).is_some_and(|ch| b"BK".contains(&ch)))
    {
        return Ok(true);
    }
    Ok(g.units.values().any(|v| {
        v.id != u.id
            && v.air.is_none()
            && !state.rules.kinds[v.kind].infantry
            && math::hypot(u.x - v.x, u.z - v.z) < 3.5
    }))
}
fn economy_fields(state: &Match, slot: usize) -> Result<Map<String, Value>, SnapshotError> {
    let p = &state.start.economy.players[slot];
    finite(&[p.mp, p.mun, p.fuel, p.inc, p.vp])?;
    let mut fields = Map::new();
    // Ordinary Conquest does not initialize the source player mun/fuel fields.
    // Canonical accounting zeros do not make those wire fields present.
    for (key, value) in [("mp", p.mp.floor()), ("inc", r(p.inc))] {
        fields.insert(key.into(), json!(value));
    }
    for (key, value) in [("fuelInc", p.fuel_inc), ("upkeep", p.upkeep)] {
        if let Some(value) = value {
            finite(&[value])?;
            fields.insert(key.into(), json!(r(value)));
        }
    }
    fields.insert("tick".into(), json!(state.world.game.tick));
    fields.insert(
        "out".into(),
        json!(
            state
                .start
                .economy
                .players
                .iter()
                .map(|p| p.out)
                .collect::<Vec<_>>()
        ),
    );
    for player in &state.start.economy.players {
        finite(&[player.vp])?;
    }
    fields.insert(
        "vp".into(),
        json!(
            state
                .start
                .economy
                .players
                .iter()
                .map(|p| p.vp.floor())
                .collect::<Vec<_>>()
        ),
    );
    fields.insert(
        "winner".into(),
        state.finish.map_or(Value::Null, |end| json!(end.winner)),
    );
    if let Some(end) = state.finish {
        fields.insert(
            "end".into(),
            json!({"reason":end.reason.as_str(),"x":end.end_at.x,"z":end.end_at.z}),
        );
    }
    fields.insert(
        "rally".into(),
        state
            .player_rallies
            .get(slot)
            .ok_or(SnapshotError::InvalidRecipient)?
            .map_or(Value::Null, |p| json!([r(p.x), r(p.z)])),
    );
    if state.options.tech {
        let research = state
            .production
            .player(slot)
            .ok_or(SnapshotError::InvalidRecipient)?;
        let tech = research.tech();
        let lab = research
            .jobs()
            .iter()
            .map(|job| {
                finite(&[job.progress, job.time])?;
                if job.time <= 0. {
                    return Err(SnapshotError::InvalidNumber);
                }
                let name = match job.kind {
                    Technology::Tier => "tier",
                    Technology::Infantry => "inf",
                    Technology::Guns => "guns",
                    Technology::Armor => "armor",
                };
                Ok(json!([name, r(job.progress / job.time)]))
            })
            .collect::<Result<Vec<_>, _>>()?;
        fields.insert("tech".into(),json!({"tier":tech.tier(),"armory":{"inf":tech.level(Technology::Infantry),"guns":tech.level(Technology::Guns),"armor":tech.level(Technology::Armor)},"lab":lab}));
    }
    let weather = WeatherRules::compiled()
        .map_err(|_| SnapshotError::Rules)?
        .weather_row(Some(&state.start.weather), state.world.game.tick as u64);
    fields.insert(
        "weather".into(),
        match weather {
            WeatherRow::Current(now) => json!([now.name()]),
            WeatherRow::Changing { now, next, seconds } => {
                json!([now.name(), next.name(), seconds])
            }
        },
    );
    Ok(fields)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        economy::Faction,
        economy_state::ArmySize,
        game::Point,
        map::AuthoritativeMap,
        match_commands::UnitControl,
        match_runtime::{MatchOptions, PlayerConfig},
        unit_spawn,
        weather::WeatherChoice,
    };
    fn n(v: &Value, key: &str) -> f64 {
        v[key].as_f64().unwrap()
    }
    fn optional(v: &Value, key: &str) -> Option<f64> {
        v[key].as_f64()
    }
    fn initialize(case: &Value) -> Match {
        let map = AuthoritativeMap::from_json(&serde_json::to_vec(&case["map"]).unwrap()).unwrap();
        let mut m = Match::new(
            &map,
            MatchOptions {
                seed: 1,
                mode: Mode::Conquest,
                players: (0..3)
                    .map(|i| PlayerConfig {
                        name: format!("P{i}"),
                        team: if i == 2 { 1 } else { 0 },
                        faction: Faction::try_from(i).unwrap(),
                    })
                    .collect(),
                shuffle: false,
                army: ArmySize::Standard,
                weather: WeatherChoice::Fixed(crate::weather::WeatherKind::Clear),
                weather_seed: Some(1),
                living_weather: true,
                map_key: None,
                tech: true,
                supply: true,
                logistics: false,
                sandbox: false,
            },
        )
        .unwrap();
        m.world.game.tick = case["tick"].as_i64().unwrap();
        m.world.game.chars = case["chars"].as_str().unwrap().as_bytes().to_vec();
        m.world.game.units.clear();
        m.commands.units.clear();
        let catalog = Catalog::compiled().unwrap();
        for v in case["inputs"].as_array().unwrap() {
            let id = v["id"].as_u64().unwrap() as u32;
            let kind = m
                .rules
                .kinds
                .iter()
                .position(|k| k.name == v["type"].as_str().unwrap())
                .unwrap();
            let owner = v["owner"].as_u64().unwrap() as usize;
            let mut u = unit_spawn::initial_unit(
                &m.rules,
                catalog,
                kind,
                id,
                owner,
                Point {
                    x: n(v, "x"),
                    z: n(v, "z"),
                },
            )
            .unwrap();
            u.rot = n(v, "rot");
            u.hp = n(v, "hp");
            u.supp = n(v, "supp");
            u.still = optional(v, "still");
            u.shot_at = optional(v, "shotAt");
            u.dust = optional(v, "dust");
            u.retreating = v["retreating"].as_bool().unwrap_or(false);
            u.hold_pos = v["holdPos"].as_bool().unwrap_or(false);
            u.built = optional(v, "built");
            u.move_speed = optional(v, "moveSpeed").unwrap_or(0.);
            u.vx = optional(v, "vx");
            u.vz = optional(v, "vz");
            u.travel_dir = optional(v, "travelDir");
            u.riding = v["riding"].as_u64().is_some_and(|n| n > 0);
            u.garrison = v["garrison"]
                .as_i64()
                .filter(|n| *n >= 0)
                .map(|n| n as usize);
            let mut control = UnitControl::new(&u, &m.rules).unwrap();
            control.aim = n(v, "aim");
            control.hold_fire = v["holdFire"].as_bool().unwrap_or(false);
            control.auto_retreat = v["autoRetreat"].as_bool().unwrap_or(false);
            control.auto_cast = v["auto"].as_bool().unwrap_or(false);
            m.commands.units.insert(id, control);
            let ability = m.abilities.initialize(id);
            ability.cooldown = n(v, "cd");
            ability.buff = optional(v, "buff").unwrap_or(0.);
            ability.ap = v["ap"].as_bool().unwrap_or(false);
            let recovery = m.recovery.initialize(id);
            recovery.reinforcement = optional(v, "reinf").unwrap_or(0.);
            recovery.heal_at = v["healAt"].as_i64();
            let combat = m.combat.initialize(id);
            combat.xp = optional(v, "xp").unwrap_or(0.);
            combat.cargo = v["cargo"].as_array().map_or_else(Vec::new, |a| {
                a.iter().map(|v| v.as_u64().unwrap() as u32).collect()
            });
            let firing = m.firing.initialize(id);
            firing.target_id = v["targetId"].as_u64().unwrap_or(0) as u32;
            firing.cooldown = optional(v, "cooldown").unwrap_or(0.);
            m.world.game.units.insert(id, u);
        }
        for (slot, p) in case["players"].as_array().unwrap().iter().enumerate() {
            let economy = &mut m.start.economy.players[slot];
            economy.mp = n(p, "mp");
            economy.mun = optional(p, "mun").unwrap_or(0.);
            economy.fuel = optional(p, "fuel").unwrap_or(0.);
            economy.inc = n(p, "inc");
            economy.vp = optional(p, "vp").unwrap_or(0.);
            economy.fuel_inc = optional(p, "fuelInc");
            economy.upkeep = optional(p, "upkeep");
            m.player_rallies[slot] = p["rally"].as_object().map(|_| Point {
                x: n(&p["rally"], "x"),
                z: n(&p["rally"], "z"),
            });
            m.world.game.players[slot].visible = p["visible"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_u64().unwrap() as u32)
                .collect();
        }
        for p in &mut m.world.game.players {
            p.visible.sort_unstable();
        }
        if case["finished"] == true {
            m.apply_conquest_victory().unwrap();
        }
        m
    }
    fn equal(actual: &Value, expected: &Value, path: &str) {
        match (actual, expected) {
            (Value::Number(a), Value::Number(b)) => assert_eq!(a.as_f64(), b.as_f64(), "{path}"),
            (Value::Array(a), Value::Array(b)) => {
                assert_eq!(a.len(), b.len(), "{path}");
                for (i, (a, b)) in a.iter().zip(b).enumerate() {
                    equal(a, b, &format!("{path}[{i}]"));
                }
            }
            (Value::Object(a), Value::Object(b)) => {
                assert_eq!(a.len(), b.len(), "{path}");
                for (k, b) in b {
                    equal(&a[k], b, &format!("{path}.{k}"));
                }
            }
            _ => assert_eq!(actual, expected, "{path}"),
        }
    }
    #[test]
    fn represented_fields_match_actual_cached_reference_snapshots() {
        let fixture: Value =
            serde_json::from_str(include_str!("../tests/fixtures/recipient-snapshot.json"))
                .unwrap();
        for case in fixture["cases"].as_array().unwrap() {
            let m = initialize(case);
            let cache = SnapshotCache::new(&m).unwrap();
            for (slot, expected) in case["outputs"].as_array().unwrap().iter().enumerate() {
                let projection = cache.recipient(slot).unwrap();
                equal(
                    &Value::Object(projection.fields),
                    expected,
                    &format!("{} recipient{slot}", case["name"]),
                );
                assert!(!projection.missing.is_empty());
            }
        }
    }
    #[test]
    fn absent_canonical_metadata_and_unsupported_features_are_never_zero_filled() {
        let fixture: Value =
            serde_json::from_str(include_str!("../tests/fixtures/recipient-snapshot.json"))
                .unwrap();
        let mut m = initialize(&fixture["cases"][0]);
        assert!(matches!(
            SnapshotCache::new(&m).unwrap().recipient(3),
            Err(SnapshotError::InvalidRecipient)
        ));
        let saved_kind = m.world.game.units[&2].kind;
        m.world.game.units.get_mut(&2).unwrap().kind = usize::MAX;
        assert!(matches!(SnapshotCache::new(&m), Err(SnapshotError::Rules)));
        m.world.game.units.get_mut(&2).unwrap().kind = saved_kind;
        let z = m.world.game.units[&1].z;
        m.world.game.units.get_mut(&1).unwrap().z = 1e20;
        assert!(matches!(
            SnapshotCache::new(&m),
            Err(SnapshotError::InvalidNumber)
        ));
        m.world.game.units.get_mut(&1).unwrap().z = z;
        m.abilities.remove(1);
        assert!(matches!(
            SnapshotCache::new(&m),
            Err(SnapshotError::MissingState {
                unit: 1,
                owner: "abilities"
            })
        ));
        m.abilities.initialize(1);
        m.world.game.units.get_mut(&1).unwrap().dig = true;
        assert!(matches!(
            SnapshotCache::new(&m),
            Err(SnapshotError::Unsupported {
                unit: 1,
                capability: SnapshotCapability::Engineering
            })
        ));
    }
}
