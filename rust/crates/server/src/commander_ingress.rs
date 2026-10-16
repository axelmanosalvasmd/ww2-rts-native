//! Host stamps for bounded ordinary commander input over the sole core queue.
use serde_json::Value;
use std::collections::BTreeMap;
use ww2_core::{
    match_commands::{Command, CommandResult, Denial},
    match_runtime::Match,
    orders::{OrderKey, QueueError},
};

#[derive(Default, Debug)]
pub(crate) struct CommanderIngress {
    sequence: u64,
    // Feedback names only. Canonical payloads live exclusively in Match's queue.
    feedback: BTreeMap<OrderKey, &'static str>,
}
impl CommanderIngress {
    pub fn recognizes(t: &str) -> bool {
        matches!(
            t,
            "move"
                | "amove"
                | "stop"
                | "stance"
                | "garrison"
                | "dig"
                | "entrench"
                | "cover"
                | "assist"
                | "retreat"
                | "rally"
                | "tech"
                | "cancel"
                | "cancelProduction"
                | "destroy"
                | "buy"
                | "support"
                | "fireat"
                | "supply"
                | "logisticsResume"
                | "spawn"
                | "attack"
                | "ability"
                | "autocast"
                | "build"
                | "board"
                | "unload"
                | "escort"
                | "rail"
                | "orders"
        )
    }
    pub fn enqueue(&mut self, state: &mut Match, slot: usize, raw: &Value) -> Result<(), Denial> {
        let name = match raw["t"].as_str() {
            Some("move") => "move",
            Some("amove") => "amove",
            Some("stop") => "stop",
            Some("stance") => "stance",
            Some("garrison") => "garrison",
            Some("dig") => "dig",
            Some("entrench") => "entrench",
            Some("cover") => "cover",
            Some("assist") => "assist",
            Some("retreat") => "retreat",
            Some("rally") => "rally",
            Some("tech") => "tech",
            Some("cancel") => "cancel",
            Some("cancelProduction") => "cancelProduction",
            Some("destroy") => "destroy",
            Some("buy") => "buy",
            Some("support") => "support",
            Some("fireat") => "fireat",
            Some("supply") => "supply",
            Some("logisticsResume") => "logisticsResume",
            Some("spawn") => "spawn",
            Some("attack") => "attack",
            Some("ability") => "ability",
            Some("autocast") => "autocast",
            Some("build") => "build",
            Some("board") => "board",
            Some("unload") => "unload",
            Some("escort") => "escort",
            Some("rail") => "rail",
            Some("orders") => "orders",
            _ => return Err(Denial::Blocked),
        };
        let command = Command::from_json(raw)?;
        if !matches!(
            command,
            Command::Move { .. }
                | Command::Stop { .. }
                | Command::Stance { .. }
                | Command::Enter { .. }
                | Command::Dig { .. }
                | Command::Entrench { .. }
                | Command::Cover { .. }
                | Command::Assist { .. }
                | Command::Retreat { .. }
                | Command::Rally { .. }
                | Command::Technology { .. }
                | Command::Cancel { .. }
                | Command::CancelProduction { .. }
                | Command::Destroy { .. }
                | Command::Buy { .. }
                | Command::Support { .. }
                | Command::FireAt { .. }
                | Command::Supply { .. }
                | Command::LogisticsResume { .. }
                | Command::Spawn { .. }
                | Command::Attack { .. }
                | Command::Ability { .. }
                | Command::Autocast { .. }
                | Command::Build { .. }
                | Command::Board { .. }
                | Command::Unload { .. }
                | Command::Escort { .. }
                | Command::Rail { .. }
                | Command::Compound { .. }
        ) {
            return Err(Denial::Blocked);
        }
        if matches!(&command, Command::Support { kind, .. }
            if !ww2_core::recipient_snapshot::SUPPORTED_SUPPORT_KINDS
                .iter().any(|supported| supported.name() == kind))
        {
            return Err(Denial::Unsupported);
        }
        let state_player = state
            .start
            .economy
            .players
            .get(slot)
            .ok_or(Denial::Blocked)?;
        if state.finish.is_some() || state_player.out || state_player.away {
            return Err(Denial::Blocked);
        }
        // Selection, mode admission and prospective state checks belong to the
        // canonical command executor. Keep the raw selection intact for its cap,
        // mixed-selection and per-family denial rules.
        state.validate_tick_state().map_err(|_| Denial::Blocked)?;
        let tick = u64::try_from(state.world.game.tick)
            .ok()
            .and_then(|tick| tick.checked_add(1))
            .ok_or(Denial::Blocked)?;
        state
            .validate_command_tick(tick)
            .map_err(|_| Denial::Blocked)?;
        let issuer = u32::try_from(slot).map_err(|_| Denial::Blocked)?;
        let reserved = state
            .commands
            .pending_tick(tick)
            .filter(|(key, _)| key.issuer == issuer)
            .map(|(key, _)| key.sequence)
            .max()
            .unwrap_or(0);
        let sequence = self
            .sequence
            .max(reserved)
            .checked_add(1)
            .ok_or(Denial::Blocked)?;
        let key = OrderKey {
            tick,
            issuer,
            sequence,
        };
        state
            .enqueue_command(key, command)
            .map_err(|error| match error {
                QueueError::Full => Denial::QueueFull,
                _ => Denial::Blocked,
            })?;
        // Infallible publication after the one canonical enqueue succeeds.
        self.sequence = sequence;
        self.feedback.insert(key, name);
        Ok(())
    }
    /// Faulted rooms never retry or requeue accepted inputs. Drop names only.
    pub fn discard_feedback(&mut self) {
        self.feedback.clear();
    }
    pub fn complete(&mut self, results: &[CommandResult]) -> Vec<(usize, &'static str, Denial)> {
        results
            .iter()
            .filter_map(|result| {
                let name = self.feedback.remove(&result.key)?;
                result
                    .denial
                    .map(|denial| (result.key.issuer as usize, name, denial))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reservations_preserve_preloaded_orders_and_refuse_exhaustion_without_publication() {
        let mut state = crate::lobby::native_tests::native_match();
        let mut ingress = CommanderIngress::default();
        let original = [
            (
                OrderKey {
                    tick: 1,
                    issuer: 0,
                    sequence: 2,
                },
                Command::Stop { ids: vec![-1] },
            ),
            (
                OrderKey {
                    tick: 1,
                    issuer: 0,
                    sequence: 7,
                },
                Command::Stop { ids: vec![-2] },
            ),
            (
                OrderKey {
                    tick: 1,
                    issuer: 1,
                    sequence: u64::MAX,
                },
                Command::Stop { ids: vec![-3] },
            ),
            (
                OrderKey {
                    tick: 2,
                    issuer: 0,
                    sequence: 90,
                },
                Command::Stop { ids: vec![-4] },
            ),
        ];
        for (key, command) in &original {
            state.enqueue_command(*key, command.clone()).unwrap();
        }
        ingress
            .enqueue(
                &mut state,
                0,
                &json!({"t":"stop","ids":[-5],"slot":1,"tick":900,"sequence":0}),
            )
            .unwrap();
        assert_eq!(ingress.sequence, 8);
        for (key, command) in &original {
            assert!(
                state
                    .commands
                    .pending_tick(key.tick)
                    .any(|(actual, value)| actual == key && value == command)
            );
        }
        let report = state.advance_tick().unwrap();
        assert_eq!(
            report
                .commands
                .iter()
                .map(|result| result.key)
                .collect::<Vec<_>>(),
            vec![
                original[0].0,
                original[1].0,
                OrderKey {
                    tick: 1,
                    issuer: 0,
                    sequence: 8
                },
                original[2].0
            ]
        );
        assert_eq!(
            ingress.complete(&report.commands),
            vec![(0, "stop", Denial::Needs)]
        );
        assert!(ingress.feedback.is_empty());
        assert!(
            state
                .commands
                .pending_tick(2)
                .any(|(key, command)| *key == original[3].0 && *command == original[3].1)
        );
        ingress
            .enqueue(&mut state, 0, &json!({"t":"stop","ids":[-6]}))
            .unwrap();
        assert_eq!(ingress.sequence, 91);
        let exhausted = OrderKey {
            tick: 2,
            issuer: 0,
            sequence: u64::MAX,
        };
        state
            .enqueue_command(exhausted, Command::Stop { ids: vec![-7] })
            .unwrap();
        let before = format!(
            "{:?}",
            (&ingress, state.commands.pending_tick(2).collect::<Vec<_>>())
        );
        assert_eq!(
            ingress.enqueue(&mut state, 0, &json!({"t":"stop","ids":[-8]})),
            Err(Denial::Blocked)
        );
        assert_eq!(
            format!(
                "{:?}",
                (&ingress, state.commands.pending_tick(2).collect::<Vec<_>>())
            ),
            before
        );
    }

    #[test]
    fn supported_stance_is_queued_and_malformed_rows_do_not_reserve_keys() {
        let mut state = crate::lobby::native_tests::native_match();
        let actor = state
            .world
            .game
            .units
            .values()
            .find(|u| u.owner == 0 && state.rules.kinds[u.kind].name == "rifle")
            .unwrap()
            .id;
        let mut ingress = CommanderIngress::default();
        let before = format!(
            "{:?}",
            (&state.world.game.units, &state.commands.units, &ingress)
        );
        assert_eq!(
            ingress.enqueue(
                &mut state,
                0,
                &json!({"t":"move","orders":[[actor,2,"bad"]]})
            ),
            Err(Denial::Blocked)
        );
        assert_eq!(
            format!(
                "{:?}",
                (&state.world.game.units, &state.commands.units, &ingress)
            ),
            before
        );
        assert_eq!(state.commands.pending_commands(), 0);
        ingress
            .enqueue(
                &mut state,
                0,
                &json!({"t":"stance","ids":[actor],"key":"autoRetreat","on":true}),
            )
            .unwrap();
        let report = state.advance_tick().unwrap();
        assert!(ingress.complete(&report.commands).is_empty());
        assert!(state.commands.units[&actor].auto_retreat);
    }

    #[test]
    fn garrisoned_move_and_amove_exit_and_keep_tick_snapshots_valid() {
        for name in ["move", "amove"] {
            let mut state = crate::lobby::native_tests::native_match();
            let actor = state
                .world
                .game
                .units
                .values()
                .find(|u| u.owner == 0 && state.rules.kinds[u.kind].name == "rifle")
                .unwrap()
                .id;
            let house = state
                .world
                .game
                .chars
                .iter()
                .position(|ch| *ch == b'B')
                .unwrap();
            let at = ww2_core::game::Point {
                x: (house % state.world.game.w) as f64 * ww2_core::game::CELL + 1.,
                z: (house / state.world.game.w) as f64 * ww2_core::game::CELL + 1.,
            };
            let unit = state.world.game.units.get_mut(&actor).unwrap();
            unit.x = at.x;
            unit.z = at.z;
            unit.garrison = Some(house);
            state.grid.update(actor, unit);
            let mut ingress = CommanderIngress::default();
            ingress
                .enqueue(
                    &mut state,
                    0,
                    &json!({"t":name,"orders":[[actor,at.x+12.,at.z+12.]]}),
                )
                .unwrap();
            for _ in 0..4 {
                let report = state.advance_tick().unwrap();
                assert!(ingress.complete(&report.commands).is_empty());
                assert!(state.world.game.units[&actor].garrison.is_none());
                let cache = ww2_core::recipient_snapshot::SnapshotCache::new(&state).unwrap();
                for slot in 0..2 {
                    cache.recipient(slot).unwrap();
                }
            }
            assert_ne!(state.world.game.units[&actor].at(), at);
        }
    }

    #[test]
    fn conquest_production_cancel_uses_canonical_denial_after_enqueue() {
        let mut state = crate::lobby::native_tests::native_match();
        let mut ingress = CommanderIngress::default();
        ingress
            .enqueue(
                &mut state,
                0,
                &json!({"t":"cancelProduction","id":1,"job":1}),
            )
            .unwrap();
        assert_eq!(state.commands.pending_commands(), 1);
        let report = state.advance_tick().unwrap();
        assert_eq!(
            ingress.complete(&report.commands),
            vec![(0, "cancelProduction", Denial::Needs)]
        );
        assert_eq!(state.commands.pending_commands(), 0);
        assert!(ingress.feedback.is_empty());
    }

    #[test]
    fn compound_orders_use_one_identity_and_preserve_canonical_first_denial() {
        let mut state = crate::lobby::native_tests::native_match();
        let mut ingress = CommanderIngress::default();
        let raw = json!({"t":"orders","commands":[{"t":"fireat","ids":[],"x":10,"z":10}]});
        let before = format!("{:?}", (&state.world.game.units, &state.commands.units));
        assert!(CommanderIngress::recognizes("orders"));
        ingress.enqueue(&mut state, 0, &raw).unwrap();
        assert_eq!(state.commands.pending_commands(), 1);
        assert_eq!(
            format!("{:?}", (&state.world.game.units, &state.commands.units)),
            before
        );
        let report = state.advance_tick().unwrap();
        assert_eq!(ingress.complete(&report.commands).len(), 1);
        assert_eq!(state.commands.pending_commands(), 0);
    }

    #[test]
    fn base_commands_reserve_input_without_immediate_world_mutation() {
        let mut state = crate::lobby::native_tests::native_match();
        let mut ingress = CommanderIngress::default();
        let hq = state
            .world
            .game
            .units
            .values()
            .find(|unit| unit.owner == 0 && state.rules.kinds[unit.kind].name == "hq")
            .unwrap()
            .id;
        let before = format!(
            "{:?}",
            (
                &state.world,
                &state.start.economy,
                &state.building_rallies,
                &state.player_rallies
            )
        );
        for (index, raw) in [
            json!({"t":"cancel", "id":hq}),
            json!({"t":"rally", "id":hq, "x":30, "z":30}),
            json!({"t":"rally", "x":30, "z":30}),
        ]
        .into_iter()
        .enumerate()
        {
            assert_eq!(ingress.enqueue(&mut state, 0, &raw), Ok(()));
            assert_eq!(state.commands.pending_commands(), index + 1);
            assert_eq!(
                format!(
                    "{:?}",
                    (
                        &state.world,
                        &state.start.economy,
                        &state.building_rallies,
                        &state.player_rallies
                    )
                ),
                before
            );
        }
    }
}

#[cfg(test)]
#[path = "room_fireat_tests.rs"]
mod room_fireat_tests;
