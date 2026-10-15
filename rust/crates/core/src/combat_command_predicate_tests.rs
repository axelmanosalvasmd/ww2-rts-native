//! Command-only witnesses. Frozen observations never initialize Match state.
use super::Replay;
use crate::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    map::AuthoritativeMap,
    match_commands::Command,
    match_firing::LaunchEvent,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    match_tick::TickEvent,
    orders::OrderKey,
    weather::WeatherChoice,
};
use serde_json::{Value, json};

fn bits(value: f64) -> String {
    format!("{:016x}", value.to_bits())
}

fn snapshot(state: &Match, replay: &Replay) -> Value {
    let game = &state.world.game;
    let units = game.units.values().map(|unit| {
        let weapon = state.firing.get(unit.id).expect("canonical weapon owner");
        let control = &state.commands.units[&unit.id];
        json!({
            "id":unit.id,"owner":unit.owner,"kind":state.rules.kinds[unit.kind].name,
            "x":bits(unit.x),"z":bits(unit.z),"hp":bits(unit.hp),
            "target":weapon.target_id,"cooldown":bits(weapon.cooldown),
            "retarget":bits(weapon.retarget),"attack":control.attack_id,
            "shotAt":unit.shot_at.map(bits),
            "visible":game.players.iter().map(|player|player.sees(unit.id)).collect::<Vec<_>>()
        })
    }).collect::<Vec<_>>();
    json!({
        "tick":game.tick,"randomConsumed":replay.receipt()["consumed"],"units":units,
        "environment":{
            "seed32":state.start.environment_seed,
            "wind":[bits(state.start.wind.0),bits(state.start.wind.1)],
            "shower":state.start.living.map(|living|json!({
                "raining":living.raining,"next":bits(living.next)
            })),
            "rain":bits(game.rain),"wet":game.wet.map(bits)
        }
    })
}

fn compare_snapshot(state: &Match, replay: &Replay, expected: &Value) {
    let actual = snapshot(state, replay);
    if &actual != expected {
        eprintln!("actual input receipt at first observation mismatch: {}", replay.receipt());
    }
    assert_eq!(&actual, expected, "command scene tick {}", state.world.game.tick);
}

fn trace(variable: &str, scene: &str) -> Value {
    let path = std::env::var_os(variable).expect("new frozen Source trace required");
    let bytes = std::fs::read(path).unwrap();
    let trace: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(trace["format"], "combat-command-predicates-v1");
    assert_eq!(trace["reference"], "e4e579a838aabae3b1d9dadbc17a0a87147bc52d");
    assert_eq!(trace["node"], "v24.21.0");
    assert_eq!(trace["arch"], "x64");
    assert_eq!(trace["seed"], "701");
    assert_eq!(trace["sharedInput"]["seed"], trace["seed"]);
    assert_eq!(trace["sharedInput"]["layout"], json!([0, 1]));
    assert_eq!(trace["scene"], scene);
    assert_eq!(trace["instrumentation"]["erasesExactly"], true);
    assert!(trace["failure"].is_null(), "failed Source capture is never proof");
    assert_eq!(trace["completed"], 1000);
    assert_eq!(trace["frames"].as_array().unwrap().len(), 1001);
    trace
}

fn key(row: &Value) -> OrderKey {
    OrderKey {
        tick: row["tick"].as_u64().unwrap(),
        issuer: u32::try_from(row["issuer"].as_u64().unwrap()).unwrap(),
        sequence: row["sequence"].as_u64().unwrap(),
    }
}

fn id(value: &Value) -> u32 {
    u32::try_from(value.as_u64().unwrap()).unwrap()
}

fn run(trace: &Value, ready: bool) {
    let replay = Replay::new(&trace["sharedInput"]).unwrap();
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&trace["map"]).unwrap()).unwrap();
    let mut state = Match::new_aligned(
        &map,
        MatchOptions {
            seed: trace["seed"].as_str().unwrap().parse().unwrap(),
            mode: Mode::Conquest,
            players: vec![
                PlayerConfig { name: "A".into(), team: 0, faction: Faction::Usa },
                PlayerConfig { name: "B".into(), team: 1, faction: Faction::Germany },
            ],
            shuffle: true,
            army: ArmySize::Standard,
            weather: WeatherChoice::from_reference(Some("clear")),
            weather_seed: None,
            living_weather: true,
            map_key: None,
            tech: true,
            supply: true,
            logistics: false,
            sandbox: false,
        },
        replay.clone(),
    ).unwrap();
    compare_snapshot(&state, &replay, &trace["frames"][0]);
    let orders = trace["orders"].as_array().unwrap();
    let denials = trace["denials"].as_array().unwrap();
    assert_eq!(orders.len(), denials.len());
    assert!(orders.windows(2).all(|pair| key(&pair[0]) < key(&pair[1])));
    for (order, denial) in orders.iter().zip(denials) {
        assert_eq!(key(order), key(denial));
        assert_eq!(order["command"], denial["command"]);
        assert!((1..=1000).contains(&key(order).tick));
    }
    let invalidated = trace["witness"]["invalidated"].as_u64().unwrap();
    let old = trace["witness"]["oldTarget"].as_u64().map(|_| id(&trace["witness"]["oldTarget"]));
    let next = trace["witness"]["replacement"].as_u64().map(|_| id(&trace["witness"]["replacement"]));
    let medic = trace["witness"]["medic"].as_u64().map(|_| id(&trace["witness"]["medic"]));
    let mut consumed = 0;
    let mut replacement_health = None;
    let mut paid = None;
    let mut arrivals = 0;
    for tick in 1..=1000_u64 {
        if ready && tick == invalidated {
            let weapon = state.firing.get(1).unwrap();
            assert_eq!(weapon.target_id, old.unwrap());
            assert!(weapon.cooldown > 0.0 && weapon.cooldown <= 0.05,
                "natural reload completes at this tick's ordinary timer decrement");
            assert!(weapon.retarget > 0.05, "invalidation precedes scheduled search");
            let target = &state.world.game.units[&old.unwrap()];
            let fresh = &state.world.game.units[&next.unwrap()];
            assert!(target.hp > 0.0 && target.hp < fresh.hp, "earlier physical volley damage");
            replacement_health = Some(fresh.hp);
        }
        let rows = orders.iter().filter(|row| key(row).tick == tick).collect::<Vec<_>>();
        assert_eq!(state.commands.pending_commands(), 0);
        for row in &rows {
            let command = Command::from_json(&row["command"]).unwrap();
            assert!(matches!(row["command"]["t"].as_str(),
                Some("move" | "stance" | "autocast" | "destroy" | "buy" | "stop" | "attack")));
            if row["command"]["t"] == "destroy" {
                assert!(ready && tick == invalidated);
                assert_eq!(row["issuer"], 1);
                assert_eq!(row["command"]["ids"], json!([old.unwrap()]));
            }
            if !ready && row["command"]["t"] == "buy" {
                let maker = id(&row["command"]["from"]);
                let unit = &state.world.game.units[&maker];
                assert_eq!(unit.owner, 0);
                assert_eq!(state.rules.kinds[unit.kind].name, "barracks");
                assert_eq!(unit.built, Some(1.0));
                let purchase = &trace["witness"]["purchase"];
                assert_eq!(purchase["maker"], maker);
                assert_eq!(purchase["cost"], 120);
                assert_eq!(purchase["before"], bits(state.start.economy.players[0].mp));
                paid = Some((maker, state.story[0].mp_spent));
            }
            if !ready && row["command"]["t"] == "attack" {
                let medic = medic.expect("purchased medic identity");
                assert_eq!(state.rules.kinds[state.world.game.units[&medic].kind].name, "medic");
                assert_eq!(row["command"]["ids"], json!([medic]));
                assert_eq!(row["command"]["target"], 4);
                assert!(tick == 3 || tick == 700);
                assert_eq!(state.world.game.players[0].sees(4), tick == 700,
                    "visibility comes from complete simulation ticks");
            }
            state.enqueue_command(key(row), command).unwrap();
        }
        let report = state.advance_tick().unwrap();
        assert_eq!(u64::try_from(report.tick).unwrap(), tick);
        assert_eq!(report.commands.len(), rows.len());
        for (result, row) in report.commands.iter().zip(&rows) {
            let expected = &denials[consumed];
            assert_eq!(result.key, key(row));
            assert_eq!(result.key, key(expected));
            assert_eq!(json!(result.denial.map(|reason| reason.name())), expected["reason"]);
            consumed += 1;
        }
        assert_eq!(state.commands.pending_commands(), 0);
        compare_snapshot(&state, &replay, &trace["frames"][tick as usize]);
        if !ready && tick == 2 {
            let (maker, before_spent) = paid.unwrap();
            let medic = medic.unwrap();
            assert_eq!(state.world.game.units[&medic].owner, 0);
            assert_eq!(state.rules.kinds[state.world.game.units[&medic].kind].name, "medic");
            assert_eq!(state.recruit_metadata[&medic].building, maker);
            assert_eq!(state.story[0].mp_spent - before_spent, 120.0);
            let purchase = &trace["witness"]["purchase"];
            let before = f64::from_bits(u64::from_str_radix(purchase["before"].as_str().unwrap(), 16).unwrap());
            let after = f64::from_bits(u64::from_str_radix(purchase["after"].as_str().unwrap(), 16).unwrap());
            assert_eq!(before - after, 120.0, "Source paid the same price before income");
        }
        if ready && tick == invalidated {
            assert_eq!(state.firing.get(1).unwrap().target_id, next.unwrap());
            assert_eq!(state.world.game.units[&1].shot_at, Some(tick as f64));
            assert!(report.events.iter().any(|event| matches!(event,
                TickEvent::Firing(LaunchEvent::Flight { shooter: 1, time, .. })
                    if *time == tick as f64 * state.rules.movement.tick)),
                "physical replacement launch on acquisition tick");
            assert!(state.world.game.units.get(&old.unwrap()).is_none_or(|unit| unit.hp <= 0.0));
        }
        if ready && tick == invalidated + 4 {
            let hp = state.world.game.units.get(&next.unwrap()).map_or(0.0, |unit| unit.hp);
            assert!(hp < replacement_health.unwrap(), "acquisition volley damage within 0.2 seconds");
            assert_eq!(state.world.game.units[&1].shot_at, Some(invalidated as f64));
            assert_eq!(trace["witness"]["arrivalTick"], tick);
            arrivals += 1;
        }
    }
    assert_eq!(consumed, orders.len(), "every command consumed exactly once");
    if ready {
        assert!(invalidated > trace["witness"]["firstShot"].as_u64().unwrap());
        assert_eq!(arrivals, 1);
    } else {
        let attacks = denials.iter().filter(|row| row["command"]["t"] == "attack").collect::<Vec<_>>();
        assert_eq!(attacks.len(), 2);
        assert_eq!(attacks[0]["reason"], "unseen");
        assert_eq!(attacks[1]["reason"], "queueFull");
        assert!(paid.is_some());
    }
    let receipt = replay.receipt();
    println!("actual native input receipt: {receipt}");
    let source = &trace["inputReceipt"];
    for field in ["capacity", "consumed", "unconsumed", "exhausted", "draws", "startup"] {
        assert_eq!(receipt[field], source[field], "actual shared input receipt {field}");
    }
    assert_eq!(receipt["layout"], source["layout"]);
}

#[test]
#[ignore = "requires new frozen Source capture and an exact-head execution grant"]
fn ready_weapon_replaces_real_destroyed_target_and_launches_on_acquisition_tick() {
    run(&trace("WW2_COMBAT_READY_TRACE", "ready-replacement"), true);
}

#[test]
#[ignore = "requires new frozen Source capture and an exact-head execution grant"]
fn purchased_medic_attack_uses_actual_unseen_and_visible_producers() {
    run(&trace("WW2_COMBAT_MEDIC_TRACE", "medic-visibility"), false);
}
