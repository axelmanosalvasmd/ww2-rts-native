//! Typed authored-scenario conditions over canonical match facts.
//! This component does not admit maps, schedule triggers or project recipient state.
use std::collections::{BTreeMap, BTreeSet};

use crate::{game::CELL, grid::UnitId, match_runtime::Match};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Scope {
    Match,
    Player(usize),
    Team(i32),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Context {
    pub scope: Scope,
    pub slot: usize,
    pub trigger_side: Option<usize>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    Point(String),
    Region(i32),
}
impl Target {
    fn is_region(&self) -> bool {
        matches!(self, Self::Region(_))
            || matches!(self, Self::Point(id) if id.starts_with("region:"))
    }
    fn key(&self) -> String {
        match self {
            Self::Point(id) => id.clone(),
            Self::Region(id) => format!("region:{id}"),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Comparison {
    AtMost,
    AtLeast,
    Equal,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Condition {
    pub side: Option<usize>,
    pub kind: Kind,
}
#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Time(f64),
    Ownership(Target),
    Capture(Target),
    UnitLost(String),
    GroupLost(String),
    GroupCount {
        group: String,
        comparison: Comparison,
        count: usize,
    },
    AreaEntry {
        area: String,
        group: Option<String>,
    },
    ObjectiveComplete(String),
    TriggerComplete(String),
    All(Vec<Condition>),
    Any(Vec<Condition>),
}
/// Durable facts owned by the future scenario runtime. Unit and owner facts stay in Match.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ConditionState {
    pub groups: BTreeMap<String, Vec<UnitId>>,
    pub authored_units: BTreeMap<String, UnitId>,
    pub started_groups: BTreeSet<String>,
    pub deferred_groups: BTreeSet<String>,
    pub captured: BTreeSet<(String, i32)>,
    pub completed_objectives: BTreeSet<(String, Scope)>,
    pub trigger_counts: BTreeMap<(String, Scope), u32>,
    pub areas: BTreeMap<String, [usize; 4]>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConditionError {
    WorkExhausted,
    DepthExceeded,
    InvalidFacts,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorkBudget {
    pub left: usize,
}
impl WorkBudget {
    fn spend(&mut self) -> Result<(), ConditionError> {
        self.left = self
            .left
            .checked_sub(1)
            .ok_or(ConditionError::WorkExhausted)?;
        Ok(())
    }
}
impl Match {
    /// Evaluate without mutating match or scenario state. Share the budget across a tick.
    /// The 64-level guard bounds the call stack even for caller-constructed condition trees.
    pub fn evaluate_scenario_condition(
        &self,
        condition: &Condition,
        state: &ConditionState,
        context: Context,
        budget: &mut WorkBudget,
    ) -> Result<bool, ConditionError> {
        self.check_scenario_condition(condition, state, context, budget, 0)
    }

    fn scenario_group_alive(
        &self,
        state: &ConditionState,
        group: &str,
        budget: &mut WorkBudget,
    ) -> Result<Vec<UnitId>, ConditionError> {
        let mut alive = Vec::new();
        for &id in state.groups.get(group).into_iter().flatten() {
            budget.spend()?;
            if self.world.game.units.get(&id).is_some_and(|u| u.hp > 0.0) {
                alive.push(id);
            }
        }
        Ok(alive)
    }

    fn check_scenario_condition(
        &self,
        q: &Condition,
        state: &ConditionState,
        context: Context,
        budget: &mut WorkBudget,
        depth: usize,
    ) -> Result<bool, ConditionError> {
        if depth >= 64 {
            return Err(ConditionError::DepthExceeded);
        }
        budget.spend()?;
        let side = q.side.or(context.trigger_side).unwrap_or(context.slot);
        let players = &self.world.game.players;
        let team = || players.get(side).map(|p| p.team);
        let key = |id: &str, scope| (id.to_owned(), scope);
        let owner = |target: &Target| -> Option<i32> {
            match target {
                Target::Point(id) if !id.starts_with("region:") => self
                    .start
                    .points
                    .iter()
                    .find(|p| p.id.as_str() == Some(id.as_str()))
                    .map(|p| p.owner.map_or(-1, |slot| slot as i32)),
                _ => self.world.geography().and_then(|g| {
                    let key = target.key();
                    let index = g
                        .regions
                        .iter()
                        .position(|r| format!("region:{}", r.id) == key)?;
                    self.world_runtime
                        .as_ref()?
                        .regions
                        .get(index)
                        .map(|r| r.team)
                }),
            }
        };
        Ok(match &q.kind {
            Kind::All(children) => {
                for child in children {
                    if !self.check_scenario_condition(child, state, context, budget, depth + 1)? {
                        return Ok(false);
                    }
                }
                true
            }
            Kind::Any(children) => {
                for child in children {
                    if self.check_scenario_condition(child, state, context, budget, depth + 1)? {
                        return Ok(true);
                    }
                }
                false
            }
            Kind::Time(seconds) => {
                if !seconds.is_finite() || *seconds < 0.0 || self.world.game.tick < 0 {
                    return Err(ConditionError::InvalidFacts);
                }
                self.world.game.tick as f64 * 0.05 >= *seconds
            }
            Kind::Ownership(target) => {
                if target.is_region() {
                    owner(target).zip(team()).is_some_and(|(a, b)| a == b)
                } else if let Scope::Team(id) = context.scope
                    && q.side.is_none()
                {
                    owner(target)
                        .and_then(|o| usize::try_from(o).ok())
                        .and_then(|o| players.get(o))
                        .is_some_and(|p| p.team == id)
                } else {
                    owner(target)
                        .zip(i32::try_from(side).ok())
                        .is_some_and(|(a, b)| a == b)
                }
            }
            Kind::Capture(target) => {
                let target_key = target.key();
                if let Scope::Team(id) = context.scope
                    && q.side.is_none()
                    && !target.is_region()
                {
                    players.iter().any(|p| {
                        p.team == id
                            && state
                                .captured
                                .contains(&(target_key.clone(), p.slot as i32))
                    })
                } else {
                    let identity = if target.is_region() {
                        team()
                    } else {
                        i32::try_from(side).ok()
                    };
                    identity.is_some_and(|id| state.captured.contains(&(target_key, id)))
                }
            }
            Kind::UnitLost(id) => state
                .authored_units
                .get(id)
                .is_some_and(|id| !self.world.game.units.get(id).is_some_and(|u| u.hp > 0.0)),
            Kind::GroupLost(group) => {
                state.started_groups.contains(group)
                    && self.scenario_group_alive(state, group, budget)?.is_empty()
                    && !state.deferred_groups.contains(group)
            }
            Kind::GroupCount {
                group,
                comparison,
                count,
            } => {
                let n = self.scenario_group_alive(state, group, budget)?.len();
                match comparison {
                    Comparison::AtMost => n <= *count,
                    Comparison::AtLeast => n >= *count,
                    Comparison::Equal => n == *count,
                }
            }
            Kind::AreaEntry { area, group } => {
                let b = state.areas.get(area).ok_or(ConditionError::InvalidFacts)?;
                if b[0] > b[2]
                    || b[1] > b[3]
                    || b[2] >= self.world.game.w
                    || b[3] >= self.world.game.h
                {
                    return Err(ConditionError::InvalidFacts);
                }
                let inside = |u: &crate::game::Unit| {
                    u.hp > 0.0
                        && u.x >= b[0] as f64 * CELL
                        && u.x < (b[2] + 1) as f64 * CELL
                        && u.z >= b[1] as f64 * CELL
                        && u.z < (b[3] + 1) as f64 * CELL
                };
                if let Some(group) = group {
                    for id in self.scenario_group_alive(state, group, budget)? {
                        budget.spend()?;
                        if inside(&self.world.game.units[&id]) {
                            return Ok(true);
                        }
                    }
                } else {
                    for u in self.world.game.units.values() {
                        budget.spend()?;
                        if u.owner == side && inside(u) {
                            return Ok(true);
                        }
                    }
                }
                false
            }
            Kind::ObjectiveComplete(id) => {
                state.completed_objectives.contains(&key(id, context.scope))
                    || state.completed_objectives.contains(&key(id, Scope::Match))
            }
            Kind::TriggerComplete(id) => {
                state
                    .trigger_counts
                    .get(&key(id, context.scope))
                    .or_else(|| state.trigger_counts.get(&key(id, Scope::Match)))
                    .copied()
                    .unwrap_or(0)
                    > 0
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        economy::{Faction, Mode},
        economy_state::ArmySize,
        map::AuthoritativeMap,
        match_runtime::{MatchOptions, PlayerConfig},
        random::MatchRandom,
        weather::WeatherChoice,
    };
    use serde_json::{Value, json};

    fn options(mode: Mode) -> MatchOptions {
        MatchOptions {
            seed: 83,
            mode,
            players: [5, 1, 5]
                .into_iter()
                .enumerate()
                .map(|(slot, team)| PlayerConfig {
                    name: format!("p{slot}"),
                    team,
                    faction: Faction::Ussr,
                })
                .collect(),
            shuffle: false,
            army: ArmySize::Standard,
            weather: WeatherChoice::Map,
            weather_seed: Some(1),
            living_weather: false,
            map_key: None,
            tech: false,
            supply: false,
            logistics: false,
            sandbox: false,
        }
    }
    fn scene() -> Match {
        let map = AuthoritativeMap::from_json(
            &serde_json::to_vec(&json!({
                "name":"Scenario conditions", "w":64, "h":64, "rows":vec![".".repeat(64);64],
                "spawns":[{"x":4,"y":4},{"x":58,"y":58},{"x":4,"y":58}],
                "points":[{"id":"point","x":32,"y":32}]
            }))
            .unwrap(),
        )
        .unwrap();
        Match::new(&map, options(Mode::Conquest)).unwrap()
    }
    fn q(kind: Kind) -> Condition {
        Condition { side: None, kind }
    }
    fn context() -> Context {
        Context {
            scope: Scope::Match,
            slot: 0,
            trigger_side: None,
        }
    }
    fn check(m: &Match, s: &ConditionState, c: Context, condition: Condition) -> bool {
        m.evaluate_scenario_condition(&condition, s, c, &mut WorkBudget { left: 4096 })
            .unwrap()
    }
    #[test]
    fn mode83_ownership_capture_and_explicit_side_use_canonical_players() {
        let mut m = scene();
        let mut s = ConditionState::default();
        m.start.points[0].id = json!("point");
        m.start.points[0].owner = Some(2);
        let target = Target::Point("point".into());
        let team = Context {
            scope: Scope::Team(5),
            trigger_side: Some(1),
            ..context()
        };
        assert!(check(&m, &s, team, q(Kind::Ownership(target.clone()))));
        assert!(!check(
            &m,
            &s,
            team,
            Condition {
                side: Some(0),
                kind: Kind::Ownership(target.clone())
            }
        ));
        assert!(check(
            &m,
            &s,
            context(),
            Condition {
                side: Some(2),
                kind: Kind::Ownership(target.clone())
            }
        ));
        assert!(check(
            &m,
            &s,
            Context {
                trigger_side: Some(2),
                ..context()
            },
            q(Kind::Ownership(target.clone()))
        ));
        s.captured.insert(("point".into(), 2));
        m.start.points[0].owner = Some(1);
        m.start.economy.players[2].out = true;
        assert!(check(&m, &s, team, q(Kind::Capture(target.clone()))));
        assert!(!check(
            &m,
            &s,
            team,
            Condition {
                side: Some(0),
                kind: Kind::Capture(target)
            }
        ));
        assert!(!check(
            &m,
            &s,
            context(),
            q(Kind::Ownership(Target::Point("absent".into())))
        ));
    }
    #[test]
    fn mode83_reserved_region_prefix_does_not_read_a_same_named_point() {
        let mut m = scene();
        let mut s = ConditionState::default();
        m.start.points[0].id = json!("region:7");
        m.start.points[0].owner = Some(2);
        s.captured.insert(("region:7".into(), 2));
        let c = Context {
            slot: 2,
            ..context()
        };
        let target = Target::Point("region:7".into());
        assert!(!check(&m, &s, c, q(Kind::Ownership(target.clone()))));
        assert!(!check(&m, &s, c, q(Kind::Capture(target.clone()))));
        s.captured.insert(("region:7".into(), 5));
        assert!(check(&m, &s, c, q(Kind::Capture(target))));
    }
    #[test]
    fn mode83_loss_counts_and_arrival_state_distinguish_unarrived_and_dead() {
        let mut m = scene();
        let mut s = ConditionState::default();
        let id = *m.world.game.units.keys().next().unwrap();
        s.groups.insert("squad".into(), vec![id, u32::MAX]);
        assert!(!check(
            &m,
            &s,
            context(),
            q(Kind::UnitLost("authored".into()))
        ));
        s.authored_units.insert("authored".into(), id);
        assert!(!check(
            &m,
            &s,
            context(),
            q(Kind::UnitLost("authored".into()))
        ));
        for comparison in [Comparison::AtMost, Comparison::AtLeast, Comparison::Equal] {
            assert!(check(
                &m,
                &s,
                context(),
                q(Kind::GroupCount {
                    group: "squad".into(),
                    comparison,
                    count: 1
                })
            ));
        }
        assert!(!check(
            &m,
            &s,
            context(),
            q(Kind::GroupCount {
                group: "squad".into(),
                comparison: Comparison::AtLeast,
                count: 2
            })
        ));
        assert!(!check(
            &m,
            &s,
            context(),
            q(Kind::GroupCount {
                group: "squad".into(),
                comparison: Comparison::AtMost,
                count: 0
            })
        ));
        assert!(!check(
            &m,
            &s,
            context(),
            q(Kind::GroupCount {
                group: "squad".into(),
                comparison: Comparison::Equal,
                count: 0
            })
        ));
        assert_eq!(
            m.evaluate_scenario_condition(
                &q(Kind::GroupCount {
                    group: "squad".into(),
                    comparison: Comparison::Equal,
                    count: 1
                }),
                &s,
                context(),
                &mut WorkBudget { left: 2 }
            ),
            Err(ConditionError::WorkExhausted)
        );
        m.world.game.units.get_mut(&id).unwrap().hp = 0.0;
        assert!(check(
            &m,
            &s,
            context(),
            q(Kind::UnitLost("authored".into()))
        ));
        assert!(!check(
            &m,
            &s,
            context(),
            q(Kind::GroupLost("squad".into()))
        ));
        s.started_groups.insert("squad".into());
        s.deferred_groups.insert("squad".into());
        assert!(!check(
            &m,
            &s,
            context(),
            q(Kind::GroupLost("squad".into()))
        ));
        s.deferred_groups.clear();
        assert!(check(&m, &s, context(), q(Kind::GroupLost("squad".into()))));
        m.world.game.units.remove(&id);
        assert!(check(
            &m,
            &s,
            context(),
            q(Kind::UnitLost("authored".into()))
        ));
    }
    #[test]
    fn mode83_area_entry_is_half_open_and_group_scan_charges_twice() {
        let mut m = scene();
        let mut s = ConditionState::default();
        let id = *m.world.game.units.keys().next().unwrap();
        m.world.game.units.retain(|key, _| *key == id);
        let u = m.world.game.units.get_mut(&id).unwrap();
        u.owner = 1;
        u.x = 4.0;
        u.z = 6.0;
        s.areas.insert("box".into(), [2, 3, 3, 4]);
        s.groups.insert("squad".into(), vec![id, u32::MAX]);
        let group = q(Kind::AreaEntry {
            area: "box".into(),
            group: Some("squad".into()),
        });
        assert!(check(&m, &s, context(), group.clone()));
        let side = q(Kind::AreaEntry {
            area: "box".into(),
            group: None,
        });
        assert!(!check(&m, &s, context(), side.clone()));
        assert!(check(
            &m,
            &s,
            Context {
                trigger_side: Some(1),
                ..context()
            },
            side
        ));
        let mut budget = WorkBudget { left: 4 };
        assert_eq!(
            m.evaluate_scenario_condition(&group, &s, context(), &mut budget),
            Ok(true)
        );
        assert_eq!(budget.left, 0);
        assert_eq!(
            m.evaluate_scenario_condition(&group, &s, context(), &mut WorkBudget { left: 3 }),
            Err(ConditionError::WorkExhausted)
        );
        for (x, z) in [(8.0, 6.0), (4.0, 10.0), (3.999, 6.0), (4.0, 5.999)] {
            let u = m.world.game.units.get_mut(&id).unwrap();
            u.x = x;
            u.z = z;
            assert!(!check(&m, &s, context(), group.clone()));
        }
        let u = m.world.game.units.get_mut(&id).unwrap();
        u.x = 4.0;
        u.z = 6.0;
        u.hp = 0.0;
        assert!(!check(&m, &s, context(), group));
    }
    #[test]
    fn mode83_completion_fallback_and_boolean_work_follow_source_short_circuiting() {
        let mut m = scene();
        let mut s = ConditionState::default();
        let c = Context {
            scope: Scope::Player(0),
            ..context()
        };
        s.completed_objectives
            .insert(("objective".into(), Scope::Match));
        s.trigger_counts.insert(("trigger".into(), Scope::Match), 1);
        assert!(check(
            &m,
            &s,
            c,
            q(Kind::ObjectiveComplete("objective".into()))
        ));
        s.completed_objectives.insert(("local".into(), c.scope));
        assert!(check(&m, &s, c, q(Kind::ObjectiveComplete("local".into()))));
        assert!(!check(
            &m,
            &s,
            Context {
                scope: Scope::Player(1),
                ..c
            },
            q(Kind::ObjectiveComplete("local".into()))
        ));
        assert!(!check(
            &m,
            &s,
            c,
            q(Kind::ObjectiveComplete("absent".into()))
        ));
        assert!(check(&m, &s, c, q(Kind::TriggerComplete("trigger".into()))));
        s.trigger_counts.insert(("trigger".into(), c.scope), 0);
        assert!(!check(
            &m,
            &s,
            c,
            q(Kind::TriggerComplete("trigger".into()))
        ));
        m.world.game.tick = 20;
        assert!(check(&m, &s, c, q(Kind::Time(1.0))));
        assert!(!check(&m, &s, c, q(Kind::Time(1.01))));
        assert!(check(&m, &s, c, q(Kind::All(vec![]))));
        assert!(!check(&m, &s, c, q(Kind::Any(vec![]))));
        assert!(check(
            &m,
            &s,
            c,
            q(Kind::All(vec![
                q(Kind::Any(vec![q(Kind::Time(2.0)), q(Kind::Time(1.0))])),
                q(Kind::Time(0.0))
            ]))
        ));
        assert_eq!(
            m.evaluate_scenario_condition(
                &q(Kind::Time(f64::NAN)),
                &s,
                c,
                &mut WorkBudget { left: 4096 }
            ),
            Err(ConditionError::InvalidFacts)
        );
        assert_eq!(
            m.evaluate_scenario_condition(
                &q(Kind::AreaEntry {
                    area: "missing".into(),
                    group: None
                }),
                &s,
                c,
                &mut WorkBudget { left: 4096 }
            ),
            Err(ConditionError::InvalidFacts)
        );
        let tree = q(Kind::Any(vec![q(Kind::Time(0.0)), q(Kind::Time(2.0))]));
        let mut budget = WorkBudget { left: 2 };
        assert_eq!(
            m.evaluate_scenario_condition(&tree, &s, c, &mut budget),
            Ok(true)
        );
        assert_eq!(budget.left, 0);
        assert_eq!(
            m.evaluate_scenario_condition(&tree, &s, c, &mut WorkBudget { left: 1 }),
            Err(ConditionError::WorkExhausted)
        );
        let all = q(Kind::All(vec![q(Kind::Time(2.0)), q(Kind::Time(0.0))]));
        assert_eq!(
            m.evaluate_scenario_condition(&all, &s, c, &mut WorkBudget { left: 2 }),
            Ok(false)
        );
        let mut deep = q(Kind::Time(0.0));
        for _ in 0..64 {
            deep = q(Kind::All(vec![deep]));
        }
        assert_eq!(
            m.evaluate_scenario_condition(&deep, &s, c, &mut WorkBudget { left: 4096 }),
            Err(ConditionError::DepthExceeded)
        );
    }
    #[test]
    fn mode83_region_ownership_and_capture_use_region_team_not_vector_index() {
        let f: Value = serde_json::from_str(include_str!(
            "../../simtest/tests/fixtures/world-phases.json"
        ))
        .unwrap();
        let regions = f["definitions"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
            .map(|(i, r)| {
                let mut r = r.clone();
                r["bounds"] = json!([i % 8 * 64, i / 8 * 64, i % 8 * 64 + 64, i / 8 * 64 + 64]);
                if let Some(home) = [0, 63, 1].iter().position(|v| *v == i) {
                    r["home"] = json!(home);
                }
                r
            })
            .collect::<Vec<_>>();
        let map=AuthoritativeMap::from_json(&serde_json::to_vec(&json!({
            "name":"Scenario region conditions","w":512,"h":512,"rows":vec![".".repeat(512);512],
            "spawns":[{"x":32,"y":32},{"x":480,"y":480},{"x":96,"y":32}],"points":[],
            "world":{"size":"huge","total":64,"regionCells":64,"regions":regions}
        })).unwrap()).unwrap();
        let mut m = Match::new_world(&map, options(Mode::World), MatchRandom::new(83)).unwrap();
        let id = m.world.geography().unwrap().regions[2].id;
        m.world_runtime.as_mut().unwrap().regions[2].team = 5;
        let mut s = ConditionState::default();
        assert!(check(
            &m,
            &s,
            context(),
            q(Kind::Ownership(Target::Point(format!("region:{id}"))))
        ));
        assert!(check(
            &m,
            &s,
            context(),
            q(Kind::Ownership(Target::Region(id)))
        ));
        assert!(!check(
            &m,
            &s,
            context(),
            Condition {
                side: Some(1),
                kind: Kind::Ownership(Target::Region(id))
            }
        ));
        s.captured.insert((format!("region:{id}"), 5));
        m.world_runtime.as_mut().unwrap().regions[2].team = 1;
        assert!(check(
            &m,
            &s,
            context(),
            q(Kind::Capture(Target::Point(format!("region:{id}"))))
        ));
        assert!(check(
            &m,
            &s,
            context(),
            q(Kind::Capture(Target::Region(id)))
        ));
        assert!(!check(
            &m,
            &s,
            context(),
            Condition {
                side: Some(1),
                kind: Kind::Capture(Target::Region(id))
            }
        ));
    }
}
