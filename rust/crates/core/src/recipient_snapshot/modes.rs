use super::*;
use crate::game::ModeKind;

/// The public mode component from prototype sim.js:5895-5898.
/// Whole-frame admission remains separate from this recipient-independent row.
pub fn mode_row(state: &Match) -> Result<Value, SnapshotError> {
    let pair = (state.options.mode, state.world.game.mode);
    let timer = |time: f64| -> Result<f64, SnapshotError> {
        finite(&[time])?;
        Ok(math::max(0.0, time.ceil()))
    };
    match pair {
        (Mode::Default | Mode::Conquest, None) => Ok(Value::Null),
        (Mode::Assault, Some(ModeKind::Assault)) => {
            let mode = state.assault.ok_or(SnapshotError::Rules)?;
            Ok(json!({"kind":"assault", "defenderTeam":mode.defender_team,
                "attackerTeam":mode.attacker_team, "timeLeft":timer(mode.time_left)?,
                "suddenDeath":false, "total":mode.total}))
        }
        (Mode::Annihilation, Some(ModeKind::Annihilation)) => {
            let mode = state.elimination.as_ref().ok_or(SnapshotError::Rules)?;
            Ok(
                json!({"kind":"annihilation", "suddenDeath":mode.sudden_death,
                "bunkers":mode.bunkers}),
            )
        }
        (Mode::Classic, Some(ModeKind::Classic)) => {
            let mode = state.elimination.as_ref().ok_or(SnapshotError::Rules)?;
            Ok(
                json!({"kind":"classic", "timeLeft":timer(mode.time_left.ok_or(SnapshotError::Rules)?)?,
                "suddenDeath":mode.sudden_death}),
            )
        }
        (Mode::Horde, Some(ModeKind::Horde)) => super::horde_mode_row(state),
        (Mode::Tutorial, Some(ModeKind::Tutorial)) => {
            let mode = state.tutorial_state().ok_or(SnapshotError::Rules)?;
            let steps = crate::tutorial_steps::TutorialStep::ALL;
            let step = steps
                .iter()
                .position(|step| *step == mode.step())
                .ok_or(SnapshotError::Rules)?;
            Ok(
                json!({"kind":"tutorial", "suddenDeath":false, "slot":mode.enemy(),
                "goal":mode.goal(), "step":step, "steps":steps.len()}),
            )
        }
        _ => Err(SnapshotError::UnsupportedMode),
    }
}
