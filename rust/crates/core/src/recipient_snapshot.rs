//! Internal ordinary-mode projections and an explicitly admitted seat envelope.
//! Unsupported canonical actors remain explicit. No transport contract is added.
//! Projectile and aviation fields follow the pinned prototype recipient rules.
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
#[path = "recipient_snapshot/aircraft.rs"]
mod aircraft;
#[path = "recipient_snapshot/composition.rs"]
mod composition;
#[path = "recipient_snapshot/delivery.rs"]
mod delivery;
#[cfg(test)]
#[path = "recipient_snapshot/elimination_tests.rs"]
mod elimination_tests;
#[path = "recipient_snapshot/engineering.rs"]
mod engineering;
#[path = "recipient_snapshot/falling.rs"]
mod falling;
#[path = "recipient_snapshot/flights.rs"]
mod flights;
#[path = "recipient_snapshot/logistics.rs"]
mod logistics;
#[path = "recipient_snapshot/modes.rs"]
mod modes;
#[path = "recipient_snapshot/operative.rs"]
mod operative;
#[path = "recipient_snapshot/operative_flights.rs"]
mod operative_flights;
#[path = "recipient_snapshot/owners.rs"]
mod owners;
#[path = "recipient_snapshot/projectiles.rs"]
mod projectiles;
#[path = "recipient_snapshot/public_state.rs"]
mod public_state;
#[path = "recipient_snapshot/selected.rs"]
mod selected;
#[path = "recipient_snapshot/shots.rs"]
mod shots;
#[path = "recipient_snapshot/spectator.rs"]
mod spectator;
#[path = "recipient_snapshot/support.rs"]
mod support;
pub use composition::{GroundListDeliveryFrame, QuiescentDeliveryFrame, SupportedSeatSnapshot};
pub use delivery::{DeliveryFrame, DeliveryState};
pub use modes::mode_row;
pub use operative::OperativeDelivery;
pub use selected::{SelectedDeliveryFrame, SelectedGroundListFrame, SelectedSeat};
pub use shots::{SelectedGroundShotFrame, ShotRef};
pub use spectator::SpectatorDelivery;
pub use support::SUPPORTED_SUPPORT_KINDS;

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

/// Implemented fields, kept separate from the capability list so callers
/// cannot accidentally serialize a partial result as a complete snapshot.
#[derive(Clone, Debug, PartialEq)]
pub struct RecipientProjection {
    pub fields: Map<String, Value>,
    pub missing: Vec<SnapshotCapability>,
}

/// The public Horde mode row excludes seat data and hidden purchase state.
pub fn horde_mode_row(state: &Match) -> Result<Value, SnapshotError> {
    let mode = state.horde_projection().map_err(|_| SnapshotError::Rules)?;
    let horde = state.horde.as_ref().ok_or(SnapshotError::Rules)?;
    let mut row = json!({
        "kind": "horde", "defenderTeam": 0, "attackerTeam": 1,
        "timeLeft": mode.time_left, "suddenDeath": false,
        "wave": mode.wave,
        "left": mode.left, "active": mode.active,
        "slot": horde.slot
    });
    if mode.wave > 0 {
        row["profile"] = json!(mode.profile.as_str());
        row["boss"] = json!(mode.boss);
    }
    if let Some(profile) = mode.next_profile {
        row["nextProfile"] = json!(profile.as_str());
    }
    Ok(row)
}

/// Capture immutable rows for one send pass after canonical state changes.
/// The Rust borrow prevents state mutation until every recipient is projected.
pub struct SnapshotCache<'a> {
    state: &'a Match,
    rows: Vec<Value>,
    owners: Vec<Map<String, Value>>,
    public: Vec<Map<String, Value>>,
}

/// Owned send facts can pass through mutable terrain/list publication without
/// rebuilding unit rows. Those publications do not change these facts.
struct SnapshotRows {
    rows: Vec<Value>,
    owners: Vec<Map<String, Value>>,
    public: Vec<Map<String, Value>>,
}
#[cfg(test)]
thread_local! { static CACHE_BUILDS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }

impl<'a> SnapshotCache<'a> {
    pub fn new(state: &'a Match) -> Result<Self, SnapshotError> {
        validate_snapshot(state)?;
        #[cfg(test)]
        CACHE_BUILDS.with(|count| count.set(count.get() + 1));
        let rows = state
            .world
            .game
            .units
            .values()
            .map(|u| unit_row(state, u))
            .collect::<Result<_, _>>()?;
        let owners = owners::capture(state)?;
        let public = public_state::capture(state)?;
        let support = support::capture(state)?;
        let mut public = public;
        for (fields, support) in public.iter_mut().zip(support) {
            fields.extend(support);
        }
        for (fields, works) in public.iter_mut().zip(engineering::capture(state)?) {
            fields.insert("works".into(), works);
        }
        Ok(Self {
            state,
            rows,
            owners,
            public,
        })
    }
    fn into_rows(self) -> SnapshotRows {
        SnapshotRows {
            rows: self.rows,
            owners: self.owners,
            public: self.public,
        }
    }
    fn with_rows(state: &'a Match, rows: SnapshotRows) -> Self {
        Self {
            state,
            rows: rows.rows,
            owners: rows.owners,
            public: rows.public,
        }
    }
    pub fn recipient(&self, slot: usize) -> Result<RecipientProjection, SnapshotError> {
        let g = &self.state.world.game;
        if slot >= g.players.len() || slot >= self.state.start.economy.players.len() {
            return Err(SnapshotError::InvalidRecipient);
        }
        let units = self
            .rows
            .iter()
            .filter(|row| seen_by(self.state, slot, row[0].as_u64().unwrap() as u32))
            .map(|row| recipient_unit_row(self.state, slot, row.clone()))
            .collect::<Vec<_>>();
        project_recipient(
            self.state,
            slot,
            units,
            self.owners[slot].clone(),
            self.public[slot].clone(),
        )
    }
}

fn validate_snapshot(state: &Match) -> Result<(), SnapshotError> {
    if !matches!(
        state.options.mode,
        Mode::Default
            | Mode::Conquest
            | Mode::Assault
            | Mode::Annihilation
            | Mode::Classic
            | Mode::Tutorial
            | Mode::Horde
    ) || !crate::match_actor::ordinary_ground_mode(&state.world.game, state.options.mode)
        || state.world.geography().is_some()
        || ((state.options.mode == Mode::Assault) != state.assault.is_some())
        || state.world_runtime.is_some()
        || state.world.game.mode.is_some() && state.validate_ordinary_finish_state().is_err()
        || matches!(state.options.mode, Mode::Annihilation | Mode::Classic)
            && state.validate_elimination_state().is_err()
    {
        return Err(SnapshotError::UnsupportedMode);
    }
    if state.options.mode == Mode::Tutorial {
        state
            .validate_tutorial_state()
            .map_err(|_| SnapshotError::Rules)?;
    }
    if state.options.mode == Mode::Assault {
        state
            .validate_match_finish_state()
            .map_err(|_| SnapshotError::Rules)?;
    }
    if matches!(state.options.mode, Mode::Annihilation | Mode::Classic) {
        state
            .validate_elimination_state()
            .map_err(|_| SnapshotError::Rules)?;
    }
    if state.options.mode == Mode::Horde {
        state
            .validate_horde_economy()
            .map_err(|_| SnapshotError::Rules)?;
    }
    if let Some(assault) = state.assault
        && (!(0..6).contains(&assault.defender_team)
            || !(-1..6).contains(&assault.attacker_team)
            || assault.defender_team == assault.attacker_team
            || !assault.time_left.is_finite())
    {
        return Err(SnapshotError::Rules);
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
    for (id, u) in &g.units {
        if *id != u.id
            || u.id == 0
            || u.kind >= state.rules.kinds.len()
            || u.owner >= g.players.len()
        {
            return Err(SnapshotError::Rules);
        }
        finite(&[u.x, u.z])?;
    }
    if !state.firing.flights.is_empty()
        || !state.firing.salvos.is_empty()
        || !state.firing.grenades.is_empty()
    {
        state
            .validate_firing_tick_state()
            .map_err(|_| SnapshotError::Rules)?;
        state
            .validate_firing_salvos()
            .map_err(|_| SnapshotError::Rules)?;
        state
            .validate_firing_grenade_step(0.)
            .map_err(|_| SnapshotError::Rules)?;
    }
    projectiles::validate(state)?;
    engineering::validate(state)?;
    aircraft::validate(state)?;
    state
        .prepare_companion_tick()
        .map_err(|_| SnapshotError::Unsupported {
            unit: g
                .units
                .values()
                .find(|unit| {
                    state
                        .commands
                        .units
                        .get(&unit.id)
                        .is_some_and(|control| control.operative)
                        && !state.companions.soldiers.contains_key(&unit.id)
                })
                .map_or(0, |unit| unit.id),
            capability: SnapshotCapability::Operatives,
        })?;
    Ok(())
}

/// Build one recipient directly from canonical state, without a send cache.
/// Owner decision P uses the cached prototype mask for both native paths.
pub fn recipient_uncached(
    state: &Match,
    slot: usize,
) -> Result<RecipientProjection, SnapshotError> {
    validate_snapshot(state)?;
    if slot >= state.world.game.players.len() {
        return Err(SnapshotError::InvalidRecipient);
    }
    let units = state
        .world
        .game
        .units
        .values()
        .map(|unit| unit_row(state, unit))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|row| seen_by(state, slot, row[0].as_u64().unwrap() as u32))
        .map(|row| recipient_unit_row(state, slot, row))
        .collect::<Vec<_>>();
    let owners = owners::capture(state)?;
    let mut public = public_state::capture(state)?;
    for (fields, support) in public.iter_mut().zip(support::capture(state)?) {
        fields.extend(support);
    }
    for (fields, works) in public.iter_mut().zip(engineering::capture(state)?) {
        fields.insert("works".into(), works);
    }
    project_recipient(
        state,
        slot,
        units,
        owners[slot].clone(),
        public[slot].clone(),
    )
}

fn recipient_unit_row(state: &Match, slot: usize, row: Value) -> Value {
    let Value::Array(mut row) = row else {
        unreachable!("unit_row always produces an array")
    };
    if !seen_by(state, slot, row[9].as_u64().unwrap() as u32) {
        row[9] = json!(0);
    }
    if row[2].as_u64() != Some(slot as u64) {
        row[11] = json!(0);
        row[12] = json!(row[12].as_u64().unwrap() & (2047 | 32768 | (7 << 20) | 262144));
    }
    Value::Array(row)
}

fn project_recipient(
    state: &Match,
    slot: usize,
    units: Vec<Value>,
    owners: Map<String, Value>,
    public: Map<String, Value>,
) -> Result<RecipientProjection, SnapshotError> {
    let mut fields = economy_fields(state, slot)?;
    fields.insert("units".into(), json!(units));
    fields.extend(owners);
    fields.extend(public);
    fields.insert("falling".into(), falling::project(state, slot)?);
    fields.insert("flights".into(), flights::project(state, slot)?);
    if let Some(logistics) = logistics::project(state, slot)? {
        fields.insert("logistics".into(), logistics);
    }
    Ok(RecipientProjection {
        fields,
        missing: vec![
            SnapshotCapability::WrecksAndFoundMines,
            SnapshotCapability::Terrain,
            SnapshotCapability::Fog,
            SnapshotCapability::ShotsAndFlights,
            SnapshotCapability::TeamEffects,
        ],
    })
}

pub(crate) fn seen_by(state: &Match, slot: usize, id: u32) -> bool {
    let g = &state.world.game;
    let u = g.units.get(&id);
    g.reveal && u.is_some()
        || u.is_some_and(|u| g.allied(u.owner, slot))
        || g.players[slot].sees(id) && u.is_none_or(|u| u.air.is_none() || vision::airborne(u))
}

pub(crate) fn movement_for(state: &Match, slot: usize) -> Result<Value, SnapshotError> {
    owners::movement_rows(state, slot)
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
    let control = state
        .commands
        .units
        .get(&u.id)
        .ok_or(SnapshotError::MissingState {
            unit: u.id,
            owner: "control",
        })?;
    state
        .validate_gate_actor(u.id)
        .map_err(|_| unsupported(SnapshotCapability::GateState))?;
    crate::match_construction::validate_build_activity(u, control)
        .map_err(|_| unsupported(SnapshotCapability::Engineering))?;
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
    if control.operative {
        flags |= 131072;
    }
    if u.air.is_some() && !vision::airborne(u) {
        flags |= 512;
    }
    if control.gate.is_some_and(|gate| gate.open == Some(true)) {
        flags |= 65536;
    }
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
    if u.dig {
        flags |= 16;
    }
    if u.entrench {
        flags |= 1024;
    }
    if let Some(cell) = u.garrison {
        flags |= 32 | u64::from(*g.house.get(cell).ok_or(SnapshotError::Rules)?) << 20;
    }
    if u.amove.is_some() {
        flags |= 64;
    }
    if u.build {
        flags |= 128;
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
    } else if u.dig {
        let progress = state
            .field_works
            .progress(&state.world, u.id)
            .map_err(|_| SnapshotError::Rules)?;
        finite(&[progress])?;
        row.push(json!(math::round(progress * 100.0) / 100.0));
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
    // Classic initializes these wallets. Other admitted ordinary modes leave
    // them undefined in the prototype, even though native accounting has zeros.
    for (key, value) in [("mp", p.mp.floor()), ("inc", r(p.inc))] {
        fields.insert(key.into(), json!(value));
    }
    if state.options.mode == Mode::Classic {
        fields.insert("mun".into(), json!(p.mun.floor()));
        fields.insert("fuel".into(), json!(p.fuel.floor()));
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
        finite(&[end.end_at.x, end.end_at.z])?;
        if end.end_tick < 0
            || end.end_tick > 9_007_199_254_740_991
            || !(-1..6).contains(&end.winner)
            || (end.winner >= 0
                && !state
                    .world
                    .game
                    .players
                    .iter()
                    .any(|p| p.team == end.winner))
            || (end.winner == -1) != (end.reason == crate::match_end::EndReason::Draw)
        {
            return Err(SnapshotError::Rules);
        }
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

impl Match {
    /// Bind hosted purchases to the current ordinary snapshot capabilities.
    pub fn restrict_to_ordinary_snapshots(&mut self) -> Result<(), SnapshotError> {
        self.preflight_ordinary_snapshot()?;
        // A queued plane can later emit AntiAir or PlaneDown. Keep hosted
        // training closed until the aviation event adapter is integrated.
        for (id, building) in self.production.buildings() {
            if building.jobs().iter().any(|job| {
                self.rules
                    .kind(&job.kind)
                    .is_some_and(|kind| self.rules.kinds[kind].air)
            }) {
                return Err(SnapshotError::Unsupported {
                    unit: id,
                    capability: SnapshotCapability::Air,
                });
            }
        }
        self.ordinary_snapshot_only = true;
        Ok(())
    }

    /// Read-only admission of the currently supported ordinary actor frame.
    /// Trusted attachment validates every seat before installing the match.
    pub fn preflight_ordinary_snapshot(&self) -> Result<(), SnapshotError> {
        composition::admit_actors(self, &[])?;
        let cache = SnapshotCache::new(self)?;
        let owners = (0..self.world.game.players.len()).collect::<Vec<_>>();
        for &slot in &owners {
            cache.recipient(slot)?;
            self.preflight_observed_terrain(slot)
                .map_err(|_| SnapshotError::Rules)?;
        }
        // Prepare the same ordered lists as delivery without publishing their
        // remembered rows or acknowledging any World observations.
        self.prepare_ordinary_recipient_lists(&owners)
            .map_err(|_| SnapshotError::Rules)?;
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod tests {
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
    pub(crate) fn initialize(case: &Value) -> Match {
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
                tech: case["publicState"]["tech"].as_bool().unwrap_or(true),
                supply: true,
                logistics: false,
                sandbox: false,
            },
        )
        .unwrap();
        m.world.game.tick = case["tick"].as_i64().unwrap();
        m.world.game.chars = case["chars"].as_str().unwrap().as_bytes().to_vec();
        for id in m.world.game.units.keys().copied().collect::<Vec<_>>() {
            m.production.remove_building(id);
        }
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
            let point = |v: &Value| {
                v.as_object().map(|_| Point {
                    x: n(v, "x"),
                    z: n(v, "z"),
                })
            };
            u.amove = point(&v["amove"]);
            u.world_goal = point(&v["worldGoal"]);
            u.board = v["board"].as_u64().unwrap_or(0) as u32;
            u.path = v["path"].as_array().map_or_else(Vec::new, |rows| {
                rows.iter().map(|v| point(v).unwrap()).collect()
            });
            let mut control = UnitControl::new(&u, &m.rules).unwrap();
            control.aim = n(v, "aim");
            control.hold_fire = v["holdFire"].as_bool().unwrap_or(false);
            control.auto_retreat = v["autoRetreat"].as_bool().unwrap_or(false);
            control.auto_cast = v["auto"].as_bool().unwrap_or(false);
            control.attack_id = v["attackId"].as_u64().unwrap_or(0) as u32;
            control.fire_at = v["fireAt"].as_i64().filter(|n| *n >= 0).map(|n| n as usize);
            control.enter = v["enter"].as_i64().filter(|n| *n >= 0).map(|n| n as usize);
            control.nade = point(&v["nade"]);
            if let Some(orders) = v["orders"].as_array() {
                for order in orders {
                    control
                        .orders
                        .push_back(crate::match_commands::Command::Move {
                            orders: vec![crate::match_commands::MoveOrder {
                                unit: i64::from(id),
                                at: Point {
                                    x: n(order, "x"),
                                    z: n(order, "z"),
                                },
                            }],
                            control_ids: None,
                            attack_move: order["t"] == "amove",
                            queue: false,
                            together: false,
                            face: None,
                        });
                }
            }
            if v["queue"].is_array() {
                m.production.initialize_building(id).unwrap();
                m.building_rallies.insert(id, point(&v["rally"]));
                for job in v["productionJobs"].as_array().unwrap() {
                    let actual = m
                        .production
                        .record_paid(
                            id,
                            job["type"].as_str().unwrap(),
                            crate::economy::Price {
                                mp: n(job, "mp"),
                                fuel: n(job, "fuel"),
                            },
                        )
                        .unwrap();
                    assert_eq!(actual, job["id"].as_u64().unwrap());
                }
            }
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
        if let Some(public) = case["publicState"].as_object() {
            m.world.game.strikes = public["strikes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|s| crate::game::Strike {
                    kind: crate::game::StrikeKind::Other,
                    owner: s["owner"].as_u64().unwrap() as usize,
                    x: n(s, "x"),
                    z: n(s, "z"),
                    dir: n(s, "dir"),
                    live: s["live"].as_bool().unwrap_or(false),
                    runtime: Some(crate::match_support::SupportTiming {
                        kind: match s["kind"].as_str().unwrap() {
                            "artillery" => crate::match_support::SupportKind::Artillery,
                            "smoke" => crate::match_support::SupportKind::Smoke,
                            "cover" => crate::match_support::SupportKind::Cover,
                            _ => panic!("unsupported fixture strike"),
                        },
                        delay: n(s, "t"),
                        left: n(s, "left"),
                        next: n(s, "next"),
                    }),
                })
                .collect();
            m.support.covers = public["covers"]
                .as_array()
                .unwrap()
                .iter()
                .map(|c| crate::match_support::FighterCover {
                    team: c["team"].as_i64().unwrap() as i32,
                    owner: c["owner"].as_u64().unwrap() as usize,
                    at: Point {
                        x: n(c, "x"),
                        z: n(c, "z"),
                    },
                    radius: n(c, "r"),
                    remaining: n(c, "t"),
                })
                .collect();
            m.options.army = match public["army"].as_str().unwrap() {
                "standard" => ArmySize::Standard,
                "large" => ArmySize::Large,
                "massive" => ArmySize::Massive,
                "endless" => ArmySize::Endless,
                _ => panic!("invalid fixture army"),
            };
            m.start.wind = (n(&public["wind"], "a"), n(&public["wind"], "v"));
            m.world.game.rain = optional(&public["wx"], "rain").unwrap_or(0.);
            m.world.game.wet = optional(&public["wx"], "wet");
            for (slot, support) in public["support"].as_array().unwrap().iter().enumerate() {
                m.start.players[slot].support = support
                    .as_object()
                    .unwrap()
                    .iter()
                    .map(|(name, n)| (name.clone(), n.as_f64().unwrap()))
                    .collect();
            }
            let template = m.start.points[0].clone();
            m.start.points = public["points"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| {
                    let mut p = template.clone();
                    p.owner = v["owner"].as_i64().filter(|n| *n >= 0).map(|n| n as usize);
                    p.capper = v["capper"].as_i64().filter(|n| *n >= 0).map(|n| n as usize);
                    p.needs = v["needs"].as_i64().filter(|n| *n >= 0).map(|n| n as usize);
                    p.progress = n(v, "progress");
                    p.cut = v["cut"].as_bool().unwrap_or(false);
                    p.contested = v["contested"].as_bool().unwrap_or(false);
                    p.on_point = v["onPoint"].as_array().map_or_else(Vec::new, |ids| {
                        ids.iter().map(|id| id.as_u64().unwrap() as u32).collect()
                    });
                    p
                })
                .collect();
            m.world.game.smokes = public["smokes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|s| crate::game::Smoke {
                    id: s["id"].as_u64().unwrap() as u32,
                    x: n(s, "x"),
                    z: n(s, "z"),
                    r: n(s, "r"),
                    remaining: optional(s, "t"),
                })
                .collect();
            m.world.fires.clear();
            for fire in public["fires"].as_array().unwrap() {
                m.world.fires.insert(
                    fire[0].as_u64().unwrap() as usize,
                    fire[1].as_f64().unwrap(),
                );
            }
        }
        m.world.game.reveal = case["reveal"].as_bool().unwrap_or(false);
        let elapsed = case["inputs"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| optional(v, "prog"))
            .fold(0., math::max);
        if elapsed > 0. {
            let units = m
                .world
                .game
                .units
                .values()
                .map(|u| crate::production::UnitView {
                    id: u.id,
                    owner: Some(u.owner),
                    kind: m.rules.kinds[u.kind].name.as_str(),
                    hp: u.hp,
                    built: u.built.unwrap_or(0.),
                })
                .collect::<Vec<_>>();
            let context = crate::production::Context {
                mode: crate::economy_state::ModeContext {
                    kind: Mode::Conquest,
                    ..Default::default()
                },
                army: m.options.army,
                tech_enabled: m.options.tech,
                naval: m.world.game.naval,
                sudden_death: false,
                units: &units,
                drops: &[],
                world: None,
            };
            m.production
                .advance(&context, &m.start.economy, elapsed)
                .unwrap();
        }
        if case["finished"] == true {
            m.apply_conquest_victory().unwrap();
        }
        m
    }
    pub(super) fn equal(actual: &Value, expected: &Value, path: &str) {
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
    fn attachment_scene() -> Match {
        let fixture: Value =
            serde_json::from_str(include_str!("../tests/fixtures/recipient-selected.json"))
                .unwrap();
        initialize(&fixture["initial"])
    }
    fn attachment_observation_state(m: &Match) -> (String, Vec<String>, String) {
        (
            format!("{:?}", m.recipients),
            (0..m.world.game.players.len())
                .map(|slot| {
                    format!(
                        "{:?}",
                        m.world.pending(crate::world::Recipient::Player(slot))
                    )
                })
                .collect(),
            format!("{:?}", m.routing_fog),
        )
    }
    #[test]
    fn ordinary_attachment_rejects_malformed_mine_cells_without_publication() {
        let mut m = attachment_scene();
        m.preflight_ordinary_snapshot().unwrap();
        let cell = m.world.game.chars.len();
        m.world.mine_seen.insert(cell, 1);
        let before = attachment_observation_state(&m);
        assert!(m.ordinary_snapshot_raw_lists().is_ok());
        assert_eq!(m.preflight_ordinary_snapshot(), Err(SnapshotError::Rules));
        assert_eq!(attachment_observation_state(&m), before);
        assert_eq!(m.world.mine_seen.get(&cell), Some(&1));
        m.world.mine_seen.remove(&cell);
        m.preflight_ordinary_snapshot().unwrap();
        assert_eq!(attachment_observation_state(&m), before);
    }
    #[test]
    fn ordinary_attachment_rejects_late_retained_wreck_without_display_facts() {
        let mut m = attachment_scene();
        m.preflight_ordinary_snapshot().unwrap();
        let owner = m.world.game.players.len() - 1;
        m.world.game.wrecks.push(crate::game::Wreck {
            id: Some(88),
            cell: None,
            x: 800.,
            z: 400.,
            moving: false,
        });
        m.world.game.reveal = true;
        assert_eq!(m.observed_wrecks(owner).unwrap().len(), 1);
        m.world.game.reveal = false;
        m.world.game.wrecks.clear();
        assert_eq!(m.ordinary_snapshot_raw_lists().unwrap().wrecks, json!([]));
        assert!(m.prepare_ordinary_recipient_lists(&[0, 1]).is_ok());
        assert_eq!(
            m.recipient_routing(owner).unwrap().remembered_wreck_rows(),
            Err("missing observed wreck display facts: 88".into())
        );
        let before = attachment_observation_state(&m);
        assert_eq!(m.preflight_ordinary_snapshot(), Err(SnapshotError::Rules));
        assert_eq!(attachment_observation_state(&m), before);

        // Correct current facts replace the retained query during preparation,
        // but admission still leaves the canonical memory unchanged.
        m.world.game.wrecks.push(crate::game::Wreck {
            id: Some(88),
            cell: None,
            x: 800.,
            z: 400.,
            moving: false,
        });
        m.environment.wreck_metadata.insert(
            88,
            crate::environment::WreckMetadata {
                kind: m.rules.kind("tank").unwrap(),
                owner,
                rotation: 0.,
                motion: None,
            },
        );
        m.world.game.reveal = true;
        m.preflight_ordinary_snapshot().unwrap();
        assert_eq!(attachment_observation_state(&m), before);
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
                let mut expected = expected.clone();
                expected["flights"] = case["fullOutputs"][slot]["flights"].clone();
                equal(
                    &Value::Object(projection.fields),
                    &expected,
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
        m.world.game.units.get_mut(&1).unwrap().id = 2;
        assert!(matches!(SnapshotCache::new(&m), Err(SnapshotError::Rules)));
        m.world.game.units.get_mut(&1).unwrap().id = 1;
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
        m.world.game.units.get_mut(&1).unwrap().build = true;
        assert!(matches!(
            SnapshotCache::new(&m),
            Err(SnapshotError::Unsupported {
                unit: 1,
                capability: SnapshotCapability::Engineering
            })
        ));
        m.world.game.units.get_mut(&1).unwrap().build = false;
        m.world.game.units.get_mut(&1).unwrap().dig = true;
        assert!(matches!(
            SnapshotCache::new(&m),
            Err(SnapshotError::Unsupported {
                unit: 0,
                capability: SnapshotCapability::Engineering
            })
        ));
    }
}

#[cfg(test)]
#[path = "map_house_selected_tests.rs"]
mod map_house_selected_tests;

#[cfg(test)]
#[path = "recipient_snapshot/net52_tests.rs"]
mod net52_tests;

#[cfg(test)]
#[path = "economy_currency_tests.rs"]
mod economy_currency_tests;
#[cfg(test)]
#[path = "recipient_snapshot/production_state_tests.rs"]
mod production_state_tests;

#[cfg(test)]
#[path = "recipient_snapshot/combat_filter_tests.rs"]
mod combat_filter_tests;
