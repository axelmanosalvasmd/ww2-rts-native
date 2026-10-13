use super::*;
use crate::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    map::AuthoritativeMap,
    match_commands::{Command, Denial},
    match_runtime::{Match, MatchOptions, PlayerConfig},
    orders::OrderKey,
    weather::WeatherChoice,
};
use serde_json::{Value, json};

fn make() -> Match {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../simtest/tests/fixtures/match-recruitment.json"
    ))
    .unwrap();
    let map =
        AuthoritativeMap::from_json(&serde_json::to_vec(&fixture["cases"][0]["map"]).unwrap())
            .unwrap();
    Match::new(
        &map,
        MatchOptions {
            seed: 701,
            mode: Mode::Conquest,
            players: vec![
                PlayerConfig {
                    name: "A".into(),
                    team: 0,
                    faction: Faction::Usa,
                },
                PlayerConfig {
                    name: "B".into(),
                    team: 1,
                    faction: Faction::Germany,
                },
            ],
            shuffle: false,
            army: ArmySize::Standard,
            weather: WeatherChoice::Map,
            weather_seed: Some(217),
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
fn input(seq: u64) -> Value {
    json!({"seq":seq,"forward":0.5,"strafe":0,"yaw":0.2,"pitch":0,
        "fire":false,"jump":0,"reload":0,"nade":0,"squad":0})
}
fn guard(m: &Match) -> Value {
    json!({"units":m.world.game.units.values().map(|u|(u.id,u.hp)).collect::<Vec<_>>(),
        "next":m.next_unit_id,"wallets":m.start.economy.players.iter().map(|p|p.mp).collect::<Vec<_>>(),
        "companions":m.companions.records.values().map(|o|(o.id,o.owner,o.unit_id,o.respawn_at)).collect::<Vec<_>>(),
        "controls":m.commands.units.keys().collect::<Vec<_>>(),
        "firing":m.firing.units.keys().collect::<Vec<_>>(),
        "combat":m.combat.iter().map(|(id,_)|id).collect::<Vec<_>>(),
        "abilities":m.abilities.iter().collect::<Vec<_>>().len(),
        "recovery":m.recovery.iter().collect::<Vec<_>>().len()})
}
#[test]
fn journal_binding_and_raw_buy_validation() {
    assert!(CompanionJournalId::new(0).is_none());
    let id = CompanionJournalId::new(u64::MAX).unwrap();
    assert_eq!(id.get(), u64::MAX);
    assert_eq!(id.registry_key(), "journal-companion:18446744073709551615");
    for kind in ["companionJoin", "companionInput", "join", "fps"] {
        assert_eq!(
            Command::from_json(&json!({"t":kind,"identity":7})),
            Ok(Command::Unsupported)
        );
    }
    for from in [
        Value::Null,
        json!("1"),
        json!(-1),
        json!(1.5),
        json!(4294967296u64),
        json!(0),
    ] {
        assert_eq!(
            Command::from_json(&json!({"t":"buy","unit":"rifle","from":from})),
            Ok(Command::Buy {
                kind: "rifle".into(),
                from: Some(0)
            })
        );
    }
    assert_eq!(
        Command::from_json(&json!({"t":"buy","unit":"rifle"})),
        Ok(Command::Buy {
            kind: "rifle".into(),
            from: None
        })
    );
    assert_eq!(
        Command::from_json(&json!({"t":"buy","unit":"rifle","orders":[[1,2]]})),
        Err(Denial::Blocked)
    );
}
#[test]
fn trusted_fps_uses_one_queue_and_registry_with_source_gates() {
    let mut m = make();
    let first = CompanionJournalId::new(7).unwrap();
    let second = CompanionJournalId::new(3).unwrap();
    m.start.economy.players[0].away = true;
    let commands = [
        Command::CompanionJoin { identity: first },
        Command::CompanionInput {
            identity: first,
            input: input(4),
        },
        Command::CompanionJoin { identity: second },
    ];
    // Arrival order differs from dispatch order, including nonlexicographic keys.
    for index in [2, 0, 1] {
        m.enqueue_command(
            OrderKey {
                tick: 1,
                issuer: 0,
                sequence: index as u64,
            },
            commands[index].clone(),
        )
        .unwrap();
    }
    assert!(
        m.apply_commands(1, false)
            .unwrap()
            .iter()
            .all(|r| r.denial.is_none())
    );
    let op = m.companions.get(&first.registry_key()).unwrap();
    assert_eq!(op.id, 1);
    assert_eq!(op.owner, 0);
    assert_eq!(op.last_seq, 4);
    let soldier = op.unit_id;
    assert_eq!(m.companions.get(&second.registry_key()).unwrap().id, 2);
    assert_eq!(m.abilities.get(soldier), Some(&Default::default()));
    assert_eq!(m.recovery.get(soldier), Some(&Default::default()));
    // A following ordinary ability validates every newly published row.
    assert_eq!(
        m.execute_command(
            1,
            &Command::Ability {
                ids: vec![],
                at: None,
                queue: false,
                automatic: false
            },
            false
        )
        .unwrap(),
        Some(Denial::Needs)
    );
    m.execute_command(
        1,
        &Command::CompanionInput {
            identity: first,
            input: input(99),
        },
        false,
    )
    .unwrap();
    m.execute_command(1, &Command::CompanionJoin { identity: first }, false)
        .unwrap();
    assert_eq!(m.companions.get(&first.registry_key()).unwrap().last_seq, 4);
    assert_eq!(m.companions.get(&first.registry_key()).unwrap().owner, 0);
    m.execute_command(
        0,
        &Command::CompanionInput {
            identity: first,
            input: input(4),
        },
        false,
    )
    .unwrap();
    assert_eq!(m.companions.get(&first.registry_key()).unwrap().last_seq, 4);
}
#[test]
fn fresh_companion_metadata_preflight_covers_join_and_entire_respawn_batch() {
    for ability in [true, false] {
        let mut m = make();
        let fresh = m.next_unit_id;
        if ability {
            m.abilities.initialize(fresh);
        } else {
            m.recovery.initialize(fresh);
        }
        let before = guard(&m);
        assert!(
            m.join_companion("journal-companion:7", 0, false)
                .unwrap_err()
                .contains("metadata identity")
        );
        assert_eq!(guard(&m), before);
        m.abilities.remove(fresh);
        m.recovery.remove(fresh);
        let first = m
            .join_companion("journal-companion:7", 0, false)
            .unwrap()
            .unwrap();
        let second = m
            .join_companion("journal-companion:3", 0, false)
            .unwrap()
            .unwrap();
        for id in [first, second] {
            m.world.game.units.get_mut(&id).unwrap().hp = 0.;
        }
        m.prepare_companion_deaths(&[first, second]).unwrap();
        for id in [first, second] {
            assert!(m.companion_died(id).unwrap());
            m.world.game.units.remove(&id);
            m.commands.units.remove(&id);
            m.firing.remove(id);
            m.combat.remove(id);
            m.abilities.remove(id);
            m.recovery.remove(id);
        }
        m.world.game.tick = m.companions.get("journal-companion:7").unwrap().respawn_at;
        m.start.economy.players[0].mp = 100.;
        let last = m.next_unit_id + 1;
        if ability {
            m.abilities.initialize(last);
        } else {
            m.recovery.initialize(last);
        }
        let before = guard(&m);
        assert!(
            m.respawn_companions(false)
                .unwrap_err()
                .contains("metadata identity")
        );
        assert_eq!(
            guard(&m),
            before,
            "late stale metadata must not publish an earlier draft or spend"
        );
    }
}
