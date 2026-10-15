//! Composed network-to-recipient witness. This does not admit physical logistics to advance_tick.
use serde_json::{Value, json};

use super::*;
use crate::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    map::AuthoritativeMap,
    match_commands::{Command, MoveOrder, Stance},
    match_runtime::{MatchOptions, PlayerConfig},
    weather::WeatherChoice,
};

fn bits(value: f64) -> Value {
    json!(format!("{:016x}", value.to_bits()))
}

fn actors(m: &Match) -> Value {
    json!(
        m.world
            .game
            .units
            .values()
            .map(|u| json!([
                u.id,
                u.owner,
                m.rules.kinds[u.kind].name,
                bits(u.hp),
                bits(u.x),
                bits(u.z)
            ]))
            .collect::<Vec<_>>()
    )
}

fn command(m: &mut Match, owner: usize, command: Command) {
    assert_eq!(m.execute_command(owner, &command, false).unwrap(), None);
}

fn selected(m: &mut Match, troop: u32) -> Value {
    let rate = m
        .supply_net
        .rate_at(Some(0), m.world.game.units[&troop].at())
        .unwrap();
    let cut_for = m.logistics.get(troop).unwrap().cut_for.unwrap();
    let cached_rate = m.logistics.get(troop).unwrap().supply_rate.unwrap();
    let owner = m
        .with_supply_world(|ctx, state, reserves, world| {
            state.recipient_state(ctx, 0, reserves, world)
        })
        .unwrap();
    let opponent = m
        .with_supply_world(|ctx, state, reserves, world| {
            state.recipient_state(ctx, 1, reserves, world)
        })
        .unwrap();
    assert!(opponent.units.iter().all(|row| row.id != troop));
    let row = owner.units.iter().find(|row| row.id == troop).unwrap();
    let grace = row.grace.map(|seconds| {
        assert!(seconds.is_finite() && seconds > 0.0 && seconds <= 10.0);
        assert_eq!(seconds.fract(), 0.0);
        seconds as u64
    });
    json!({
        "rate": bits(rate), "cutFor": bits(cut_for), "cachedRate": bits(cached_rate),
        "recipient": {"id": row.id, "supply": bits(row.supply), "cut": row.cut, "grace": grace}
    })
}

// Advance only the composed components selected by this witness. No full-runner claim.
fn component_tick(m: &mut Match, enemy: u32) {
    m.world.game.tick = m.world.game.tick.checked_add(1).unwrap();
    let tick = m.world.game.tick;
    let ids = m.world.game.units.keys().copied().collect::<Vec<_>>();
    if tick % 4 == 1 {
        crate::vision::update_vision(&mut m.world.game, &m.rules, &m.grid);
    }
    let paths = m.tick_state.path_config();
    m.begin_path_tick(paths.budget, paths.retry_limit).unwrap();
    for &id in &ids {
        m.advance_unit_reserves(id).unwrap();
    }
    m.prepare_unit_commands(enemy, false).unwrap();
    m.world.game.units.get_mut(&enemy).unwrap().repath -= m.rules.movement.tick;
    m.move_unit_with_command_routes(enemy, 1.0).unwrap();
    // The actual Source network reads the post-movement list on its twenty-tick beat.
    if tick % 20 == 0 {
        m.refresh_supply_network(&ids).unwrap();
    }
    m.step_supply_convoys(false).unwrap();
}

#[test]
fn real_enemy_move_drives_network_grace_and_owner_cut() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/log24-causal-history.json");
    let f: Value = serde_json::from_str(
        &std::fs::read_to_string(path)
            .expect("Root must authorize authentic LOG24 Source generation before this test"),
    )
    .unwrap();
    assert_eq!(f["reference"], "e4e579a838aabae3b1d9dadbc17a0a87147bc52d");
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&f["map"]).unwrap()).unwrap();
    let mut m = Match::new(
        &map,
        MatchOptions {
            seed: 701,
            mode: Mode::Conquest,
            players: vec![
                PlayerConfig {
                    name: "a".into(),
                    team: 0,
                    faction: Faction::Usa,
                },
                PlayerConfig {
                    name: "b".into(),
                    team: 1,
                    faction: Faction::Germany,
                },
            ],
            shuffle: false,
            army: ArmySize::Standard,
            weather: WeatherChoice::Map,
            weather_seed: Some(1),
            living_weather: false,
            map_key: None,
            tech: false,
            supply: true,
            logistics: true,
            sandbox: false,
        },
    )
    .unwrap();
    assert_eq!(actors(&m), f["initialActors"]);
    assert_eq!(m.world.game.units.len(), 10);
    let troop = m
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 0 && m.rules.kinds[u.kind].name == "rifle")
        .unwrap()
        .id;
    let enemy = m
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 1 && m.rules.kinds[u.kind].name == "rifle")
        .unwrap()
        .id;
    assert_eq!(json!([troop, enemy]), f["aliases"]);
    let target = Point {
        x: m.world.game.units[&troop].x,
        z: m.world.game.units[&troop].z - 6.0,
    };
    let target_cell =
        (target.z / CELL).floor() as usize * m.world.game.w + (target.x / CELL).floor() as usize;
    assert!(target_cell < m.world.game.chars.len());
    assert_eq!(m.world.game.flags[target_cell] & MOVE, 0);
    assert!(!m.building_cells.contains_key(&target_cell));
    let footprints = m
        .world
        .game
        .units
        .values()
        .filter(|u| m.rules.kinds[u.kind].building)
        .map(|u| json!([u.id, u.cells.as_ref().unwrap()]))
        .collect::<Vec<_>>();
    assert_eq!(
        json!({
            "movement": {"owner": 1, "unit": enemy, "target": [bits(target.x), bits(target.z)]},
            "targetCell": {"cell": target_cell, "ch": char::from(m.world.game.chars[target_cell]).to_string(),
                "flags": m.world.game.flags[target_cell], "footprint": false},
            "buildingFootprints": footprints,
        }),
        f["inputReady"]
    );
    let mut raw_commands = Vec::new();
    for owner in 0..2 {
        let ids = m
            .world
            .game
            .units
            .values()
            .filter(|u| u.owner == owner)
            .map(|u| i64::from(u.id))
            .collect::<Vec<_>>();
        for (stance, key, on) in [
            (Stance::HoldFire, "holdFire", true),
            (Stance::HoldPosition, "holdPos", true),
            (Stance::AutoRetreat, "autoRetreat", false),
        ] {
            raw_commands.push(
                json!({"owner": owner, "raw": {"t": "stance", "ids": ids, "key": key, "on": on}}),
            );
            command(
                &mut m,
                owner,
                Command::Stance {
                    ids: ids.clone(),
                    stance,
                    on,
                },
            );
        }
        raw_commands
            .push(json!({"owner": owner, "raw": {"t": "autocast", "ids": ids, "on": false}}));
        command(&mut m, owner, Command::Autocast { ids, on: false });
    }
    let initial = actors(&m);
    let initial_enemy = m.world.game.units[&enemy].at();
    let terrain = m.world.game.chars.clone();
    assert_eq!(selected(&mut m, troop), f["initial"]);
    assert!(
        m.supply_net
            .rate_at(Some(0), m.world.game.units[&troop].at())
            .unwrap()
            > 0.0
    );
    assert_eq!(m.logistics.get(troop).unwrap().cut_for, Some(0.0));
    raw_commands
        .push(json!({"owner": 1, "raw": {"t": "move", "orders": [[enemy, target.x, target.z]]}}));
    let source_commands = f["commands"].as_array().unwrap();
    assert_eq!(raw_commands.len(), source_commands.len());
    for (native, source) in raw_commands.iter().zip(source_commands) {
        assert_eq!(native["owner"], source["owner"]);
        assert_eq!(
            Command::from_json(&native["raw"]).unwrap(),
            Command::from_json(&source["raw"]).unwrap()
        );
    }
    command(
        &mut m,
        1,
        Command::Move {
            control_ids: None,
            orders: vec![MoveOrder {
                unit: i64::from(enemy),
                at: target,
            }],
            attack_move: false,
            queue: false,
            together: false,
            face: None,
        },
    );
    let mut first_zero = None;
    let mut checkpoints = 0;
    for _ in 0..1140 {
        component_tick(&mut m, enemy);
        let rate = m
            .supply_net
            .rate_at(Some(0), m.world.game.units[&troop].at())
            .unwrap();
        if first_zero.is_none() && rate == 0.0 {
            first_zero = Some(m.world.game.tick);
            assert!(m.logistics.get(troop).unwrap().cut_for.unwrap() > 0.0);
            let standing = m.world.game.units[&troop].at();
            let arrived = m.world.game.units[&enemy].at();
            assert_ne!(arrived, initial_enemy);
            assert!(math::hypot(arrived.x - standing.x, arrived.z - standing.z) <= 8.0);
            let cell = (standing.z / CELL).floor() as usize * m.world.game.w
                + (standing.x / CELL).floor() as usize;
            let network = m.supply_net.teams.iter().find(|net| net.team == 0).unwrap();
            assert_ne!(network.closed[cell], 0);
        }
        if let Some(start) = first_zero {
            let elapsed = m.world.game.tick - start;
            assert_eq!(rate, 0.0, "enemy must keep the real network closed");
            if elapsed == 40 || elapsed == 240 {
                let row = selected(&mut m, troop);
                let expected = if elapsed == 40 {
                    &f["afterTwoSeconds"]
                } else {
                    &f["afterTwelveSeconds"]
                };
                assert_eq!(&row, expected, "elapsed component ticks {elapsed}");
                assert_eq!(row["recipient"]["cut"], elapsed == 240);
                if elapsed == 40 {
                    assert!(row["recipient"]["grace"].as_f64().unwrap() > 0.0);
                }
                checkpoints += 1;
            }
            if elapsed == 240 {
                break;
            }
        }
    }
    assert!(
        first_zero.is_some(),
        "real Move must bring an armed enemy within closure range"
    );
    assert_eq!(
        checkpoints, 2,
        "both selected temporal observations must be produced"
    );
    assert_eq!(m.world.game.chars, terrain);
    let final_actors = actors(&m);
    assert_eq!(
        initial.as_array().unwrap().len(),
        final_actors.as_array().unwrap().len()
    );
    for (before, after) in initial
        .as_array()
        .unwrap()
        .iter()
        .zip(final_actors.as_array().unwrap())
    {
        assert_eq!(
            &before.as_array().unwrap()[..4],
            &after.as_array().unwrap()[..4]
        );
        if before[0] != enemy {
            assert_eq!(before, after);
        }
    }
    assert!(m.convoys.as_ref().unwrap().trucks.is_empty());
}
