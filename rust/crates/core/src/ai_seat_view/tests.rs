use super::*;
use crate::{economy::Faction, economy_state::ArmySize, map::AuthoritativeMap, match_runtime::{MatchOptions, PlayerConfig}, weather::WeatherChoice};
use serde_json::json;

fn players() -> Vec<PublicPlayer> {
    vec![
        PublicPlayer { name: "A".into(), team: 0, faction: 0, spawn: Some(Point { x: 2., z: 2. }) },
        PublicPlayer { name: "B".into(), team: 1, faction: 1, spawn: Some(Point { x: 9., z: 9. }) },
    ]
}

#[test]
fn ai04_players_use_public_metadata_and_delivered_out_vp_with_world_spawn_privacy() {
    let map = PublicTerrain { width: 4, height: 1, chars: b"....".to_vec(), levels: None };
    let mut roster = players();
    roster.push(PublicPlayer { name: "Ally".into(), team: 0, faction: 2, spawn: Some(Point { x: 8., z: 8. }) });
    let mut world = SeatMemory::new(0, roster, map.clone(), true).unwrap();
    let view = world.accept(&json!({"out":[false,true,false],"vp":[3,7,2],"home":[5,6],"cells":[]})).unwrap();
    assert_eq!(view.players[0], SeatPlayer { slot: 0, name: "A".into(), team: 0, faction: 0, spawn: Some(Point { x: 5., z: 6. }), out: false, vp: 3. });
    assert_eq!(view.players[2], SeatPlayer { slot: 2, name: "Ally".into(), team: 0, faction: 2, spawn: Some(Point { x: 8., z: 8. }), out: false, vp: 2. });
    assert_eq!(view.players[0].spawn, Some(Point { x: 5., z: 6. }));
    assert_eq!(view.players[1].spawn, None);
    assert!(view.players[1].out);
    assert_eq!(view.players[1].vp, 7.);
    assert_eq!(view.local_defender.unwrap(), SeatPlayer { slot: -1, name: "Local defenders".into(), team: -1, faction: 0, spawn: None, out: false, vp: 0. });
    assert_eq!(view.terrain.chars.as_ref(), b"????");
    assert_eq!(view.terrain.height.as_deref(), Some([0, 0, 0, 0].as_slice()));

    let mut classic = SeatMemory::new(0, players(), map, false).unwrap();
    let view = classic.accept(&json!({"out":[false,false],"vp":[0,0],"cells":[]})).unwrap();
    assert_eq!(view.players[1].spawn, Some(Point { x: 9., z: 9. }));
    assert!(view.local_defender.is_none());
}

#[test]
fn ai05_only_delivered_cells_change_copy_on_write_terrain_and_mine_knowledge() {
    let map = PublicTerrain { width: 4, height: 1, chars: b".=..".to_vec(), levels: Some(vec![0, 1, 0, 0]) };
    let mut memory = SeatMemory::new(0, players(), map, false).unwrap();
    let old = memory.accept(&json!({"out":[false,false],"vp":[0,0],"cells":[]})).unwrap();
    assert_eq!(old.bridge_cells, BTreeSet::from([1]));
    let changed = memory.accept(&json!({"out":[false,false],"vp":[0,0],"cells":[
        [1,".",2,0,{"ground":".","object":"."}],
        [2,"N",3,0,{"ground":".","object":".","mineOwned":true}]
    ]})).unwrap();
    assert_eq!(changed.terrain.chars.as_ref(), b"..N.");
    assert_eq!(changed.map_chars.as_ref(), b".=..");
    assert_eq!(changed.terrain.flags[2], terrain_flag(b'.').unwrap());
    assert_eq!(changed.terrain.height.as_ref().unwrap()[2], 3);
    assert_eq!(changed.bridge_cells, BTreeSet::from([1]));
    assert_eq!(changed.own_mines, BTreeSet::from([2]));
    assert_eq!(old.terrain.chars.as_ref(), b".=..");
    assert_eq!(old.terrain.height.as_ref().unwrap()[2], 0);
    let denied = memory.accept(&json!({"out":[false,false],"vp":[0,0],"cells":[
        [2,"N",3,0,{"ground":".","object":".","mineOwned":false}],
        [9,"T",0,0,{"ground":".","object":"T"}]
    ]}));
    assert!(denied.is_err());
    let after = memory.accept(&json!({"out":[false,false],"vp":[0,0],"cells":[]})).unwrap();
    assert_eq!(after.own_mines, BTreeSet::from([2]));
    assert_eq!(after.terrain, changed.terrain);
    let enemy = memory.accept(&json!({"out":[false,false],"vp":[0,0],"cells":[
        [2,"N",3,0,{"ground":".","object":".","mineOwned":false}]
    ]})).unwrap();
    assert!(enemy.own_mines.is_empty());
    let cleared = memory.accept(&json!({"out":[false,false],"vp":[0,0],"cells":[
        [2,".",3,0,{"ground":".","object":"."}]
    ]})).unwrap();
    assert!(cleared.own_mines.is_empty());
    assert_eq!(cleared.terrain.chars.as_ref(), b"....");
}

#[test]
fn ai05_world_handover_replays_only_delivered_discoveries_and_remembers_bridges() {
    let map = PublicTerrain { width: 4, height: 1, chars: b"....".to_vec(), levels: None };
    let mut memory = SeatMemory::new(0, players(), map, true).unwrap();
    let human_cells = json!({"out":[false,false],"vp":[0,0],"cells":[
        [0,"=",1,0,{"ground":".","object":"="}],
        [1,"N",1,0,{"ground":".","object":".","mineOwned":false}]
    ]});
    let seen = memory.accept(&human_cells).unwrap();
    assert_eq!(seen.terrain.chars.as_ref(), b"=N??");
    assert_eq!(seen.bridge_cells, BTreeSet::from([0]));
    assert!(seen.own_mines.is_empty());
    let later = memory.accept(&json!({"out":[false,false],"vp":[0,0],"cells":[]})).unwrap();
    assert_eq!(later.terrain.chars.as_ref(), b"=N??");
    assert_eq!(later.map_chars.as_ref(), b"=N??");
}

#[test]
fn live_memory_follows_a_moved_match_and_requires_its_own_forward_frames() {
    let fixture: Value = serde_json::from_str(include_str!("../../tests/fixtures/recipient-selected.json")).unwrap();
    let match_a = crate::recipient_snapshot::tests::initialize(&fixture["initial"]);
    let mut other = crate::recipient_snapshot::tests::initialize(&fixture["initial"]);
    let mut delivery = DeliveryState::new(&match_a).unwrap();
    let mut memory = SeatMemory::from_match(&match_a, 0).unwrap();
    let mut moved = Box::new(match_a);
    assert!(matches!(memory.observe_match(&mut other, &mut delivery), Err(SnapshotError::UnsupportedMode)));
    assert_eq!(memory.observe_match(&mut moved, &mut delivery).unwrap().players.len(), 3);
    assert!(matches!(memory.observe_match(&mut moved, &mut delivery), Err(SnapshotError::Rules)));
    moved.world.game.tick += 1;
    assert_eq!(memory.observe_match(&mut moved, &mut delivery).unwrap().players.len(), 3);
    moved.options.mode = Mode::World;
    assert!(matches!(SeatMemory::from_match(&moved, 0), Err(SnapshotError::UnsupportedMode)));
}

#[test]
fn authored_mine_is_masked_before_ai_terrain_memory_starts() {
    let mut rows = vec![".".repeat(20); 20];
    rows[10].replace_range(10..11, "N");
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&json!({
        "name":"AI public terrain", "w":20, "h":20, "rows":rows,
        "spawns":[{"x":2,"y":2},{"x":17,"y":17}], "points":[{"x":8,"y":8}]
    })).unwrap()).unwrap();
    let state = Match::new(&map, MatchOptions {
        seed: 7, mode: Mode::Conquest,
        players: vec![
            PlayerConfig { name: "A".into(), team: 0, faction: Faction::Usa },
            PlayerConfig { name: "B".into(), team: 1, faction: Faction::Germany },
        ],
        shuffle: false, army: ArmySize::Standard, weather: WeatherChoice::Map,
        weather_seed: None, living_weather: false, map_key: None,
        tech: false, supply: false, logistics: false, sandbox: false,
    }).unwrap();
    assert_eq!(state.world.initial().chars[210], b'N');
    let memory = SeatMemory::from_match(&state, 0).unwrap();
    assert_eq!(memory.current.chars[210], b'.');
    assert_eq!(memory.current.flags[210], terrain_flag(b'.').unwrap());
}
