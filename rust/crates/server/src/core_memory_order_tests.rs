use super::*;
use serde_json::json;
use ww2_core::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::Point,
    map::AuthoritativeMap,
    match_runtime::{MatchOptions, PlayerConfig},
    weather::WeatherChoice,
};

fn scene(mode: Mode) -> (Match, Value) {
    let raw = json!({"name":"Memory and recipients","w":96,"h":96,
        "rows":vec![".".repeat(96);96],
        "spawns":[{"x":5,"y":5},{"x":88,"y":88},{"x":5,"y":88}],
        "points":[{"id":"front","x":48,"y":48}],
        "scenario":{"version":1,"groups":[],"areas":[],"objectives":[
            {"id":"allied","text":"Allied mission","recipients":"team","side":0}],
            "triggers":[{"id":"intro","scope":"match","side":0,"recipients":"team",
                "condition":{"kind":"time","seconds":0.5},"actions":[
                {"kind":"objectiveActivate","objective":"allied"},
                {"kind":"say","text":"Allied announcement"}]}]}});
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&raw).unwrap()).unwrap();
    let options = MatchOptions {
        seed: 100,
        mode,
        players: [0, 1, 0]
            .into_iter()
            .enumerate()
            .map(|(slot, team)| PlayerConfig {
                name: format!("p{slot}"),
                team,
                faction: Faction::Usa,
            })
            .collect(),
        shuffle: false,
        army: ArmySize::Standard,
        weather: WeatherChoice::Map,
        weather_seed: Some(1),
        living_weather: false,
        map_key: Some("memory".into()),
        tech: false,
        supply: false,
        logistics: false,
        sandbox: false,
    };
    let mut state = if mode == Mode::Classic {
        Match::new_classic(&map, options).unwrap()
    } else {
        Match::new(&map, options).unwrap()
    };
    for control in state.commands.units.values_mut() {
        control.hold_fire = true;
        control.auto_cast = false;
        control.auto_retreat = false;
    }
    (state, raw)
}
fn seats() -> Vec<SelectedSeat> {
    (0..3)
        .map(|slot| SelectedSeat {
            slot,
            full: true,
            omit: false,
        })
        .collect()
}
fn read(runtime: &mut RoomRuntime, kind: FrameKind) -> Vec<Value> {
    runtime
        .collect_frame(&seats(), kind)
        .unwrap()
        .into_iter()
        .map(|s| s.message)
        .collect()
}
fn advance(runtime: &mut RoomRuntime, count: usize) -> Vec<Value> {
    let mut result = Vec::new();
    for _ in 0..count {
        runtime.advance_tick(&[false; 3]).unwrap();
        result = read(runtime, FrameKind::Scheduled);
        assert!(runtime.fault().is_none());
    }
    result
}
fn move_unit(state: &mut Match, id: u32, at: Point) {
    let unit = state.world.game.units.get_mut(&id).unwrap();
    unit.x = at.x;
    unit.z = at.z;
    unit.hold_pos = true;
    state.grid.rebuild(&state.world.game.units);
}
fn cell_row(message: &Value, cell: usize) -> Option<Value> {
    message["cells"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r[0] == cell)
        .cloned()
}

#[test]
fn map100_ordinary_reconnect_rescout_and_hidden_counterfactual_keep_delivered_memory() {
    let (state, raw) = scene(Mode::Conquest);
    let startup = crate::native_startup::NativeStartup::prepare_loaded(
        &state,
        crate::native_startup::StartupSettings {
            map: "memory",
            mode: "conquest",
            army: "standard",
            weather: "map",
            sandbox: false,
        },
        &raw,
    )
    .unwrap();
    let mut room = RoomRuntime::new(state, 0).unwrap();
    let names = vec!["p0".into(), "p1".into(), "p2".into()];
    let initial = room
        .collect_starts(&[0, 1, 2], &startup, &names, 41)
        .unwrap();
    let scout = room
        .state()
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 2 && room.state().rules.kinds[u.kind].name == "rifle")
        .unwrap()
        .id;
    let cell = 35 * 96 + 35;
    let never_seen = 5 * 96 + 70;
    assert!(cell_row(&initial[0].1, cell).is_none());
    move_unit(room.state_mut(), scout, Point { x: 71., z: 71. });
    room.state_mut().world.set_cell(cell, b'D').unwrap();
    room.advance_tick(&[false; 3]).unwrap();
    let incremental_seats = seats()
        .into_iter()
        .map(|mut seat| {
            seat.full = false;
            seat
        })
        .collect::<Vec<_>>();
    let incremental = room
        .collect_frame(&incremental_seats, FrameKind::Scheduled)
        .unwrap();
    assert_eq!(cell_row(&incremental[0].message, cell).unwrap()[1], "D");
    let seen = advance(&mut room, 4);
    let remembered = cell_row(&seen[0], cell).expect("scouting delivers the changed road");
    assert_eq!(remembered[1], "D");
    assert_eq!(cell_row(&seen[2], cell), Some(remembered.clone()));
    move_unit(room.state_mut(), scout, Point { x: 11., z: 177. });
    advance(&mut room, 4);
    let saved = room.state().save_json().unwrap();
    let mut unchanged = RoomRuntime::new(Match::from_save_json(saved.clone()).unwrap(), 0).unwrap();
    let mut hidden = RoomRuntime::new(Match::from_save_json(saved).unwrap(), 0).unwrap();
    hidden
        .state_mut()
        .world
        .mutate_cell(
            cell,
            ww2_core::world::CellChange {
                ground: Some(b'+'),
                height: Some(2),
                ..Default::default()
            },
        )
        .unwrap();
    hidden.state_mut().world.set_cell(never_seen, b'B').unwrap();
    let hidden_enemy = hidden
        .state()
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 1 && hidden.state().rules.kinds[u.kind].name == "rifle")
        .unwrap()
        .id;
    assert!(!hidden.state().world.game.players[0].sees(hidden_enemy));
    assert!(!hidden.state().world.game.players[2].sees(hidden_enemy));
    hidden
        .state_mut()
        .world
        .game
        .units
        .get_mut(&hidden_enemy)
        .unwrap()
        .hp *= 0.5;
    assert_eq!(hidden.state().world.game.chars[cell], b'+');
    for _ in 0..4 {
        let a = advance(&mut unchanged, 1);
        let b = advance(&mut hidden, 1);
        for slot in [0, 2] {
            assert_eq!(a[slot], b[slot], "hidden terrain changes nothing delivered");
            assert_eq!(cell_row(&b[slot], cell), Some(remembered.clone()));
            assert!(cell_row(&b[slot], never_seen).is_none());
        }
    }
    let reconnect = hidden
        .collect_starts(&[0, 2], &startup, &names, 41)
        .unwrap();
    let control = unchanged
        .collect_starts(&[0, 2], &startup, &names, 41)
        .unwrap();
    assert_eq!(
        reconnect, control,
        "complete reconnect envelopes must match"
    );
    for (_, start) in reconnect {
        assert_eq!(start["t"], "start");
        assert_eq!(start["matchId"], 41);
        assert_eq!(cell_row(&start, cell), Some(remembered.clone()));
        assert!(cell_row(&start, never_seen).is_none());
        assert!(start["fog"]["e"].is_string());
        // Ordinary authored maps are public; remembered differences arrive in cells.
        assert_eq!(
            start["map"]["rows"][35].as_str().unwrap().as_bytes()[35],
            b'.'
        );
    }
    move_unit(hidden.state_mut(), scout, Point { x: 71., z: 71. });
    let rescouted = advance(&mut hidden, 4);
    for slot in [0, 2] {
        let current = cell_row(&rescouted[slot], cell).unwrap();
        assert_eq!(current[1], "+");
        assert_eq!(current[2], 2);
        assert_ne!(current, remembered);
        assert!(cell_row(&rescouted[slot], never_seen).is_none());
    }
    advance(&mut hidden, 3);
}

#[test]
fn net55_classic_nonempty_team_ghosts_compose_with_mission_recipients_through_finish() {
    let (mut state, _) = scene(Mode::Classic);
    let scout = state
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 2 && !state.rules.kinds[u.kind].structure)
        .unwrap()
        .id;
    let building = state
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 1 && state.rules.kinds[u.kind].name == "hq")
        .unwrap()
        .id;
    let enemy_at = state.world.game.units[&building].at();
    let scout_at = Point {
        x: enemy_at.x - 10.,
        z: enemy_at.z,
    };
    state.world.game.units.get_mut(&building).unwrap().built = Some(0.25);
    move_unit(&mut state, scout, scout_at);
    let mut room = RoomRuntime::new(state, 0).unwrap();
    let visible = advance(&mut room, 5);
    for slot in [0, 2] {
        assert!(
            visible[slot]["units"]
                .as_array()
                .unwrap()
                .iter()
                .any(|r| r[0] == building)
        );
        assert!(
            !visible[slot]["ghosts"]
                .as_array()
                .unwrap()
                .iter()
                .any(|r| r[0] == building)
        );
    }
    let enemy = room.state().world.game.units[&building].clone();
    let ghost = json!([building, "hq", 1, enemy.x, enemy.z, 0.3]);
    move_unit(room.state_mut(), scout, Point { x: 11., z: 177. });
    let before = advance(&mut room, 4);
    for slot in [0, 2] {
        assert_eq!(before[slot]["ghosts"], json!([ghost.clone()]));
    }
    room.state_mut()
        .world
        .game
        .units
        .get_mut(&building)
        .unwrap()
        .built = Some(0.75);
    let messages = advance(&mut room, 1);
    let objective = json!({"id":"allied","scope":"match","text":"Allied mission","state":"active",
        "recipients":"team","side":0,"updated":0.5,"execution":"scenario:1"});
    let shot = json!({"k":"say","text":"Allied announcement","localized":"Allied announcement",
        "scenarioRecipients":"team","scenarioSide":0,"pub":false,"execution":"scenario:1","atTick":10});
    for (slot, message) in messages.iter().enumerate() {
        assert_eq!(
            message["ghosts"],
            if slot == 1 {
                json!([])
            } else {
                json!([ghost.clone()])
            }
        );
        assert_eq!(
            message["scenario"],
            json!({"version":1,"objectives":if slot == 1 {vec![]} else {vec![objective.clone()]}})
        );
        assert_eq!(
            message["shots"],
            if slot == 1 {
                json!([])
            } else {
                json!([shot.clone()])
            }
        );
    }
    let mut restored = RoomRuntime::new(
        Match::from_save_json(room.state().save_json().unwrap()).unwrap(),
        0,
    )
    .unwrap();
    let reconnect = read(&mut restored, FrameKind::Current);
    for slot in 0..3 {
        assert_eq!(reconnect[slot]["ghosts"], messages[slot]["ghosts"]);
        assert_eq!(reconnect[slot]["scenario"], messages[slot]["scenario"]);
    }
    for _ in 0..4 {
        let a = advance(&mut room, 1);
        let b = advance(&mut restored, 1);
        for slot in 0..3 {
            assert_eq!(a[slot]["ghosts"], b[slot]["ghosts"]);
            assert_eq!(a[slot]["scenario"], b[slot]["scenario"]);
        }
    }
    move_unit(room.state_mut(), scout, scout_at);
    let rediscovered = advance(&mut room, 4);
    for slot in [0, 2] {
        assert_eq!(rediscovered[slot]["ghosts"], json!([]));
        assert!(
            rediscovered[slot]["units"]
                .as_array()
                .unwrap()
                .iter()
                .any(|r| r[0] == building && r[14] == 0.75)
        );
    }
    // Ordinary headquarters loss decides Classic even with completed mission data.
    let enemy_hqs = room
        .state()
        .world
        .game
        .units
        .values()
        .filter(|u| u.owner == 1 && room.state().rules.kinds[u.kind].name == "hq")
        .map(|u| u.id)
        .collect::<Vec<_>>();
    for id in enemy_hqs {
        room.state_mut().world.game.units.get_mut(&id).unwrap().hp = 0.;
    }
    advance(&mut room, 1);
    assert!(room.state().finish.is_some());
    for _ in 0..120 {
        if room.hold_callback().unwrap().unwrap().advance {
            advance(&mut room, 1);
        }
        assert!(room.fault().is_none());
    }
}

#[test]
fn net55_classic_mission_recipient_extension_keeps_spawn_and_damage_actuators_refused() {
    let (base, mut raw) = scene(Mode::Classic);
    for action in [
        json!({"kind":"destroy","box":[20,20,20,20]}),
        json!({"kind":"reinforce","group":"later","side":0,"at":[20,20],"roster":[{"type":"rifle","count":1}],"expires":10,"order":{"kind":"hold"}}),
    ] {
        raw["scenario"]["groups"] = if action["kind"] == "reinforce" {
            json!([{"id":"later","units":[]}])
        } else {
            json!([])
        };
        raw["scenario"]["triggers"][0]["actions"] = json!([action]);
        let map = AuthoritativeMap::from_json(&serde_json::to_vec(&raw).unwrap()).unwrap();
        let mut ordinary = base.options.clone();
        ordinary.mode = Mode::Conquest;
        Match::new(&map, ordinary).expect("the mission itself is valid outside Classic");
        assert!(Match::new_classic(&map, base.options.clone()).is_err());
    }
}
