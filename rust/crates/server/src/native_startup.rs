//! Trusted ordinary startup over a bound authored map and canonical Match.
use serde_json::{Value, json};
use ww2_core::{
    economy::Mode,
    economy_state::ArmySize,
    match_runtime::{Match, StartPlan},
    weather::{WeatherChoice, WeatherRow, WeatherRules},
    world::World,
};

pub(crate) const MAX_MATCH_ID: u64 = 9_007_199_254_740_991;
#[cfg(test)]
#[path = "native_map_batch3_tests.rs"]
mod map_runtime_batch3;
pub(crate) struct StartupSettings<'a> {
    pub map: &'a str,
    pub mode: &'a str,
    pub army: &'a str,
    pub weather: &'a str,
    pub sandbox: bool,
}
pub(crate) struct NativeStartup {
    map: Value,
    raw_map: Value,
    count: usize,
}
pub(crate) struct StartEnvelope {
    map: Value,
    names: Vec<String>,
    spawns: Value,
    teams: Value,
    factions: Value,
    weather: Value,
    sandbox: bool,
    match_id: u64,
}
impl NativeStartup {
    pub(crate) fn public_map(&self) -> &Value {
        &self.map
    }
    pub(crate) fn prepare(state: &Match, settings: StartupSettings<'_>) -> Result<Self, String> {
        let authored = crate::authoritative_maps::load(settings.map).map_err(|e| e.to_string())?;
        Self::prepare_authored(state, settings, authored)
    }

    pub(crate) fn raw_map(&self) -> &Value {
        &self.raw_map
    }
    pub(crate) fn prepare_loaded(
        state: &Match,
        settings: StartupSettings<'_>,
        raw: &Value,
    ) -> Result<Self, String> {
        let authored = ww2_core::map::AuthoritativeMap::from_json(
            &serde_json::to_vec(raw).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        Self::prepare_authored(state, settings, &authored)
    }
    fn prepare_authored(
        state: &Match,
        settings: StartupSettings<'_>,
        authored: &ww2_core::map::AuthoritativeMap,
    ) -> Result<Self, String> {
        let mode_matches = matches!(
            (settings.mode, state.options.mode, state.world.game.mode),
            ("conquest", Mode::Default | Mode::Conquest, None)
                | ("horde", Mode::Horde, Some(ww2_core::game::ModeKind::Horde))
                | (
                    "tutorial",
                    Mode::Tutorial,
                    Some(ww2_core::game::ModeKind::Tutorial)
                )
                | (
                    "assault",
                    Mode::Assault,
                    Some(ww2_core::game::ModeKind::Assault)
                )
                | (
                    "annihilation",
                    Mode::Annihilation,
                    Some(ww2_core::game::ModeKind::Annihilation)
                )
                | (
                    "classic",
                    Mode::Classic,
                    Some(ww2_core::game::ModeKind::Classic)
                )
        );
        if !mode_matches
            || state.options.map_key.as_deref() != Some(settings.map)
            || state.options.weather != WeatherChoice::from_reference(Some(settings.weather))
            || state.options.sandbox != settings.sandbox
        {
            return Err("native startup settings disagree with the lobby".into());
        }
        let army = match state.options.army {
            ArmySize::Standard => "standard",
            ArmySize::Large => "large",
            ArmySize::Massive => "massive",
            ArmySize::Endless => "endless",
        };
        if army != settings.army {
            return Err("native startup army disagrees with the lobby".into());
        }
        let public = ww2_core::map::map_for_client(authored.source()).map_err(|e| e.to_string())?;
        let initial = World::from_map(authored).map_err(|e| e.to_string())?;
        let expected = initial.initial();
        let actual = state.world.initial();
        if authored.dimensions() != (state.world.game.w, state.world.game.h)
            || state.start.dimensions != authored.dimensions()
            || actual.chars != expected.chars
            || actual.ground != expected.ground
            || actual.objects != expected.objects
            || actual.mines != expected.mines
            || actual.height != expected.height
            || match (&actual.trench_front, &expected.trench_front) {
                (Some(a), Some(b)) => {
                    a.len() != b.len() || a.iter().zip(b).any(|(a, b)| a.to_bits() != b.to_bits())
                }
                (None, None) => false,
                _ => true,
            }
        {
            return Err("native startup authoring map binding changed".into());
        }
        let expected_mission = ww2_core::authored_mission::AuthoredMission::from_map(authored)?;
        if state
            .authored_mission
            .as_ref()
            .map(|mission| mission.definition())
            != expected_mission
                .as_ref()
                .map(|mission| mission.definition())
        {
            return Err("native startup mission definition disagrees with the bound map".into());
        }
        let plan = match state.options.mode {
            Mode::Horde => StartPlan::prepare_horde(authored, &state.options, &state.rules)?,
            Mode::Tutorial => StartPlan::prepare_tutorial(authored, &state.options, &state.rules)?,
            Mode::Assault => StartPlan::prepare_assault(
                authored,
                &state.options,
                &state.rules,
                state
                    .assault
                    .ok_or("missing Assault startup state")?
                    .defender_team,
            )?,
            Mode::Classic | Mode::Annihilation => {
                StartPlan::prepare_elimination(authored, &state.options, &state.rules)?
            }
            _ => StartPlan::prepare(authored, &state.options, &state.rules)?,
        };
        if plan.players.len() != state.start.players.len()
            || plan
                .players
                .iter()
                .zip(&state.start.players)
                .any(|(a, b)| a.spawn != b.spawn)
            || plan.points.len() != state.start.points.len()
            || plan.points.iter().zip(&state.start.points).any(|(a, b)| {
                a.id != b.id || a.at != b.at || a.vp != b.vp || a.mp != b.mp || a.kind != b.kind
            })
        {
            return Err("native startup layout disagrees with the bound map".into());
        }
        Ok(Self {
            map: public,
            raw_map: authored.source().clone(),
            count: plan.players.len(),
        })
    }
    pub(crate) fn envelope(
        &self,
        state: &Match,
        names: &[String],
        match_id: u64,
    ) -> Result<StartEnvelope, String> {
        let mut names = names.to_vec();
        if state
            .tutorial_state()
            .is_some_and(|t| t.enemy() == names.len())
        {
            names.push(state.options.players[names.len()].name.clone());
        }
        if names.len() != self.count
            || state.start.players.len() != self.count
            || state.world.game.players.len() != self.count
            || !(1..=MAX_MATCH_ID).contains(&match_id)
        {
            return Err("native startup roster or match identity changed".into());
        }
        let weather = WeatherRules::compiled()
            .map_err(|e| e.to_string())?
            .weather_row(Some(&state.start.weather), state.world.game.tick as u64);
        let weather = match weather {
            WeatherRow::Current(kind) => json!([kind.name()]),
            WeatherRow::Changing { now, next, seconds } => {
                json!([now.name(), next.name(), seconds])
            }
        };
        Ok(StartEnvelope {
            map: self.map.clone(),
            names: names.to_vec(),
            spawns: json!(
                state
                    .start
                    .players
                    .iter()
                    .map(|p| json!({"x":p.spawn.x,"z":p.spawn.z}))
                    .collect::<Vec<_>>()
            ),
            teams: json!(
                state
                    .world
                    .game
                    .players
                    .iter()
                    .map(|p| p.team)
                    .collect::<Vec<_>>()
            ),
            factions: json!(
                state
                    .start
                    .players
                    .iter()
                    .map(|p| p.config.faction as u8)
                    .collect::<Vec<_>>()
            ),
            weather,
            sandbox: state.options.sandbox,
            match_id,
        })
    }
}
impl StartEnvelope {
    /// Full delivery has already admitted these exact seats and generated both fields.
    pub(crate) fn message(&self, slot: usize, mut snapshot: Value) -> Value {
        let fields = snapshot
            .as_object_mut()
            .expect("canonical full delivery object");
        let cells = fields
            .remove("cells")
            .expect("canonical full delivery cells");
        let fog = fields.remove("fog").expect("canonical full delivery fog");
        json!({"t":"start", "matchId":self.match_id, "map":self.map, "you":slot,
            "spawn":self.spawns[slot], "spawns":self.spawns, "cells":cells, "fog":fog,
            "names":self.names, "teams":self.teams, "factions":self.factions,
            "weather":self.weather, "sandbox":self.sandbox})
    }
}
