//! Source-ordered execution of the migrated legacy trigger subset.
use crate::{
    environment::{Blast, DamageContext, Effect},
    game::{self, Point},
    legacy_scenario::{
        DestroyAction, LegacyAction, LegacyStepError, PointOwner, SayAction, validate_public_say,
    },
    match_combat::DamageWorld,
    match_runtime::Match,
};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum LegacyScenarioEmission {
    Say(SayAction),
    Environment(Effect),
}

/// Execute one tick of the map's existing legacy trigger state through canonical owners.
/// Emissions are delivered as each action commits, so a later actuator error keeps the
/// already published prefix and prevents later triggers from advancing.
pub(crate) fn step_legacy_scenario_emitting(
    state: &mut Match,
    tick: u64,
    emit: &mut dyn FnMut(LegacyScenarioEmission),
) -> Result<(), String> {
    step_legacy_scenario_emitting_with_cell_hook(state, tick, emit, &mut || Ok(()))
}

fn step_legacy_scenario_emitting_with_cell_hook(
    state: &mut Match,
    tick: u64,
    emit: &mut dyn FnMut(LegacyScenarioEmission),
    after_cell: &mut dyn FnMut() -> Result<(), String>,
) -> Result<(), String> {
    state.preflight_legacy_scenario()?;
    let Match {
        scenario,
        start,
        world,
        combat,
        grid,
        rules,
        logistics,
        production,
        options,
        environment,
        next_unit_id,
        ..
    } = state;
    let Some(scenario) = scenario.as_mut() else {
        return Ok(());
    };
    let points = start
        .points
        .iter()
        .map(|point| {
            let id = point
                .id
                .as_str()
                .ok_or_else(|| "legacy scenario point identity is not a string".to_owned())?;
            let owner = match point.owner {
                Some(owner) => i32::try_from(owner)
                    .map_err(|_| "legacy scenario point owner is out of range".to_owned())?,
                None => -1,
            };
            Ok(PointOwner { id, owner })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let (wind_angle, wind_strength) = start.wind;
    scenario
        .step_emitting(tick, &points, |action| match action {
            LegacyAction::Say(say) => {
                validate_public_say(&say)
                    .map_err(|error| format!("legacy scenario Say: {error}"))?;
                emit(LegacyScenarioEmission::Say(say));
                Ok(())
            }
            LegacyAction::Destroy(destroy) => emit_destroy(
                world,
                combat,
                grid,
                rules,
                logistics,
                production,
                options.mode,
                options.tech,
                environment,
                &mut start.environment_seed,
                next_unit_id,
                wind_angle,
                wind_strength,
                destroy,
                emit,
                after_cell,
            ),
        })
        .map_err(|error| match error {
            LegacyStepError::Scenario(error) => error.to_string(),
            LegacyStepError::Emit(error) => error,
        })
}

#[allow(clippy::too_many_arguments)]
fn emit_destroy(
    world: &mut crate::world::World,
    combat: &mut crate::match_combat::CombatState,
    grid: &mut crate::grid::SpatialGrid,
    rules: &crate::rules::Rules,
    logistics: &crate::logistics::LogisticsState,
    production: &crate::production::ProductionState,
    mode: crate::economy::Mode,
    tech_enabled: bool,
    environment: &mut crate::environment::EnvironmentRuntime,
    environment_seed: &mut u32,
    next_id: &mut u32,
    wind_angle: f64,
    wind_strength: f64,
    destroy: DestroyAction,
    emit: &mut dyn FnMut(LegacyScenarioEmission),
    after_cell: &mut dyn FnMut() -> Result<(), String>,
) -> Result<(), String> {
    let [x0, y0, x1, y1] = destroy.bounds;
    if x0 > x1 || y0 > y1 || x1 >= world.game.w || y1 >= world.game.h {
        return Err("legacy scenario destroy bounds are outside the map".into());
    }
    let unit_order: Vec<_> = world.game.units.keys().copied().collect();
    let skipped = BTreeSet::new();
    let mut hit = DamageContext::plain(&skipped);
    let width = world.game.w;
    let execution = destroy.execution.as_str();
    let contact_ids: Vec<_> = (y0..=y1)
        .flat_map(|y| {
            (x0..=x1).map(move |x| {
                let cell = y * width + x;
                format!("{execution}:{cell}")
            })
        })
        .collect();
    let mut damage = DamageWorld {
        world,
        combat,
        grid,
        rules,
        logistics,
        production,
        mode,
        tech_enabled,
        environment,
        environment_seed,
        next_id,
        unit_order: &unit_order,
        wind_angle,
        wind_strength,
    };
    let mut contact_id_index = 0;
    for y in y0..=y1 {
        for x in x0..=x1 {
            hit.contact_id = Some(&contact_ids[contact_id_index]);
            contact_id_index += 1;
            let mut output = |effect| emit(LegacyScenarioEmission::Environment(effect));
            damage
                .terrain_blast_emitting(
                    Blast {
                        at: Point {
                            x: (x as f64 + 0.5) * game::CELL,
                            z: (y as f64 + 0.5) * game::CELL,
                        },
                        radius: game::CELL * 0.49,
                        damage: 100_000.0,
                        scar: 0.0,
                    },
                    &mut hit,
                    &mut output,
                )
                .map_err(|error| format!("legacy scenario destroy: {error}"))?;
            after_cell()?;
        }
    }
    Ok(())
}

impl Match {
    /// Bind a saved authored owner to the exact private definition of its source map.
    pub fn validate_authored_map_binding(
        &self,
        map: &crate::map::AuthoritativeMap,
    ) -> Result<(), String> {
        let owners = usize::from(self.scenario.is_some())
            + usize::from(self.authored_scenario.is_some())
            + usize::from(self.authored_mission.is_some());
        if owners > 1 {
            return Err("authored and legacy scenario owners cannot coexist".into());
        }
        let matches = if let Some(scenario) = &self.authored_scenario {
            scenario.matches_map(map)
        } else if let Some(mission) = &self.authored_mission {
            mission.matches_map(map)
        } else {
            crate::scenario_validation::explicit_scenario(map.source()).is_none()
        };
        if !matches {
            return Err("authored scenario definition disagrees with source map".into());
        }
        Ok(())
    }

    /// Validate admitted mission owners before ticks, snapshots and checkpoint restoration.
    pub fn preflight_legacy_scenario(&self) -> Result<(), String> {
        self.preflight_authored_scenario()?;
        if let Some(mission) = &self.authored_mission {
            if self.scenario.is_some() || self.authored_scenario.is_some() {
                return Err("authored and legacy scenario owners cannot coexist".into());
            }
            mission.validate(self.world.game.players.len())?;
            if self.world.game.tick < 0
                || mission
                    .last_tick()
                    .is_some_and(|tick| tick > self.world.game.tick as u64)
                || (self.finish.is_none()
                    && mission.last_tick().unwrap_or(0) != self.world.game.tick as u64)
            {
                return Err("authored mission clock disagrees with match tick".into());
            }
        }
        let Some(scenario) = &self.scenario else {
            return Ok(());
        };
        scenario
            .validate_legacy_subset()
            .map_err(|error| format!("legacy scenario state: {error}"))?;
        let point_ids = self
            .start
            .points
            .iter()
            .map(|point| {
                point
                    .id
                    .as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| "legacy scenario point identity is not a string".to_owned())
            })
            .collect::<Result<BTreeSet<_>, _>>()?;
        let scenario_state = scenario.state();
        let player_count = self.world.game.players.len();
        if self
            .start
            .points
            .iter()
            .any(|point| point.owner.is_some_and(|owner| owner >= player_count))
            || point_ids.len() != self.start.points.len()
            || scenario_state
                .owners
                .keys()
                .cloned()
                .collect::<BTreeSet<_>>()
                != point_ids
            || scenario_state.owners.values().flatten().any(|owner| {
                *owner < -1 || (*owner >= 0 && *owner as usize >= self.world.game.players.len())
            })
            || scenario_state.captured.iter().any(|capture| {
                let Some((point, owner)) = capture.rsplit_once(':') else {
                    return true;
                };
                let Ok(owner_slot) = owner.parse::<usize>() else {
                    return true;
                };
                !point_ids.contains(point)
                    || owner_slot >= player_count
                    || owner_slot.to_string() != owner
            })
        {
            return Err("legacy scenario point state does not match this match".into());
        }
        for trigger in scenario.definitions() {
            for action in &trigger.actions {
                if let crate::legacy_scenario::ActionDefinition::Destroy { bounds } = action {
                    let [x0, y0, x1, y1] = *bounds;
                    if x0 > x1 || y0 > y1 || x1 >= self.world.game.w || y1 >= self.world.game.h {
                        return Err("legacy scenario destroy bounds are outside the map".into());
                    }
                }
            }
        }
        Ok(())
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
        weather::WeatherChoice,
    };
    use std::cell::Cell;

    #[test]
    fn great_bridge_due_triggers_emit_say_and_commit_destroy_effects() {
        let map = AuthoritativeMap::from_json(include_bytes!(
            "../../server/src/authoritative-maps/the-great-bridge.json"
        ))
        .unwrap();
        let mut state = Match::new(
            &map,
            MatchOptions {
                seed: 701,
                mode: Mode::Conquest,
                players: (0..2)
                    .map(|team| PlayerConfig {
                        name: format!("P{team}"),
                        team,
                        faction: Faction::try_from(team as u8).unwrap(),
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
            },
        )
        .unwrap();
        state.world.game.tick = 8400;
        let old_flags = state.world.game.flags.clone();
        let mut emitted = Vec::new();

        step_legacy_scenario_emitting(&mut state, 8400, &mut |event| emitted.push(event)).unwrap();

        let say: Vec<_> = emitted
            .iter()
            .filter_map(|event| match event {
                LegacyScenarioEmission::Say(action) => Some(action),
                LegacyScenarioEmission::Environment(_) => None,
            })
            .collect();
        let effects = emitted
            .iter()
            .filter(|event| matches!(event, LegacyScenarioEmission::Environment(_)))
            .count();
        assert_eq!(say.len(), 3);
        assert_eq!(
            say[0].text,
            "Enemy engineers are wiring the great bridge. It goes up at 7:00."
        );
        assert_eq!(say[1].text, "Twenty seconds: get off the great bridge!");
        assert_eq!(say[2].at_tick, 8400);
        assert!(say.iter().all(|action| validate_public_say(action).is_ok()));
        assert!(
            effects > 0,
            "the destroy trigger must emit canonical environment effects"
        );
        assert_ne!(
            state.world.game.flags, old_flags,
            "destroy must commit terrain changes"
        );
        assert_eq!(
            state.scenario.as_ref().unwrap().state().triggers["legacy:2@match"].count,
            1
        );
        let mut persisted = state.scenario.as_ref().unwrap().state().clone();
        let point = persisted.owners.keys().next().unwrap().clone();
        persisted.captured.insert(format!("{point}:99"));
        state
            .scenario
            .as_mut()
            .unwrap()
            .restore_state(persisted)
            .unwrap();
        assert!(state.preflight_legacy_scenario().is_err());
    }

    #[test]
    fn post_damage_failure_keeps_say_effect_and_world_prefix() {
        let map = AuthoritativeMap::from_json(include_bytes!(
            "../../server/src/authoritative-maps/the-great-bridge.json"
        ))
        .unwrap();
        let mut state = Match::new(
            &map,
            MatchOptions {
                seed: 701,
                mode: Mode::Conquest,
                players: (0..2)
                    .map(|team| PlayerConfig {
                        name: format!("P{team}"),
                        team,
                        faction: Faction::try_from(team as u8).unwrap(),
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
            },
        )
        .unwrap();
        state.world.game.tick = 8400;
        let old_flags = state.world.game.flags.clone();
        let environment_count = Cell::new(0);
        let mut emitted = Vec::new();
        let result = step_legacy_scenario_emitting_with_cell_hook(
            &mut state,
            8400,
            &mut |event| {
                if matches!(event, LegacyScenarioEmission::Environment(_)) {
                    environment_count.set(environment_count.get() + 1);
                }
                emitted.push(event);
            },
            &mut || {
                if environment_count.get() > 0 {
                    Err("injected after canonical damage emission".into())
                } else {
                    Ok(())
                }
            },
        );

        assert_eq!(
            result,
            Err("injected after canonical damage emission".into())
        );
        assert!(matches!(
            emitted.first(),
            Some(LegacyScenarioEmission::Say(_))
        ));
        assert!(environment_count.get() > 0);
        assert_ne!(state.world.game.flags, old_flags);
        assert_eq!(
            state.scenario.as_ref().unwrap().state().triggers["legacy:2@match"].count,
            1
        );
        assert!(
            !state
                .scenario
                .as_ref()
                .unwrap()
                .state()
                .triggers
                .contains_key("legacy:3@match")
        );
    }
}
