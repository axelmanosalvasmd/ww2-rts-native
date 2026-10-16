use super::{tests::*, *};
use crate::{
    commander_ai::Input,
    economy_state::ArmySize,
    map::AuthoritativeMap,
    match_commands::Command,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    recipient_snapshot::{DeliveryState, SelectedSeat},
    weather::WeatherChoice,
};

fn fire_at_commands(commands: &[Value]) -> Vec<Value> {
    commands
        .iter()
        .filter(|c| c["t"] == "fireat")
        .cloned()
        .collect()
}

#[test]
fn ai38_medics_follow_the_ground_army_center_beyond_twenty_four_meters() {
    // shared/ai.js:593,673: the medic itself counts in the ground army mean.
    for distance in [23.9, 24.0, 24.1] {
        let mut input = fixture(vec![
            unit(1, "medic", 20.0),
            unit(2, "rifle", 20.0 + 2.0 * distance),
            unit(3, "engineer", 155.0),
            unit(4, "hq", 155.0),
            unit(5, "fighter", 155.0),
        ]);
        let mut ally = unit(6, "rifle", 155.0);
        ally.owner = 2;
        input.units.push(ally);
        let commands = run(input.clone(), &mut State::default(), Level::Normal);
        let movement: Vec<_> = commands
            .iter()
            .filter(|c| c["t"] == "move")
            .flat_map(|c| c["orders"].as_array().unwrap())
            .filter(|r| r[0] == 1)
            .collect();
        assert_eq!(movement.len(), usize::from(distance > 24.0), "{distance}");
        if distance > 24.0 {
            assert_eq!(*movement[0], json!([1, 20.0 + distance, 20.0]));
        }
        input.units[0].path = vec![Point { x: 70.0, z: 20.0 }];
        let commands = run(input, &mut State::default(), Level::Normal);
        assert!(!acts(&commands, "move", 1));
        assert!(!acts(&commands, "amove", 1));
    }
}

fn house_input(distance: f64) -> Input {
    let mut enemy = unit(10, "rifle", 40.0 + distance);
    enemy.owner = 1;
    enemy.garrison = true;
    fixture(vec![unit(1, "tank", 40.0), enemy])
}

#[test]
fn ai38_house_shelling_requires_a_watched_garrison_strictly_within_sixty_meters() {
    // shared/ai.js:675-678: use delivered contacts after the notice delay.
    for (distance, garrison, owner, sheller, expected) in [
        (59.9, true, 1, "tank", true),
        (60.0, true, 1, "tank", false),
        (60.1, true, 1, "tank", false),
        (40.0, false, 1, "tank", false),
        (40.0, true, 2, "tank", false),
        (40.0, true, 1, "rifle", false),
    ] {
        let mut input = house_input(distance);
        input.units[0] = unit(1, sheller, 40.0);
        input.units[1].garrison = garrison;
        input.units[1].owner = owner;
        let mut state = State::default();
        assert!(fire_at_commands(&run(input.clone(), &mut state, Level::Normal)).is_empty());
        input.tick += 24;
        assert!(fire_at_commands(&run(input.clone(), &mut state, Level::Normal)).is_empty());
        input.tick += 1;
        let commands = fire_at_commands(&run(input, &mut state, Level::Normal));
        assert_eq!(
            commands,
            if expected {
                vec![json!({"t":"fireat","ids":[1],"x":40.0 + distance,"z":20.0})]
            } else {
                vec![]
            },
            "{distance}, {garrison}, {owner}, {sheller}"
        );
    }
}

#[test]
fn ai38_house_shelling_uses_contact_id_order_and_keeps_the_prototype_chore_precedence() {
    let mut input = house_input(58.0);
    let mut nearer = unit(20, "rifle", 80.0);
    nearer.owner = 1;
    nearer.garrison = true;
    input.units.push(nearer);
    // Chores precede the generic busy guard. A route can be replaced by house
    // shelling, but an active target or FireAt cannot.
    input.units[0].path = vec![Point { x: 120.0, z: 20.0 }];
    for busy in ["path", "target", "fireat"] {
        let mut input = input.clone();
        input.units[0].target_id = if busy == "target" { 10 } else { 0 };
        input.units[0].fire_at = busy == "fireat";
        let mut state = State::default();
        run(input.clone(), &mut state, Level::Normal);
        input.tick += 25;
        let commands = fire_at_commands(&run(input, &mut state, Level::Normal));
        assert_eq!(
            commands,
            if busy == "path" {
                vec![json!({"t":"fireat","ids":[1],"x":98.0,"z":20.0})]
            } else {
                vec![]
            },
            "{busy}"
        );
    }
}

#[test]
fn ai38_busy_fighters_keep_their_existing_work() {
    for busy in ["path", "attack", "nade", "dig", "enter", "fireat", "target"] {
        let mut u = unit(1, "rifle", 40.0);
        match busy {
            "path" => u.path = vec![Point { x: 60.0, z: 20.0 }],
            "attack" => u.attack_id = true,
            "nade" => u.nade = true,
            "dig" => u.dig = true,
            "enter" => u.enter = true,
            "fireat" => u.fire_at = true,
            "target" => u.target_id = 2,
            _ => unreachable!(),
        }
        let commands = run(fixture(vec![u]), &mut State::default(), Level::Normal);
        assert!(!acts(&commands, "move", 1), "{busy}");
        assert!(!acts(&commands, "amove", 1), "{busy}");
        assert!(!acts(&commands, "fireat", 1), "{busy}");
    }
}

fn ordinary_chores_match() -> (Match, u32, u32) {
    let mut rows = vec![".".repeat(80); 80];
    rows[20].replace_range(20..24, "BBBB");
    rows[21].replace_range(20..24, "BBBB");
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"Commander chores", "w":80,"h":80,"rows":rows,
            "spawns":[{"x":4,"y":4},{"x":75,"y":75}],
            "points":[{"x":40,"y":40}]
        }))
        .unwrap(),
    )
    .unwrap();
    let mut m = Match::new(
        &map,
        MatchOptions {
            seed: 1804,
            mode: Mode::Conquest,
            players: (0..2)
                .map(|team| PlayerConfig {
                    name: format!("P{team}"),
                    team,
                    faction: Faction::Usa,
                })
                .collect(),
            shuffle: false,
            army: ArmySize::Endless,
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
    let owned: Vec<_> = m
        .world
        .game
        .units
        .values()
        .filter(|u| u.owner == 0 && m.rules.kinds[u.kind].name == "rifle")
        .map(|u| u.id)
        .collect();
    let (tank, medic) = (owned[0], owned[1]);
    for (&id, u) in &mut m.world.game.units {
        if u.owner == 0 && !m.rules.kinds[u.kind].structure {
            u.x = 31.0;
            u.z = 41.0;
        }
        u.path.clear();
        u.hold_pos = true;
        let c = m.commands.units.get_mut(&id).unwrap();
        c.hold_fire = true;
        c.auto_retreat = false;
    }
    for (id, kind, x) in [(tank, "tank", 31.0), (medic, "medic", 130.0)] {
        let d = Catalog::compiled().unwrap().unit(kind).unwrap();
        let u = m.world.game.units.get_mut(&id).unwrap();
        u.kind = m.rules.kind(kind).unwrap();
        u.hp = full(d);
        u.x = x;
        u.hold_pos = false;
    }
    let enemy = m
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 1 && m.rules.kinds[u.kind].name == "rifle")
        .unwrap()
        .id;
    let u = m.world.game.units.get_mut(&enemy).unwrap();
    u.x = 41.0;
    u.z = 41.0;
    u.garrison = Some(20 * 80 + 20);
    m.grid.rebuild(&m.world.game.units);
    (m, tank, medic)
}

#[test]
fn ai38_planned_chores_survive_ordinary_ticks_all_seat_snapshots_and_match_finish() {
    let (mut m, tank, medic) = ordinary_chores_match();
    let mut delivery = DeliveryState::new(&m).unwrap();
    let selected: Vec<_> = (0..m.world.game.players.len())
        .map(|slot| SelectedSeat {
            slot,
            full: false,
            omit: false,
        })
        .collect();
    let mut state = State::default();
    let mut random = MatchRandom::new(m.options.seed);
    let mut issued = false;
    let mut launched = false;
    let initial_medic_x = m.world.game.units[&medic].x;
    for _ in 0..600 {
        let report = m.advance_tick().unwrap();
        launched |= report.events.iter().any(|e| matches!(e,
            crate::match_tick::TickEvent::Firing(crate::match_firing::LaunchEvent::Flight { shooter, .. }) if *shooter == tank));
        {
            let frame = delivery
                .capture_selected_with_tick_events(&mut m, &selected, &report.events)
                .unwrap()
                .unwrap();
            for seat in &selected {
                assert_eq!(frame.recipient(seat.slot).unwrap().into_json()["t"], "s");
            }
        }
        if !issued && m.world.game.tick % 2 == 0 {
            let observation = m.opening_commander_observation(0).unwrap();
            let commands = plan(&observation, &[], &mut state, Level::Normal, &mut random).unwrap();
            if !fire_at_commands(&commands).is_empty() {
                assert!(acts(&commands, "move", medic));
                for raw in commands
                    .iter()
                    .filter(|c| c["t"] == "move" || c["t"] == "fireat")
                {
                    assert_eq!(
                        m.execute_command(0, &Command::from_json(raw).unwrap(), false)
                            .unwrap(),
                        None
                    );
                }
                assert!(m.commands.units[&tank].fire_at.is_some());
                assert!(!m.world.game.units[&medic].path.is_empty());
                issued = true;
            }
        }
        if issued && launched && m.world.game.tick >= 240 {
            m.start.economy.players[0].vp = m.start.win_vp;
        }
        if m.finish.is_some() {
            break;
        }
    }
    assert!(
        issued,
        "the delivered garrison must cause the planner to issue FireAt"
    );
    assert!(
        launched,
        "the planned FireAt must reach the ordinary firing phase"
    );
    assert!(m.world.game.units[&medic].x < initial_medic_x - 1.0);
    assert!(m.finish.is_some());
    assert_eq!(m.tick_state.fault(), None);
}
