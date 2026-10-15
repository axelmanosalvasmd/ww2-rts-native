//! The complete MOV-12 corner journey, with canonical births and actual ticks.
use serde_json::{Value, json};
use std::io::Read;
use ww2_core::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::Point,
    map::AuthoritativeMap,
    match_commands::Command,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    weather::WeatherChoice,
};
fn bits(n: f64) -> Value {
    json!(format!("{:016x}", n.to_bits()))
}
fn roster(m: &Match) -> Value {
    json!(
        m.world
            .game
            .units
            .values()
            .map(|u| json!({"id":u.id,
        "type":m.rules.kinds[u.kind].name,"owner":u.owner,"cells":u.cells}))
            .collect::<Vec<_>>()
    )
}
fn refresh(m: &mut Match) {
    m.grid.rebuild(&m.world.game.units);
    ww2_core::vision::update_vision(&mut m.world.game, &m.rules, &m.grid);
    for owner in 0..m.world.game.players.len() {
        m.refresh_recipient_routing(owner).unwrap();
    }
}
fn issue(m: &mut Match, input: Value, observed: &Value) -> Vec<u32> {
    assert_eq!(input, observed["input"]);
    let command = Command::from_json(&input).unwrap();
    let before: Vec<_> = m.world.game.units.keys().copied().collect();
    let result = m.execute_command(0, &command, false);
    assert_eq!(result, Ok(None), "actual accepted public command");
    assert!(observed["result"].is_null());
    let births: Vec<_> = m
        .world
        .game
        .units
        .keys()
        .copied()
        .filter(|id| !before.contains(id))
        .collect();
    assert_eq!(json!(births), observed["births"]);
    births
}
#[test]
fn original_obstructed_corner_is_kept_then_arrives_after_sixty_more_ticks() {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mov-corner.json.gz");
    let file = std::fs::File::open(path).expect("Root-granted authentic MOV-12 fixture required");
    let mut text = String::new();
    flate2::read::GzDecoder::new(file)
        .read_to_string(&mut text)
        .unwrap();
    let f: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(f["reference"], "e4e579a838aabae3b1d9dadbc17a0a87147bc52d");
    assert_eq!(f["node"], "v24.21.0");
    assert_eq!(f["arch"], "x64");
    assert_eq!(f["completed"], true);
    assert!(f["failure"].is_null() && f["readbackFailure"].is_null());
    let original = &f["original"];
    assert_eq!(original["completed"], true);
    assert_eq!(
        original["name"],
        "Jam recovery keeps terrain corners when the next waypoint is obstructed"
    );
    assert_eq!(
        original["sha256"],
        "53571d81b0266ac7c31812eda9800d37b758f3c013abfed93b63bf6503db92ea"
    );
    let constructors = original["constructors"].as_array().unwrap();
    assert_eq!(constructors.len(), 1);
    let original_rows: Vec<_> = (0..20)
        .map(|y| {
            if y == 6 {
                ".".repeat(6) + "B" + &".".repeat(13)
            } else {
                ".".repeat(20)
            }
        })
        .collect();
    assert_eq!(
        constructors[0]["input"],
        json!([{
        "w":20,"h":20,"rows":original_rows,
        "spawns":[{"x":1,"y":1},{"x":18,"y":1},{"x":1,"y":18}],
        "points":[{"x":10,"y":10}]}, ["a","b"], false])
    );
    assert_eq!(constructors[0]["sequence"], 0);
    assert_eq!(constructors[0]["completed"], true);
    assert!(constructors[0]["initial"].is_object());
    assert!(constructors[0].get("failure").is_none());
    let original_commands = original["commands"].as_array().unwrap();
    assert_eq!(original_commands.len(), 1);
    let command = &original_commands[0];
    assert_eq!(command["sequence"], 1);
    assert_eq!(command["op"], "fixtureCommand");
    assert_eq!(
        command["input"],
        json!({"owner":0,"cmd":{"t":"buy","unit":"rifle"}})
    );
    assert_eq!(command["completed"], true);
    assert_eq!(command["returnedUndefined"], true);
    assert!(command.get("result").is_none() && command.get("failure").is_none());
    assert!(command["before"].is_object() && command["after"].is_object());
    let original_calls = original["calls"].as_array().unwrap();
    assert_eq!(original_calls.len(), 61);
    for (index, call) in original_calls.iter().enumerate() {
        assert_eq!(call["sequence"], index + 2);
        assert_eq!(call["op"], "step");
        assert_eq!(call.get("input"), Some(&Value::Null));
        assert_eq!(call["completed"], true);
        assert_eq!(call["returnedUndefined"], true);
        assert!(call.get("result").is_none() && call.get("failure").is_none());
        assert!(call["before"].is_object() && call["after"].is_object());
    }
    let c = &f["companion"];
    assert_eq!(c["completed"], true);
    let rows: Vec<_> = (0..96)
        .map(|y| {
            if y == 6 {
                ".".repeat(6) + "B" + &".".repeat(73)
            } else {
                ".".repeat(80)
            }
        })
        .collect();
    let map = json!({"name":"MOV-12 canonical corner companion","w":80,"h":96,"rows":rows,
        "spawns":[{"x":10,"y":82},{"x":70,"y":82}],"points":[{"x":40,"y":90,"mp":0,"vp":0}]});
    assert_eq!(map, c["map"]);
    assert_eq!(
        c["options"],
        json!({"mode":"conquest","army":"standard","weather":false,
        "tech":false,"supply":false,"logistics":false})
    );
    let authoritative = AuthoritativeMap::from_json(&serde_json::to_vec(&map).unwrap()).unwrap();
    let mut m = Match::new(
        &authoritative,
        MatchOptions {
            seed: 7301,
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
            weather_seed: Some(1),
            living_weather: false,
            map_key: None,
            tech: false,
            supply: false,
            logistics: false,
            sandbox: false,
        },
    )
    .expect("canonical native construction");
    assert_eq!(roster(&m), c["initial"]);
    let retained: Vec<_> = m
        .world
        .game
        .units
        .values()
        .map(|u| (u.id, u.kind, u.owner, u.cells.clone()))
        .collect();
    let hq = m
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 0 && m.rules.kinds[u.kind].name == "hq")
        .unwrap()
        .id;
    m.start.economy.players[0].mp = 10000.0;
    let commands = c["commands"].as_array().unwrap();
    assert_eq!(commands.len(), 2);
    let births = issue(
        &mut m,
        json!({"t":"buy","unit":"rifle","from":hq}),
        &commands[0],
    );
    assert_eq!(births.len(), 1);
    let id = births[0];
    assert_eq!(json!(id), c["actor"]);
    assert!(
        m.combat.get(id).is_some() && m.firing.get(id).is_some() && m.recovery.get(id).is_some()
    );
    let u = m.world.game.units.get_mut(&id).unwrap();
    u.x = 11.0;
    u.z = 11.0;
    u.rot = 0.0;
    let control = m.commands.units.get_mut(&id).unwrap();
    control.aim = 0.0;
    control.auto_retreat = false;
    refresh(&mut m);
    issue(
        &mut m,
        json!({"t":"move","orders":[[id,17,15]]}),
        &commands[1],
    );
    assert_eq!(
        c["authored"],
        json!({"path":[{"x":11,"z":15},{"x":17,"z":15}],"was":{"x":11,"z":11},"jam":3.1})
    );
    // was/jam are declared by the old check but neither pinned movement module
    // reads them. Keep them in raw Source evidence; no native field is invented.
    let corner = Point { x: 11.0, z: 15.0 };
    let end = Point { x: 17.0, z: 15.0 };
    let u = m.world.game.units.get_mut(&id).unwrap();
    u.path = vec![corner, end];
    u.path_gen = u.path_gen.checked_add(1).unwrap();
    let steps = c["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 61);
    for (index, expected) in steps.iter().enumerate() {
        m.advance_tick()
            .unwrap_or_else(|e| panic!("first real corner tick {} failed: {e:?}", index + 1));
        for (id, kind, owner, cells) in &retained {
            let u = &m.world.game.units[id];
            assert_eq!((u.kind, u.owner, &u.cells), (*kind, *owner, cells));
        }
        let u = &m.world.game.units[&id];
        let actual = json!({"id":id,"x":bits(u.x),"z":bits(u.z),"rot":bits(u.rot),"speed":bits(u.move_speed),
            "path":u.path.iter().map(|p|vec![bits(p.x),bits(p.z)]).collect::<Vec<_>>(),
            "vx":bits(u.vx.unwrap_or(f64::NAN)),"vz":bits(u.vz.unwrap_or(f64::NAN))});
        assert_eq!(actual, *expected, "native corner tick {}", index + 1);
        if index == 0 {
            assert_eq!(
                u.path.first(),
                Some(&corner),
                "three seconds of crowding never skips a required corner"
            );
            assert_eq!(u.x, 11.0, "the squad keeps moving along the clear leg");
        }
    }
    let u = &m.world.game.units[&id];
    assert!(
        ww2_core::math::hypot(u.x - end.x, u.z - end.z) < 1.0,
        "the route still reaches its destination around the building"
    );
    assert_eq!(roster(&m), c["final"]);
}
