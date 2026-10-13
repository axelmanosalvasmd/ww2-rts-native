//! Explicit authority-to-observation boundary for the opening Conquest policy.
//! The policy receives owned recipient facts and never borrows Game or routing state.
use std::collections::BTreeSet;

use crate::{
    catalog::Catalog,
    commander_ai::{
        self, Input, Level, Observation, PlayerObservation, PointObservation, SourceState,
        UnitObservation,
    },
    economy::Mode,
    game::{CELL, Point, Unit},
    match_runtime::Match,
    math,
    orders::OrderKey,
    vision,
};

fn rounded(n: f64) -> f64 {
    math::round(n * 10.0) / 10.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        economy::Faction,
        economy_state::ArmySize,
        map::AuthoritativeMap,
        match_commands::{Command, MoveOrder},
        match_runtime::{MatchOptions, PlayerConfig},
        orders::OrderQueue,
        random::StreamId,
        weather::WeatherChoice,
    };
    use serde_json::{Value, json};

    fn instance(seed: u64) -> Match {
        let map = AuthoritativeMap::from_json(
            &serde_json::to_vec(&json!({
                "name":"Opening integration", "w":64,"h":64,
                "rows":vec![".".repeat(64);64],
                "spawns":[{"x":4,"y":4},{"x":59,"y":59}],
                "points":[{"x":27,"y":10},{"x":13,"y":27},{"x":32,"y":32}]
            }))
            .unwrap(),
        )
        .unwrap();
        Match::new(
            &map,
            MatchOptions {
                seed,
                mode: Mode::Conquest,
                players: vec![
                    PlayerConfig {
                        name: "A".into(),
                        team: 0,
                        faction: Faction::try_from(0).unwrap(),
                    },
                    PlayerConfig {
                        name: "B".into(),
                        team: 1,
                        faction: Faction::try_from(1).unwrap(),
                    },
                ],
                shuffle: false,
                army: ArmySize::Standard,
                weather: WeatherChoice::Map,
                weather_seed: None,
                living_weather: false,
                map_key: None,
                tech: false,
                supply: false,
                logistics: false,
                sandbox: false,
            },
        )
        .unwrap()
    }
    fn advance(m: &mut Match) -> Vec<crate::match_commands::CommandResult> {
        let next = m.world.game.tick as u64 + 1;
        let results = m.apply_commands(next, false).unwrap();
        m.world.game.tick += 1;
        results
    }
    fn source_projection(v: &Value) -> Value {
        let units:Vec<_>=v["units"].as_array().unwrap().iter().map(|u|json!({
            "id":u["id"],"kind":u["type"],"owner":u["owner"],"x":u["x"],"z":u["z"],
            "hp":u["hp"],"supp":u["supp"],"target":u["targetId"],"cover":u["cover"],"path":u["path"],
            "attack":u["attackId"].as_u64().unwrap()!=0,"retreat":u["retreating"],
            "riding":u["flags"].as_u64().unwrap() & 262144 !=0,"build":u["build"].as_u64().unwrap()!=0,
            "dig":!u["dig"].is_null(),"enter":u["enter"].as_i64().unwrap()>=0,
            "fire":u["fireAt"].as_i64().unwrap()>=0,"nade":!u["nade"].is_null()
        })).collect();
        let players: Vec<_> = v["players"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| json!([p["team"], p["out"], p["spawn"]]))
            .collect();
        let points: Vec<_> = v["points"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| {
                json!([
                    p["x"],
                    p["z"],
                    p["vp"],
                    p["mp"],
                    p["owner"],
                    p["cut"],
                    p["locked"]
                ])
            })
            .collect();
        let flags: Vec<_> = v["flags"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|r| {
                std::iter::repeat_n(r[0].as_u64().unwrap(), r[1].as_u64().unwrap() as usize)
            })
            .collect();
        json!({"tick":v["tick"],"players":players,"points":points,"units":units,"flags":flags})
    }
    fn native_projection(v: &Input) -> Value {
        let units:Vec<_>=v.units.iter().map(|u|json!({
            "id":u.id,"kind":u.kind,"owner":u.owner,"x":u.at.x,"z":u.at.z,"hp":u.hp,"supp":u.supp,
            "target":u.target_id,"cover":u.cover,"path":u.path.iter().map(|p|json!({"x":p.x,"z":p.z})).collect::<Vec<_>>(),
            "attack":u.attack_id,"retreat":u.retreating,"riding":u.riding,"build":u.build,"dig":u.dig,
            "enter":u.enter,"fire":u.fire_at,"nade":u.nade
        })).collect();
        json!({"tick":v.tick,
            "players":v.players.iter().map(|p|json!([p.team,p.out,{"x":p.spawn.x,"z":p.spawn.z}])).collect::<Vec<_>>(),
            "points":v.points.iter().map(|p|json!([p.at.x,p.at.z,p.vp,p.mp,p.owner.map_or(-1,|o|o as i64),p.cut,p.locked])).collect::<Vec<_>>(),
            "units":units,"flags":v.flags})
    }
    fn equal_numbers(a: &Value, b: &Value) -> bool {
        match (a, b) {
            (Value::Number(a), Value::Number(b)) => {
                a.as_f64().unwrap().to_bits() == b.as_f64().unwrap().to_bits()
            }
            (Value::Array(a), Value::Array(b)) => {
                a.len() == b.len() && a.iter().zip(b).all(|(a, b)| equal_numbers(a, b))
            }
            (Value::Object(a), Value::Object(b)) => {
                a.len() == b.len()
                    && a.iter()
                        .all(|(k, a)| b.get(k).is_some_and(|b| equal_numbers(a, b)))
            }
            _ => a == b,
        }
    }
    #[test]
    fn actual_source_observations_and_vision_phase_match() {
        let f: Value =
            serde_json::from_str(include_str!("../tests/fixtures/commander-ai.json")).unwrap();
        assert_eq!(f["producer"].as_array().unwrap().len(), 2);
        for c in f["producer"].as_array().unwrap() {
            let map = AuthoritativeMap::from_json(&serde_json::to_vec(&c["map"]).unwrap()).unwrap();
            let mut m = Match::new(&map, instance(913).options).unwrap();
            m.world.game.tick = c["tick"].as_i64().unwrap();
            m.world.game.players[0].visible = c["visible"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_u64().unwrap() as u32)
                .collect();
            for o in c["overrides"].as_array().unwrap() {
                let id = o["id"].as_u64().unwrap() as u32;
                let u = m.world.game.units.get_mut(&id).unwrap();
                for (k, v) in o.as_object().unwrap() {
                    let at = |v: &Value| Point {
                        x: v["x"].as_f64().unwrap(),
                        z: v["z"].as_f64().unwrap(),
                    };
                    match k.as_str() {
                        "id" => {}
                        "x" => u.x = v.as_f64().unwrap(),
                        "z" => u.z = v.as_f64().unwrap(),
                        "hp" => u.hp = v.as_f64().unwrap(),
                        "supp" => u.supp = v.as_f64().unwrap(),
                        "worldGoal" => u.world_goal = Some(at(v)),
                        "path" => u.path = v.as_array().unwrap().iter().map(at).collect(),
                        "targetId" => {
                            m.firing.units.get_mut(&id).unwrap().target_id =
                                v.as_u64().unwrap() as u32
                        }
                        "fireAt" => {
                            m.commands.units.get_mut(&id).unwrap().fire_at =
                                Some(v.as_u64().unwrap() as usize)
                        }
                        "enter" => {
                            m.commands.units.get_mut(&id).unwrap().enter =
                                Some(v.as_u64().unwrap() as usize)
                        }
                        "nade" => m.commands.units.get_mut(&id).unwrap().nade = Some(at(v)),
                        _ => panic!("unhandled source input {k}"),
                    }
                }
            }
            let v = m.opening_commander_observation(0).unwrap();
            let actual = native_projection(v.input());
            let expected = source_projection(&c["expected"]["frame"]);
            assert!(
                equal_numbers(&actual, &expected),
                "{}: actual {actual}, expected {expected}",
                c["name"]
            );
        }
        let mut m = instance(917);
        let mut seat = OpeningCommander::new(0, Level::Normal);
        for row in f["visionSchedule"].as_array().unwrap() {
            assert_eq!(m.world.game.tick, row[0].as_i64().unwrap());
            m.tick_opening_commander(&mut seat).unwrap();
            assert_eq!(m.world.game.vision_tick, row[1].as_i64());
            advance(&mut m);
        }
    }
    #[test]
    fn opening_orders_use_real_admission_and_the_delivered_view() {
        let mut a = instance(803);
        let mut b = instance(803);
        let mut sa = OpeningCommander::new(0, Level::Normal);
        let mut sb = sa.clone();
        assert_eq!(
            a.tick_opening_commander(&mut sa).unwrap(),
            b.tick_opening_commander(&mut sb).unwrap()
        );
        let queue = a.commands.queue.snapshot();
        assert_eq!(
            a.world.game.vision_tick, None,
            "ordinary startup has no vision pass"
        );
        assert!(!queue.pending.is_empty());
        assert_eq!(queue, b.commands.queue.snapshot());
        let ids: Vec<_> = queue
            .pending
            .iter()
            .flat_map(|r| match &r.value {
                Command::Move { orders, .. } => {
                    orders.iter().map(|o| o.unit as u32).collect::<Vec<_>>()
                }
                _ => panic!("opening emits moves"),
            })
            .collect();
        assert_eq!(a.start.random.snapshot(), b.start.random.snapshot());
        assert!(
            a.start
                .random
                .snapshot()
                .streams
                .iter()
                .any(|s| s.0 == StreamId::Ai(0))
        );
        let results = advance(&mut a);
        assert!(results.iter().all(|r| r.denial.is_none()));
        assert_eq!(results, advance(&mut b));
        assert!(
            ids.iter()
                .all(|id| a.world.game.units[id].world_goal.is_some())
        );
        assert!(ids.iter().any(|id| !a.world.game.units[id].path.is_empty()));
        assert_eq!(a.start.random.snapshot(), b.start.random.snapshot());

        // Seat one's hard cadence acts at odd tick seven from the tick-six delivery.
        let mut m = instance(807);
        let mut seat = OpeningCommander::new(1, Level::Hard);
        for tick in 0..7 {
            assert_eq!(m.world.game.tick, tick);
            assert_eq!(
                m.tick_opening_commander(&mut seat).unwrap(),
                OpeningTick::Waiting
            );
            assert_eq!(
                m.world.game.vision_tick,
                (tick > 0).then(|| tick - (tick - 1) % 4)
            );
            advance(&mut m);
        }
        let delivered = seat.delivered().unwrap().input().clone();
        assert_eq!(delivered.tick, 6);
        assert_eq!(m.world.game.vision_tick, Some(5));
        let own = m
            .world
            .game
            .units
            .values()
            .find(|u| u.owner == 1 && m.rules.kinds[u.kind].infantry)
            .unwrap()
            .id;
        m.world
            .game
            .units
            .get_mut(&own)
            .unwrap()
            .path
            .push(Point { x: 110.0, z: 110.0 });
        assert!(matches!(
            m.tick_opening_commander(&mut seat).unwrap(),
            OpeningTick::Queued(_)
        ));
        assert_eq!(seat.delivered().unwrap().input(), &delivered);
        assert!(advance(&mut m).iter().all(|r| r.denial.is_none()));
    }
    #[test]
    fn hidden_facts_and_opponent_plans_cannot_change_the_opening() {
        let mut a = instance(901);
        let mut b = instance(901);
        let enemies: Vec<_> = b
            .world
            .game
            .units
            .values()
            .filter(|u| u.owner == 1)
            .map(|u| u.id)
            .collect();
        for id in &enemies {
            let u = b.world.game.units.get_mut(id).unwrap();
            u.hp = (u.hp / 2.0).max(1.0);
            u.x -= 0.25;
            u.z -= 0.25;
            u.path = vec![Point { x: 120.0, z: 120.0 }];
            let c = b.commands.units.get_mut(id).unwrap();
            c.attack_id = 1;
            c.fire_at = Some(64 * 60 + 60);
            c.nade = Some(Point { x: 119.0, z: 119.0 });
        }
        b.start.economy.players[1].mp = 9999.0;
        // A logged edit beyond all friendly eyes remains pending and cannot become cover.
        b.world.set_cell(60 * 64 + 40, b'O').unwrap();
        let va = a.opening_commander_observation(0).unwrap();
        let vb = b.opening_commander_observation(0).unwrap();
        assert_eq!(va.input(), vb.input());
        assert!(va.input().units.iter().all(|u| u.owner == 0));
        let mut sa = OpeningCommander::new(0, Level::Hard);
        let mut sb = sa.clone();
        assert_eq!(
            a.tick_opening_commander(&mut sa).unwrap(),
            b.tick_opening_commander(&mut sb).unwrap()
        );
        assert_eq!(a.commands.queue.snapshot(), b.commands.queue.snapshot());
        assert_eq!(a.start.random.snapshot(), b.start.random.snapshot());
        assert_eq!(sa, sb);
    }
    #[test]
    fn projection_and_queue_failures_preserve_policy_random_and_orders() {
        let mut m = instance(905);
        m.start.points[0].needs = Some(usize::MAX);
        let mut seat = OpeningCommander::new(0, Level::Hard);
        let prior = seat.clone();
        let random = m.start.random.snapshot();
        let queue = m.commands.queue.snapshot();
        assert!(m.tick_opening_commander(&mut seat).is_err());
        assert_eq!(seat, prior);
        assert_eq!(m.start.random.snapshot(), random);
        assert_eq!(m.commands.queue.snapshot(), queue);
        assert_eq!(m.world.game.vision_tick, None);
        m.start.points[0].needs = None;
        let hidden = m
            .world
            .game
            .units
            .values()
            .find(|u| u.owner == 1)
            .unwrap()
            .id;
        let kind = m.world.game.units[&hidden].kind;
        m.world.game.units.get_mut(&hidden).unwrap().kind = usize::MAX;
        assert!(m.tick_opening_commander(&mut seat).is_err());
        assert_eq!(m.world.game.vision_tick, None);
        m.world.game.units.get_mut(&hidden).unwrap().kind = kind;
        let hp = m.world.game.units[&hidden].hp;
        m.world.game.units.get_mut(&hidden).unwrap().hp = f64::NAN;
        assert!(m.tick_opening_commander(&mut seat).is_err());
        assert_eq!(m.world.game.vision_tick, None);
        assert_eq!(seat, prior);
        assert_eq!(m.start.random.snapshot(), random);
        assert_eq!(m.commands.queue.snapshot(), queue);
        m.world.game.units.get_mut(&hidden).unwrap().hp = hp;
        let own = m
            .world
            .game
            .units
            .values()
            .find(|u| u.owner == 0 && m.rules.kinds[u.kind].infantry)
            .unwrap()
            .id;
        m.world.game.units.get_mut(&own).unwrap().entrench = true;
        m.commands.units.get_mut(&own).unwrap().enter = Some(0);
        assert!(m.tick_opening_commander(&mut seat).is_err());
        assert_eq!(seat, prior);
        assert_eq!(m.world.game.vision_tick, None);
        m.world.game.units.get_mut(&own).unwrap().entrench = false;
        m.commands.units.get_mut(&own).unwrap().enter = None;
        let key = OrderKey {
            tick: 1,
            issuer: 0,
            sequence: 0,
        };
        m.enqueue_command(
            key,
            Command::Move {
                orders: vec![MoveOrder {
                    unit: -1,
                    at: Point { x: 10.0, z: 10.0 },
                }],
                control_ids: None,
                attack_move: false,
                queue: false,
                together: false,
                face: None,
            },
        )
        .unwrap();
        let queue = m.commands.queue.snapshot();
        assert!(m.tick_opening_commander(&mut seat).is_err());
        assert_eq!(seat, prior);
        assert_eq!(m.start.random.snapshot(), random);
        assert_eq!(m.commands.queue.snapshot(), queue);
        let mut empty = OrderQueue::new(0);
        empty.advance().unwrap();
        m.commands.queue = empty;
        assert!(m.tick_opening_commander(&mut seat).is_err());
        assert_eq!(seat, prior);
        assert_eq!(m.start.random.snapshot(), random);
        assert!(m.commands.queue.is_empty());
        let mut empty = OrderQueue::new(4096);
        empty.advance().unwrap();
        m.commands.queue = empty;
        seat.sequence = u64::MAX;
        let prior = seat.clone();
        assert!(m.tick_opening_commander(&mut seat).is_err());
        assert_eq!(seat, prior);
        assert_eq!(m.start.random.snapshot(), random);
        assert!(m.commands.queue.is_empty());
        m.world.game.tick = i64::MAX;
        let mut snapshot = m.commands.queue.snapshot();
        snapshot.next_tick = Some(i64::MAX as u64 + 1);
        m.commands.queue = OrderQueue::restore(snapshot).unwrap();
        let queue = m.commands.queue.snapshot();
        let prior = seat.clone();
        assert!(
            m.tick_opening_commander(&mut seat)
                .unwrap_err()
                .contains("tick exhausted")
        );
        assert_eq!(seat, prior);
        assert_eq!(m.commands.queue.snapshot(), queue);
        assert_eq!(m.start.random.snapshot(), random);
    }
}
fn point(p: Point) -> Point {
    Point {
        x: rounded(p.x),
        z: rounded(p.z),
    }
}

/// External per-seat state, retained by the eventual fixed Match AI phase.
/// This is not a persisted snapshot or a complete strategic commander.
#[derive(Clone, Debug, PartialEq)]
pub struct OpeningCommander {
    owner: usize,
    level: Level,
    memory: SourceState,
    delivered: Option<Observation>,
    last_tick: Option<i64>,
    sequence: u64,
}
impl OpeningCommander {
    pub fn new(owner: usize, level: Level) -> Self {
        Self {
            owner,
            level,
            memory: SourceState::default(),
            delivered: None,
            last_tick: None,
            sequence: 0,
        }
    }
    pub fn owner(&self) -> usize {
        self.owner
    }
    pub fn memory(&self) -> &SourceState {
        &self.memory
    }
    pub fn delivered(&self) -> Option<&Observation> {
        self.delivered.as_ref()
    }
}
#[derive(Clone, Debug, PartialEq)]
pub enum OpeningTick {
    Waiting,
    Stopped,
    Deferred(commander_ai::PolicyError),
    Queued(Vec<OrderKey>),
}

impl Match {
    fn validate_opening_projection(&self, owner: usize) -> Result<(), String> {
        if !matches!(self.options.mode, Mode::Default | Mode::Conquest)
            || self.world.geography().is_some()
            || self.options.logistics
        {
            return Err("opening commander requires ordinary Conquest".into());
        }
        self.recipient_routing(owner)?; // validates recipient/context before refresh writes
        let g = &self.world.game;
        if g.tick < 0
            || !(1..=6).contains(&g.players.len())
            || !(20..=1024).contains(&g.w)
            || !(20..=1024).contains(&g.h)
            || g.units.len() > 65_536
            || self.start.points.len() > 1024
            || self.start.players.len() != g.players.len()
            || self.start.economy.players.len() != g.players.len()
        {
            return Err("invalid commander player state or clock".into());
        }
        for u in g.units.values().filter(|u| u.owner == owner) {
            if !self.commands.units.contains_key(&u.id) || !self.firing.units.contains_key(&u.id) {
                return Err("missing canonical own-unit intent".into());
            }
            let c = &self.commands.units[&u.id];
            if u.entrench
                && !u.dig
                && !u.build
                && c.enter.is_some()
                && !u.retreating
                && !g.units.contains_key(&u.board)
                && !(c.attack_id != 0
                    && g.units.contains_key(&c.attack_id)
                    && g.players[owner].sees(c.attack_id))
                && c.fire_at.is_none()
                && c.nade.is_none()
            {
                return Err("commander needs source entrench job state for an enter plan".into());
            }
        }
        let within = |p: Point| {
            p.x.is_finite()
                && p.z.is_finite()
                && p.x >= 0.0
                && p.z >= 0.0
                && p.x <= g.w as f64 * CELL
                && p.z <= g.h as f64 * CELL
        };
        if self.start.players.iter().any(|p| !within(p.spawn))
            || self.start.points.iter().any(|p| {
                !within(p.at)
                    || !p.vp.is_finite()
                    || !p.mp.is_finite()
                    || p.owner.is_some_and(|o| o >= g.players.len())
                    || p.needs.is_some_and(|i| i >= self.start.points.len())
            })
            || g.units.values().any(|u| {
                u.id == 0
                    || !within(u.at())
                    || u.hp < 0.0
                    || !u.supp.is_finite()
                    || u.supp < 0.0
                    || (u.owner == owner
                        && (u.path.len() > 4096 || u.path.iter().any(|p| !within(*p))))
            })
        {
            return Err("invalid commander projection inputs".into());
        }
        Ok(())
    }
    /// Snapshot only the fields consumed by this bounded policy, at source public precision.
    /// Job destination, economy, aircraft and strategic-memory projections are later slices.
    pub fn opening_commander_observation(&mut self, owner: usize) -> Result<Observation, String> {
        self.validate_opening_projection(owner)?;
        let g = &self.world.game;
        if g.tick % 2 != 0 {
            return Err("commander observation requires a two-tick delivery boundary".into());
        }
        // Ordinary createGame has no initial vision. step updates it at 1,5,9,...
        // Never replace that with a fresh between-phase look during delivery.
        let expected = (g.tick > 0).then(|| g.tick - (g.tick - 1) % 4);
        if g.vision_tick != expected {
            return Err("commander requires the canonical vision phase".into());
        }
        self.refresh_recipient_routing(owner)?;
        let g = &self.world.game;
        let seen: BTreeSet<_> = g
            .units
            .values()
            .filter(|u| {
                g.reveal
                    || g.allied(u.owner, owner)
                    || (g.players[owner].sees(u.id) && (u.air.is_none() || vision::airborne(u)))
            })
            .map(|u| u.id)
            .collect();
        // ai-view starts with the public map, hiding authored mines, and applies
        // remembered rows. Route flags have different mine semantics, so decode
        // the AI consumer directly rather than forwarding pathfinding masks.
        let initial = self.world.initial();
        let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
        let terrain = catalog.table("TERRAIN").map_err(|e| e.to_string())?;
        let flag = |ch: u8| -> u16 {
            terrain
                .get(&char::from(ch).to_string())
                .and_then(|v| v.number())
                .unwrap_or(0.0) as u16
        };
        let mut flags: Vec<_> = initial
            .chars
            .iter()
            .enumerate()
            .map(|(c, &ch)| {
                let ch = if initial.mines[c] != 0 {
                    if initial.objects[c] != b'.' {
                        initial.objects[c]
                    } else {
                        initial.ground[c]
                    }
                } else {
                    ch
                };
                flag(ch)
            })
            .collect();
        for observed in self.recipient_routing(owner)?.remembered() {
            let row = &observed.row;
            let surface = if row.data.object != b'.' {
                row.data.object
            } else {
                row.data.ground
            };
            flags[row.cell] = flag(if row.ch == b'N' { surface } else { row.ch });
        }
        let players = g
            .players
            .iter()
            .enumerate()
            .map(|(slot, p)| PlayerObservation {
                team: p.team,
                out: self.start.economy.players[slot].out,
                spawn: self.start.players[slot].spawn,
            })
            .collect();
        let points = self
            .start
            .points
            .iter()
            .map(|p| PointObservation {
                at: p.at,
                vp: p.vp,
                mp: p.mp,
                owner: p.owner,
                cut: p.cut,
                locked: p.needs.is_some_and(|i| {
                    !g.allied(p.owner.unwrap_or(usize::MAX), owner)
                        && !g.allied(self.start.points[i].owner.unwrap_or(usize::MAX), owner)
                }),
            })
            .collect();
        let mut units = Vec::with_capacity(seen.len());
        for u in g.units.values().filter(|u| seen.contains(&u.id)) {
            let own = u.owner == owner;
            let target = self.firing.get(u.id).map_or(0, |s| s.target_id);
            let target_unit = g.units.get(&target);
            // seenBy tests the visibility set even if a target has just died.
            let target_seen = (g.reveal && target_unit.is_some())
                || target_unit.is_some_and(|v| g.allied(v.owner, owner))
                || (g.players[owner].sees(target)
                    && target_unit.is_none_or(|v| v.air.is_none() || vision::airborne(v)));
            let target_id = if target_seen { target } else { 0 };
            let mut row = UnitObservation {
                id: u.id,
                kind: self.rules.kinds[u.kind].name.clone(),
                owner: u.owner,
                at: point(u.at()),
                hp: u.hp.ceil(),
                supp: math::round(u.supp),
                target_id,
                cover: self.commander_cover(u),
                path: Vec::new(),
                attack_id: false,
                retreating: u.retreating,
                riding: u.riding,
                build: u.build,
                dig: u.dig,
                enter: false,
                fire_at: false,
                nade: false,
            };
            if own && !self.rules.kinds[u.kind].structure {
                row.path = u.path.iter().copied().map(point).collect();
                let c = &self.commands.units[&u.id];
                // Decode the precedence relevant to busy selection, without
                // inventing coordinates for unrepresented dig/build jobs.
                if u.board != 0 && g.units.contains_key(&u.board) {
                    row.enter = true;
                } else if !u.retreating {
                    if c.attack_id != 0
                        && g.units.contains_key(&c.attack_id)
                        && g.players[owner].sees(c.attack_id)
                    {
                        row.attack_id = true;
                    } else if c.fire_at.is_some() {
                        row.fire_at = true;
                    } else if c.nade.is_some() {
                        row.nade = true;
                    } else if !u.dig && !u.entrench && !u.build && c.enter.is_some() {
                        row.enter = true;
                    }
                }
            }
            units.push(row);
        }
        Observation::new(Input {
            width: g.w,
            height: g.h,
            tick: g.tick,
            slot: owner,
            players,
            units,
            points,
            flags,
        })
        .map_err(|e| format!("commander observation: {e:?}"))
    }
    fn commander_cover(&self, u: &Unit) -> u8 {
        let g = &self.world.game;
        if vision::in_trench(g, &self.rules, u) {
            return 2;
        }
        if crate::terrain::in_cover(g, &self.rules, u) {
            return 1;
        }
        if !self.rules.kinds[u.kind].infantry {
            return 0;
        }
        let c = (u.z / CELL).floor() as isize * g.w as isize + (u.x / CELL).floor() as isize;
        let at = |c: isize| {
            usize::try_from(c)
                .ok()
                .and_then(|c| g.chars.get(c))
                .copied()
        };
        if [c + 1, c - 1, c + g.w as isize, c - g.w as isize]
            .into_iter()
            .any(|c| at(c).is_some_and(|ch| b"B#RHKQ".contains(&ch)))
            || [
                c + g.w as isize + 1,
                c + g.w as isize - 1,
                c - g.w as isize + 1,
                c - g.w as isize - 1,
            ]
            .into_iter()
            .any(|c| at(c).is_some_and(|ch| matches!(ch, b'B' | b'K')))
            || g.units.values().any(|v| {
                v.id != u.id
                    && v.air.is_none()
                    && !self.rules.kinds[v.kind].infantry
                    && math::hypot(u.x - v.x, u.z - v.z) < 3.5
            })
        {
            3
        } else {
            0
        }
    }
    /// Produce on fixed delivery ticks and enqueue at the next authoritative boundary.
    /// Queue, policy memory and purpose RNG commit together after preflight.
    pub fn tick_opening_commander(
        &mut self,
        seat: &mut OpeningCommander,
    ) -> Result<OpeningTick, String> {
        let now = self.world.game.tick;
        if now < 0
            || seat.owner >= self.world.game.players.len()
            || seat.last_tick.is_some_and(|t| now <= t)
        {
            return Err("invalid or repeated commander tick".into());
        }
        let next = now.checked_add(1).ok_or("commander tick exhausted")? as u64;
        self.validate_command_tick(next)?;
        self.validate_opening_projection(seat.owner)?;
        if self.finish.is_some() || self.start.economy.players[seat.owner].out {
            seat.last_tick = Some(now);
            return Ok(OpeningTick::Stopped);
        }
        if now % 4 == 1 && self.world.game.vision_tick != Some(now) {
            let grid = crate::grid::SpatialGrid::new(&self.world.game.units, 16.0);
            vision::update_vision(&mut self.world.game, &self.rules, &grid);
        }
        let mut staged = seat.clone();
        if now % 2 == 0 {
            staged.delivered = Some(self.opening_commander_observation(seat.owner)?);
        }
        staged.last_tick = Some(now);
        if !staged.level.due(now, staged.owner) {
            *seat = staged;
            return Ok(OpeningTick::Waiting);
        }
        let Some(observation) = &staged.delivered else {
            *seat = staged;
            return Ok(OpeningTick::Waiting);
        };
        let mut random = self.start.random.clone();
        let planned =
            match commander_ai::plan(observation, &mut staged.memory, staged.level, &mut random) {
                Ok(plan) => plan,
                Err(error) => {
                    *seat = staged;
                    return Ok(OpeningTick::Deferred(error));
                }
            };
        let mut queue = self.commands.queue.clone();
        let mut keys = Vec::with_capacity(planned.commands.len());
        for command in planned.commands {
            let key = OrderKey {
                tick: next,
                issuer: staged.owner as u32,
                sequence: staged.sequence,
            };
            staged.sequence = staged
                .sequence
                .checked_add(1)
                .ok_or("commander sequence exhausted")?;
            queue
                .enqueue(key, command)
                .map_err(|e| format!("commander queue: {e:?}"))?;
            keys.push(key);
        }
        self.commands.queue = queue;
        self.start.random = random;
        *seat = staged;
        Ok(OpeningTick::Queued(keys))
    }
}
