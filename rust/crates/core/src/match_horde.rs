//! Deterministic Horde setup and explicit late wave phase. Public tick admission
//! remains guarded until the other Horde mode owners are connected.
use crate::{
    catalog::{Catalog, Value},
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::{CELL, ModeKind, Point},
    map::AuthoritativeMap,
    match_runtime::{Match, MatchOptions, PlayerConfig, StartPlan, terrain_view},
    math,
    navigation::Versions,
    pathfinding::{PathRules, PathTerrain, Pathfinding},
    produced_units::Door,
    unit_publication::StagedUnit,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HordeProfile {
    Mixed,
    Infantry,
    Armor,
    Siege,
}
impl HordeProfile {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Mixed => "mixed",
            Self::Infantry => "infantry",
            Self::Armor => "armor",
            Self::Siege => "siege",
        }
    }
    fn types(self) -> &'static [&'static str] {
        match self {
            Self::Mixed => &[],
            Self::Infantry => &["rifle", "conscript", "mg", "flamer"],
            Self::Armor => &["tank", "medium", "tiger", "armoredcar", "tankdestroyer"],
            Self::Siege => &["mortar", "rocket"],
        }
    }
    fn share(self) -> f64 {
        match self {
            Self::Mixed => 0.0,
            Self::Infantry => 0.65,
            Self::Armor => 0.5,
            Self::Siege => 0.45,
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct HordePoolUnit {
    pub kind: String,
    pub from: u32,
    pub cost: f64,
    pub weight: f64,
}
#[derive(Clone, Debug, PartialEq)]
pub struct HordeState {
    pub slot: usize,
    pub defenders: usize,
    pub bunker: u32,
    pub gates: Vec<Point>,
    pub profile_seed: u32,
    pub profile_history: Vec<HordeProfile>,
    pub next_profile: HordeProfile,
    pub profile_wave: u32,
    pub profile: HordeProfile,
    pub wave: u32,
    pub active: bool,
    pub time_left: f64,
    pub reserve: Vec<String>,
    pub budget: f64,
    pub total_budget: f64,
    pub profile_spent: f64,
    pub purchase_seed: u32,
    pub left: f64,
    pub boss: Option<[f64; 2]>,
}
/// The caller detaches these public world effects at the same boundary as other
/// phase events. They do not define snapshot or wire fields.
#[derive(Clone, Debug, PartialEq)]
pub enum HordeEvent {
    BossSurfaced { at: Point },
}
fn cfg() -> Result<&'static Value, String> {
    Catalog::compiled()
        .map_err(|e| e.to_string())?
        .table("CFG")
        .map_err(|e| e.to_string())?
        .get("horde")
        .ok_or_else(|| "missing Horde rules".into())
}
fn number(value: &Value, name: &str) -> Result<f64, String> {
    value
        .get(name)
        .and_then(Value::number)
        .filter(|v| v.is_finite())
        .ok_or_else(|| format!("invalid Horde rule {name}"))
}
/// The prototype's independent mulberry32 stream. Profile draws never consume
/// purchase, combat, weather or commander randomness.
fn random(seed: &mut u32) -> f64 {
    *seed = seed.wrapping_add(0x6d2b_79f5);
    let mut t = *seed;
    t = (t ^ (t >> 15)).wrapping_mul(t | 1);
    t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 61));
    f64::from(t ^ (t >> 14)) / 4_294_967_296.0
}
pub fn horde_budget(wave: u32, defenders: usize, income: f64) -> Result<f64, String> {
    if wave == 0 || !(1..=5).contains(&defenders) || !income.is_finite() || income <= 0.0 {
        return Err("invalid Horde budget inputs".into());
    }
    let h = cfg()?;
    Ok((number(h, "budget")?
        * math::pow(number(h, "growth")?, f64::from(wave - 1))
        * defenders as f64
        * income)
        .min(9_007_199_254_740_991.0))
}
pub fn horde_pool(wave: u32, profile: HordeProfile) -> Result<Vec<HordePoolUnit>, String> {
    let h = cfg()?;
    let lean = (f64::from(wave) - number(h, "heavyWave")?).max(0.0) / 6.0;
    let unlock = h
        .get("unlock")
        .and_then(Value::array)
        .ok_or("missing Horde unlocks")?;
    let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
    let mut pool = Vec::new();
    for row in unlock {
        let row = row.array().ok_or("invalid Horde unlock row")?;
        let [kind, from, weight] = row else {
            return Err("invalid Horde unlock row".into());
        };
        let kind = kind.string().ok_or("invalid Horde unlock kind")?;
        let from = from.number().ok_or("invalid Horde unlock wave")? as u32;
        if wave < from {
            continue;
        }
        let cost = number(catalog.unit(kind).map_err(|e| e.to_string())?, "cost")?;
        pool.push(HordePoolUnit {
            kind: kind.into(),
            from,
            cost,
            weight: weight.number().ok_or("invalid Horde unlock weight")?
                * math::pow(cost / 200.0, lean)
                * if profile.types().contains(&kind) {
                    5.0
                } else {
                    1.0
                },
        });
    }
    Ok(pool)
}
pub fn horde_profile(
    wave: u32,
    budget: f64,
    seed: u32,
    history: &[HordeProfile],
    requested: Option<HordeProfile>,
) -> Result<HordeProfile, String> {
    let pool = horde_pool(wave, HordeProfile::Mixed)?;
    let eligible: Vec<_> = [
        HordeProfile::Mixed,
        HordeProfile::Infantry,
        HordeProfile::Armor,
        HordeProfile::Siege,
    ]
    .into_iter()
    .filter(|profile| {
        *profile == HordeProfile::Mixed
            || pool
                .iter()
                .any(|unit| unit.cost <= budget && profile.types().contains(&unit.kind.as_str()))
    })
    .collect();
    if let Some(requested) = requested {
        return Ok(if eligible.contains(&requested) {
            requested
        } else {
            HordeProfile::Mixed
        });
    }
    let recent = &history[history.len().saturating_sub(2)..];
    let can: Vec<_> = eligible
        .into_iter()
        .filter(|profile| !(recent.len() == 2 && recent.iter().all(|p| p == profile)))
        .collect();
    let mut seed = seed;
    let index = (random(&mut seed) * can.len() as f64).floor() as usize;
    Ok(can.get(index).copied().unwrap_or(HordeProfile::Mixed))
}
pub fn horde_stars(wave: u32) -> Result<u32, String> {
    let h = cfg()?;
    let first = number(h, "vetWave")? as u32;
    Ok(if wave < first {
        0
    } else {
        (1 + (wave - first) / number(h, "vetEvery")? as u32).min(3)
    })
}
fn pick<'a>(
    pool: &'a [HordePoolUnit],
    budget: f64,
    seed: &mut u32,
    defining: Option<HordeProfile>,
) -> Option<&'a HordePoolUnit> {
    let eligible = |unit: &&HordePoolUnit| {
        unit.cost <= budget && defining.is_none_or(|p| p.types().contains(&unit.kind.as_str()))
    };
    let total: f64 = pool.iter().filter(eligible).map(|u| u.weight).sum();
    let first = pool.iter().find(eligible)?;
    let mut r = random(seed) * total;
    pool.iter()
        .filter(eligible)
        .find(|u| {
            r -= u.weight;
            r < 0.0
        })
        .or(Some(first))
}
impl HordeState {
    fn prepare_profile(&mut self, income: f64) -> Result<(), String> {
        let wave = self.wave.checked_add(1).ok_or("Horde wave exhausted")?;
        self.next_profile = horde_profile(
            wave,
            horde_budget(wave, self.defenders, income)?,
            self.profile_seed.wrapping_add(wave),
            &self.profile_history,
            None,
        )?;
        self.profile_wave = wave;
        Ok(())
    }
    fn fill_reserve(&mut self, pool: &[HordePoolUnit]) {
        for _ in 0..32 {
            if self.budget <= 0.0 || self.reserve.len() >= 240 {
                break;
            }
            let defining = (self.profile_spent < self.total_budget * self.profile.share())
                .then_some(self.profile);
            let chosen = defining
                .and_then(|p| pick(pool, self.budget, &mut self.purchase_seed, Some(p)))
                .or_else(|| pick(pool, self.budget, &mut self.purchase_seed, None));
            let Some(unit) = chosen else {
                self.budget = 0.0;
                break;
            };
            if self.profile.types().contains(&unit.kind.as_str()) {
                self.profile_spent += unit.cost;
            }
            self.budget -= unit.cost;
            self.reserve.push(unit.kind.clone());
        }
    }
}
fn along_route(mut at: Point, route: &[Point], mut left: f64) -> Point {
    for &waypoint in route {
        let distance = math::hypot(waypoint.x - at.x, waypoint.z - at.z);
        if distance >= left {
            return Point {
                x: at.x + (waypoint.x - at.x) * left / distance,
                z: at.z + (waypoint.z - at.z) * left / distance,
            };
        }
        left -= distance;
        at = waypoint;
    }
    at
}
impl Match {
    /// Explicit core vision phase. This does not admit Horde to ordinary ticking or delivery.
    pub fn refresh_horde_visibility(&mut self) -> Result<(), String> {
        self.validate_horde_phase(0.0)?;
        let horde = self.horde.as_ref().unwrap();
        let defender_team = self.world.game.players[0].team;
        let few = horde.budget == 0.0 && horde.left <= number(cfg()?, "reveal")?;
        let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
        let mut revealed = Vec::new();
        if horde.active {
            for unit in self.world.game.units.values() {
                if unit.owner == horde.slot
                    && unit.air.is_none()
                    && (few
                        || catalog
                            .unit(&self.rules.kinds[unit.kind].name)
                            .map_err(|e| e.to_string())?
                            .get("boss")
                            .and_then(Value::boolean)
                            .unwrap_or(false))
                {
                    revealed.push(unit.id);
                }
            }
        }
        crate::vision::update_vision(&mut self.world.game, &self.rules, &self.grid);
        for player in &mut self.world.game.players {
            if player.team == defender_team {
                player.visible.extend_from_slice(&revealed);
                player.visible.sort_unstable();
                player.visible.dedup();
            }
        }
        Ok(())
    }
    /// Explicit constructor for the Horde-owned wave phase. Generic `Match::new`
    /// and ordinary `advance_tick` retain their existing admission guards.
    pub fn new_horde(map: &AuthoritativeMap, mut options: MatchOptions) -> Result<Self, String> {
        if options.mode != Mode::Horde || !(1..=5).contains(&options.players.len()) {
            return Err("Horde needs Horde mode and one through five defenders".into());
        }
        let defend = map
            .defend()
            .filter(|d| !d.is_empty())
            .ok_or("Horde needs defender spawns")?;
        if defend.len()
            >= map.source()["spawns"]
                .as_array()
                .ok_or("missing Horde spawns")?
                .len()
        {
            return Err("Horde needs an attacker spawn".into());
        }
        if map.source().get("scenario").is_some() {
            return Err("Horde startup does not yet apply authored scenarios".into());
        }
        if options.army == ArmySize::Endless {
            return Err("Horde does not support Endless armies".into());
        }
        let defenders = options.players.len();
        let faction = [Faction::Germany, Faction::Ussr, Faction::Usa]
            .into_iter()
            .find(|f| !options.players.iter().any(|p| p.faction == *f))
            .unwrap_or(Faction::Germany);
        for p in &mut options.players {
            p.team = 0;
        }
        options.players.push(PlayerConfig {
            name: "Horde".into(),
            team: 1,
            faction,
        });
        options.supply = false;
        options.logistics = false;
        let rules = crate::rules::Rules::compiled().map_err(|e| e.to_string())?;
        let start = StartPlan::prepare_horde(map, &options, &rules)?;
        let mut result = Self::skirmish_shell(map, options, rules, start)?;
        result.world.game.mode = Some(ModeKind::Horde);
        result.fortify_assault(0)?;
        let bunker = result
            .world
            .game
            .units
            .values()
            .find(|u| result.rules.kinds[u.kind].name == "bunker")
            .ok_or("missing Horde bunker")?;
        let (bunker_id, target) = (bunker.id, bunker.at());
        let mut gates = Vec::new();
        for (index, spawn) in map.source()["spawns"]
            .as_array()
            .ok_or("missing Horde spawns")?
            .iter()
            .enumerate()
        {
            if defend.contains(&index) {
                continue;
            }
            let at = Point {
                x: (spawn["x"].as_f64().ok_or("invalid Horde spawn")? + 0.5) * CELL,
                z: (spawn["y"].as_f64().ok_or("invalid Horde spawn")? + 0.5) * CELL,
            };
            let route = result.horde_path("medium", at, target)?;
            gates.push(along_route(at, &route, 16.0));
        }
        let mut state = HordeState {
            slot: defenders,
            defenders,
            bunker: bunker_id,
            gates,
            profile_seed: result.start.environment_seed,
            profile_history: Vec::new(),
            next_profile: HordeProfile::Mixed,
            profile_wave: 1,
            profile: HordeProfile::Mixed,
            wave: 0,
            active: false,
            time_left: number(cfg()?, "break")?,
            reserve: Vec::new(),
            budget: 0.0,
            total_budget: 0.0,
            profile_spent: 0.0,
            purchase_seed: 0,
            left: 0.0,
            boss: None,
        };
        state.prepare_profile(1.0)?;
        result.horde = Some(state);
        result.complete_skirmish_start()?;
        Ok(result)
    }
    fn horde_income(&self) -> Result<f64, String> {
        let key = match self.options.army {
            ArmySize::Standard => "standard",
            ArmySize::Large => "large",
            ArmySize::Massive => "massive",
            ArmySize::Endless => return Err("Horde Endless army".into()),
        };
        let cfg = Catalog::compiled()
            .map_err(|e| e.to_string())?
            .table("CFG")
            .map_err(|e| e.to_string())?;
        number(
            cfg.get("armies")
                .and_then(|v| v.get(key))
                .ok_or("missing army rules")?,
            "income",
        )
    }
    fn horde_path(&self, name: &str, from: Point, target: Point) -> Result<Vec<Point>, String> {
        let game = &self.world.game;
        let n = game.w * game.h;
        let near = vec![0; n];
        let hazards = vec![false; n];
        let mut paths = Pathfinding::default();
        let path = paths
            .find_path(
                PathTerrain {
                    navigation: terrain_view(game),
                    near_walls: &near,
                    known_mines: &hazards,
                    fires: &hazards,
                    bunkers: &[],
                    wear: game.wear.as_deref(),
                    wetness: game.wet.unwrap_or(0.0),
                    roads: game.ground.as_ref().unwrap_or(&game.chars).contains(&b'D'),
                },
                PathRules::compiled().map_err(|e| e.to_string())?,
                &self.rules.kinds[self.rules.kind(name).ok_or("unknown Horde kind")?],
                from,
                target,
                None,
            )
            .map_err(|e| format!("Horde path: {e:?}"))?;
        Ok(path.waypoints)
    }
    fn spawn_horde_ground(
        &mut self,
        slot: usize,
        name: &str,
        gate: Point,
        target: Point,
        stars: u32,
        radius: f64,
    ) -> Result<u32, String> {
        let kind = self.rules.kind(name).ok_or("unknown Horde unit")?;
        let id = self.next_unit_id;
        let heading = math::atan2(target.z - gate.z, target.x - gate.x);
        let mut view = terrain_view(&self.world.game);
        view.versions = Versions {
            infantry: self.world.versions.infantry,
            vehicle: self.world.versions.vehicle,
            naval: self.world.versions.naval,
            obstruction: self.world.versions.obstruction,
            knowledge: 0,
        };
        let placement = self
            .produced_placer
            .step_out(
                view,
                &self.rules,
                kind,
                id,
                &self.world.game.units,
                Door {
                    at: gate,
                    radius,
                    heading,
                    origin: Some(gate),
                },
                self.world.game.tick as u64,
            )
            .map_err(|e| format!("Horde gate entry: {e:?}"))?;
        let mut draft = StagedUnit::ordinary(
            &self.world.game,
            &self.rules,
            slot,
            kind,
            id,
            self.world.game.units.len() as u32,
            self.start.players[slot].spawn,
            Some(placement.at),
        )
        .map_err(|e| format!("Horde unit: {e:?}"))?;
        draft.unit.rot = placement.heading;
        draft.unit.ghost = placement.ghost_until;
        draft.unit.amove = Some(target);
        draft.unit.path = self.horde_path(name, placement.at, target)?;
        draft.control.aim = placement.aim;
        draft.control.auto_retreat = false;
        self.publish_starting_unit(draft)?;
        if stars > 0 {
            let cfg = Catalog::compiled()
                .map_err(|e| e.to_string())?
                .table("CFG")
                .map_err(|e| e.to_string())?;
            let threshold = cfg
                .get("vetXp")
                .and_then(Value::array)
                .and_then(|v| v.get(stars as usize - 1))
                .and_then(Value::number)
                .ok_or("invalid Horde veterancy")?;
            self.combat
                .get_mut(id)
                .ok_or("missing Horde combat metadata")?
                .xp = threshold
                * number(
                    Catalog::compiled()
                        .map_err(|e| e.to_string())?
                        .unit(name)
                        .map_err(|e| e.to_string())?,
                    "cost",
                )?;
        }
        Ok(id)
    }
    /// Run after ordinary capture/income and death removal. It does not advance
    /// the clock or run combat; newly created units act on the following tick.
    pub fn step_horde(&mut self, dt: f64) -> Result<Vec<HordeEvent>, String> {
        self.validate_horde_phase(dt)?;
        if self.finish.is_some() {
            return Ok(Vec::new());
        }
        let state = self.horde.as_ref().ok_or("missing Horde state")?;
        let bunker = self.world.game.units.get(&state.bunker);
        if bunker.is_none_or(|u| u.hp <= 0.0) {
            let finish = self.prepare_match_finish(
                1,
                crate::match_end::EndReason::Structures,
                self.fallen.bunker,
            )?;
            self.publish_match_finish(finish);
            return Ok(Vec::new());
        }
        let target = bunker.expect("living bunker").at();
        let income = self.horde_income()?;
        let mut state = self.horde.take().expect("validated Horde state");
        let result = self.step_horde_state(&mut state, dt, income, target);
        self.horde = Some(state);
        result
    }
    fn step_horde_state(
        &mut self,
        m: &mut HordeState,
        dt: f64,
        income: f64,
        target: Point,
    ) -> Result<Vec<HordeEvent>, String> {
        let mut events = Vec::new();
        if !m.active {
            m.time_left -= dt;
            if m.time_left > 0.0 {
                return Ok(events);
            }
            if m.profile_wave != m.wave + 1 {
                m.prepare_profile(income)?;
            }
            m.wave += 1;
            m.active = true;
            m.time_left = 0.0;
            m.profile = m.next_profile;
            m.profile_history.push(m.profile);
            if m.profile_history.len() > 2 {
                m.profile_history.remove(0);
            }
            m.purchase_seed = m.profile_seed.wrapping_add(m.wave);
            m.profile_spent = 0.0;
            m.reserve.clear();
            m.budget = horde_budget(m.wave, m.defenders, income)?;
            m.total_budget = m.budget;
            self.start.economy.players[m.slot].mp = if m.wave >= 6 {
                100.0 + 150.0 * f64::from(m.wave - 5) * m.defenders as f64 * income
            } else {
                0.0
            };
            if m.wave.is_multiple_of(10) {
                for i in 0..m.wave / 10 {
                    let gate = m.gates[i as usize % m.gates.len()];
                    self.spawn_horde_ground(m.slot, "kaiju", gate, target, 0, 0.0)?;
                    events.push(HordeEvent::BossSurfaced { at: gate });
                }
            }
            if m.wave >= 10 {
                let count = ((1 + (m.wave - 10) / 3) as usize * m.defenders.div_ceil(2)).min(8);
                for i in 0..count {
                    self.spawn_aircraft(m.slot, if i % 3 == 2 { "fighter" } else { "attacker" })
                        .map_err(|e| format!("Horde aircraft: {e:?}"))?;
                }
            }
        }
        let pool = horde_pool(m.wave, m.profile)?;
        m.fill_reserve(&pool);
        let mut field = 0;
        let (mut boss_hp, mut boss_max) = (0.0, 0.0);
        for u in self
            .world
            .game
            .units
            .values()
            .filter(|u| u.owner == m.slot && u.air.is_none() && u.hp > 0.0)
        {
            field += 1;
            if self.rules.kinds[u.kind].name == "kaiju" {
                boss_hp += u.hp;
                boss_max += 6000.0;
            }
        }
        m.boss = (boss_max > 0.0).then_some([boss_hp.ceil(), boss_max]);
        if self.world.game.tick % 10 == 0 {
            for &gate in &m.gates {
                if m.reserve.is_empty() || field >= (60 * m.defenders).min(240) {
                    break;
                }
                let name = m.reserve.pop().expect("nonempty reserve");
                self.spawn_horde_ground(m.slot, &name, gate, target, horde_stars(m.wave)?, 3.0)?;
                field += 1;
            }
        }
        let cheapest = pool.iter().map(|u| u.cost).fold(f64::INFINITY, f64::min);
        m.left = field as f64 + m.reserve.len() as f64 + (m.budget / cheapest).ceil();
        if m.left == 0.0 {
            m.active = false;
            m.time_left = number(cfg()?, "break")?;
            m.prepare_profile(income)?;
            let bunker = self
                .world
                .game
                .units
                .get_mut(&m.bunker)
                .ok_or("missing Horde bunker")?;
            bunker.hp = (bunker.hp + 300.0).min(3000.0);
            let ids: Vec<_> = self
                .world
                .game
                .units
                .values()
                .filter(|u| u.owner == m.slot)
                .map(|u| u.id)
                .collect();
            for id in ids {
                self.world.game.units.remove(&id);
                self.grid.remove_unit(id);
                self.aviation.remove(id);
                self.abilities.remove(id);
                self.recovery.remove(id);
                self.logistics.remove(id);
                self.recruit_metadata.remove(&id);
                self.commands.requests.retire_unit(id);
                self.commands.units.remove(&id);
                // Combat and firing metadata can still belong to in-flight shots.
            }
        }
        Ok(events)
    }
    fn validate_horde_phase(&self, dt: f64) -> Result<(), String> {
        let m = self.horde.as_ref().ok_or("missing Horde state")?;
        if self.options.mode != Mode::Horde
            || self.world.game.mode != Some(ModeKind::Horde)
            || !dt.is_finite()
            || dt < 0.0
            || !(1..=5).contains(&m.defenders)
            || m.slot != m.defenders
            || self.start.players.len() != m.slot + 1
            || self.start.economy.players.len() != m.slot + 1
            || self.world.game.players.len() != m.slot + 1
            || self.options.players.len() != m.slot + 1
            || self.story.len() != m.slot + 1
            || self.world.game.tick < 0
            || m.wave == u32::MAX
            || m.gates.is_empty()
            || m.gates.iter().any(|p| !p.x.is_finite() || !p.z.is_finite())
            || ![
                m.time_left,
                m.budget,
                m.total_budget,
                m.profile_spent,
                m.left,
            ]
            .iter()
            .all(|v| v.is_finite())
            || m.budget < 0.0
            || m.total_budget < 0.0
            || m.profile_spent < 0.0
            || m.reserve.len() > 240
            || (m.active && m.wave == 0)
            || m.profile_history.len() > 2
        {
            return Err("invalid Horde phase state".into());
        }
        for (slot, p) in self.start.players.iter().enumerate() {
            let team = i32::from(slot == m.slot);
            if p.config.team != team
                || self.options.players[slot].team != team
                || self.start.economy.players[slot].team != team
                || self.world.game.players[slot].team != team
            {
                return Err("invalid Horde team identity".into());
            }
        }
        for name in &m.reserve {
            let kind = self.rules.kind(name).ok_or("invalid Horde reserve kind")?;
            if self.rules.kinds[kind].air || self.rules.kinds[kind].structure {
                return Err("invalid Horde reserve kind".into());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "match_horde_tests.rs"]
mod tests;
