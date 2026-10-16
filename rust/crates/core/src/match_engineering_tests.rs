use super::*;
use crate::{
    catalog::Catalog,
    economy::{Faction, Mode},
    economy_state::ArmySize,
    field_works::FieldWorksRuntime,
    game::Player,
    grid::SpatialGrid,
    logistics::LogisticsState,
    map::AuthoritativeMap,
    match_abilities::AbilityState,
    match_combat::CombatState,
    match_commands::{Command, MatchCommands, UnitControl},
    match_firing::FiringState,
    match_runtime::{MatchOptions, PlayerConfig},
    unit_spawn::initial_unit,
    weather::WeatherChoice,
    world::World,
};
use serde_json::{Value, json};
fn point(v: &Value) -> Point {
    Point {
        x: v["x"].as_f64().unwrap(),
        z: v["z"].as_f64().unwrap(),
    }
}
fn at(p: Point) -> Value {
    json!({"x":p.x,"z":p.z})
}
fn encode(v: Value) -> Value {
    match v {
        Value::Number(n) => json!(format!("{:016x}", n.as_f64().unwrap().to_bits())),
        Value::Array(v) => Value::Array(v.into_iter().map(encode).collect()),
        Value::Object(v) => Value::Object(v.into_iter().map(|(k, v)| (k, encode(v))).collect()),
        v => v,
    }
}
fn prime(m: &mut Match, id: u32, fields: &Value) {
    let u = m.world.game.units.get_mut(&id).unwrap();
    for (k, v) in fields.as_object().unwrap() {
        match k.as_str() {
            "id" | "type" | "owner" => {}
            "x" => u.x = v.as_f64().unwrap(),
            "z" => u.z = v.as_f64().unwrap(),
            "retreating" => u.retreating = v.as_bool().unwrap(),
            "garrison" => u.garrison = v.as_i64().filter(|x| *x >= 0).map(|x| x as usize),
            "stuck" => u.stuck = v.as_f64().unwrap(),
            "drift" => u.drift = v.as_bool().unwrap(),
            "reverse" => u.reverse = Some(point(v)),
            "path" => u.path = v.as_array().unwrap().iter().map(point).collect(),
            "repath" => u.repath = v.as_f64().unwrap(),
            k => panic!("unrepresented engineering input {k}"),
        }
    }
    m.grid.update(id, u);
}
fn make(case: &Value) -> Match {
    make_mode(case, Mode::Conquest)
}
fn make_mode(case: &Value, mode: Mode) -> Match {
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&case["map"]).unwrap()).unwrap();
    let mut m = Match::new(
        &map,
        MatchOptions {
            seed: 701,
            mode,
            players: (0..2)
                .map(|slot| PlayerConfig {
                    name: format!("p{slot}"),
                    team: slot,
                    faction: Faction::try_from(slot as u8).unwrap(),
                })
                .collect(),
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
    .unwrap();
    m.world = World::from_map(&map).unwrap();
    m.world.game.tick = 10;
    m.world.game.rain = 0.;
    m.world.game.wet = Some(0.);
    m.world.game.players = (0..2)
        .map(|slot| Player {
            slot,
            team: slot as i32,
            visible: Vec::new(),
        })
        .collect();
    m.commands = MatchCommands::new(4096);
    m.abilities = AbilityState::default();
    m.firing = FiringState::default();
    m.combat = CombatState::default();
    m.logistics = LogisticsState::default();
    m.field_works = FieldWorksRuntime::default();
    m.grid = SpatialGrid::new(&m.world.game.units, 16.);
    for p in &mut m.start.economy.players {
        p.mp = 500.;
        p.mun = 0.;
        p.fuel = 0.;
    }
    for v in case["units"].as_array().unwrap() {
        let id = v["id"].as_u64().unwrap() as u32;
        let kind = m.rules.kind(v["type"].as_str().unwrap()).unwrap();
        let u = initial_unit(
            &m.rules,
            Catalog::compiled().unwrap(),
            kind,
            id,
            0,
            point(v),
        )
        .unwrap();
        let mut control = UnitControl::new(&u, &m.rules).unwrap();
        control.follow = 77;
        control.auto_cast = false;
        control.auto_retreat = false;
        m.world.game.units.insert(id, u);
        m.commands.units.insert(id, control);
        m.abilities.initialize(id);
        m.firing.initialize(id);
        m.combat.initialize(id);
        prime(&mut m, id, v);
    }
    m.building_cells.clear();
    for slot in 0..2 {
        m.world
            .register_recipient(crate::world::Recipient::Player(slot));
    }
    m.initialize_recipient_routing().unwrap();
    m
}
fn snapshot(m: &Match) -> Value {
    crate::unit_placement::tests::assert_current(&m.world.game, &m.grid);
    encode(json!({
        "units":m.world.game.units.values().map(|u| {
            let c=&m.commands.units[&u.id];
            let dig=m.field_works.jobs().get(&u.id).map(|d|json!({"kind":d.kind.as_str(),"at":at(d.at),"dir":d.direction,"cells":d.cells.iter().map(|c|json!([c.cell,(c.ch as char).to_string()])).collect::<Vec<_>>(),"t":d.elapsed,"n":d.original_cells,"reach":d.reach,"from":d.from.map(at)}));
            assert_eq!(u.dig,dig.is_some());
            json!({"id":u.id,"x":u.x,"z":u.z,"garrison":u.garrison.map(i64::try_from).transpose().unwrap().unwrap_or(-1),"dig":dig,"follow":c.follow,"path":u.path.iter().copied().map(at).collect::<Vec<_>>(),"repath":u.repath,"drift":u.drift,"reverse":u.reverse.map(at),"stuck":u.stuck,"moveOutcome":c.outcome.map(|x| match x { MoveOutcome::Interrupted=>"interrupted", MoveOutcome::Moving=>"moving", MoveOutcome::Unreachable=>"unreachable", MoveOutcome::Arrived=>"arrived" }),"moveOutcomeTick":c.outcome_tick})
        }).collect::<Vec<_>>(),
        "mp":m.start.economy.players.iter().map(|p|p.mp).collect::<Vec<_>>(),
        "story":m.story.iter().map(|s|json!({"mpSpent":s.mp_spent,"built":s.built})).collect::<Vec<_>>(),
        "chars":String::from_utf8(m.world.game.chars.clone()).unwrap(),"pits":m.field_works.pits().iter().copied().collect::<Vec<_>>()
    }))
}
#[test]
fn original_paid_dig_lifecycles() {
    let fixture: Value =
        serde_json::from_str(include_str!("../tests/fixtures/match-engineering.json")).unwrap();
    assert_eq!(
        fixture["reference"],
        "e4e579a838aabae3b1d9dadbc17a0a87147bc52d"
    );
    for case in fixture["traces"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let mut m = make(case);
        assert_eq!(snapshot(&m), case["initial"], "{name} initial");
        for (index, row) in case["rows"].as_array().unwrap().iter().enumerate() {
            let a = &row["action"];
            let mut result = Value::Null;
            match a["op"].as_str().unwrap() {
                "command" => {
                    let command = Command::from_json(&a["cmd"]).unwrap();
                    let denial = m
                        .execute_command(a["slot"].as_u64().unwrap_or(0) as usize, &command, false)
                        .unwrap();
                    result = denial.map(|d| json!(d.name())).unwrap_or(Value::Null);
                }
                "step" => {
                    for _ in 0..a["n"].as_u64().unwrap() {
                        m.apply_field_work_unit(a["id"].as_u64().unwrap() as u32)
                            .unwrap();
                    }
                }
                "begin" => {
                    m.world.game.tick = a["tick"].as_i64().unwrap();
                    m.begin_path_tick(
                        a["budget"].as_u64().unwrap() as usize,
                        a["retry"].as_u64().unwrap_or(1) as u32,
                    )
                    .unwrap();
                }
                "phase" => m
                    .apply_field_work_unit(a["id"].as_u64().unwrap() as u32)
                    .unwrap(),
                "goal" => {
                    result = m
                        .current_dig_goal(a["id"].as_u64().unwrap() as u32)
                        .map(at)
                        .unwrap_or(Value::Null)
                }
                "prime" => prime(&mut m, a["id"].as_u64().unwrap() as u32, &a["fields"]),
                "money" => {
                    m.start.economy.players[a["slot"].as_u64().unwrap_or(0) as usize].mp =
                        a["mp"].as_f64().unwrap()
                }
                "mutate" => {
                    for cell in a["cells"].as_array().unwrap() {
                        m.world
                            .set_cell(
                                cell.as_u64().unwrap() as usize,
                                a["ch"].as_str().unwrap().as_bytes()[0],
                            )
                            .unwrap();
                    }
                }
                "seen" => {
                    m.world.mine_seen.insert(
                        a["cell"].as_u64().unwrap() as usize,
                        a["mask"].as_u64().unwrap() as u32,
                    );
                }
                op => panic!("unrepresented engineering action {op}"),
            }
            assert_eq!(encode(result), row["result"], "{name} {index} result");
            assert_eq!(snapshot(&m), row["snapshot"], "{name} {index} state");
        }
    }
}
#[test]
fn admission_guards_reject_before_payment_or_cancellation() {
    let fixture: Value =
        serde_json::from_str(include_str!("../tests/fixtures/match-engineering.json")).unwrap();
    let mut m = make(&fixture["traces"][1]);
    m.execute_dig(
        0,
        &[1],
        FortKind::Trench,
        Point { x: 20., z: 20. },
        None,
        false,
    )
    .unwrap();
    let before = snapshot(&m);
    let command =
        Command::from_json(&json!({"t":"dig","ids":[1],"kind":"gate","x":20,"z":20})).unwrap();
    assert_eq!(
        m.execute_command(0, &command, false).unwrap(),
        Some(Denial::Unsupported)
    );
    assert_eq!(snapshot(&m), before);
    assert_eq!(
        Command::from_json(&json!({"t":"dig","ids":[1],"kind":"unknown","x":20,"z":20})),
        Err(Denial::Blocked)
    );
    m.world.game.units.get_mut(&1).unwrap().x = f64::NAN;
    assert_eq!(m.current_dig_goal(1), None);
    m.world.game.units.get_mut(&1).unwrap().x = 20.;
    assert_eq!(snapshot(&m), before);
    m.world.game.units.get_mut(&1).unwrap().dig = false;
    assert!(
        m.execute_dig(
            0,
            &[1],
            FortKind::Wire,
            Point { x: 20., z: 20. },
            None,
            false
        )
        .is_err()
    );
    m.world.game.units.get_mut(&1).unwrap().dig = true;
    assert_eq!(snapshot(&m), before);
    m.story[0].mp_spent = f64::NAN;
    assert!(
        m.execute_dig(
            0,
            &[1],
            FortKind::Wire,
            Point { x: 20., z: 20. },
            None,
            false
        )
        .is_err()
    );
    m.story[0].mp_spent = 30.;
    assert_eq!(snapshot(&m), before);
}

#[test]
fn ordinary_default_dig_matches_real_source_payment_and_preserves_refused_work() {
    let fixture: Value =
        serde_json::from_str(include_str!("../tests/fixtures/match-engineering.json")).unwrap();
    let case = &fixture["traces"][0];
    let row = &case["rows"][0];
    let command = Command::from_json(&row["action"]["cmd"]).unwrap();
    let mut default = make_mode(case, Mode::Default);
    let mut conquest = make_mode(case, Mode::Conquest);
    assert!(default.world.game.mode.is_none() && conquest.world.game.mode.is_none());
    for m in [&mut default, &mut conquest] {
        assert_eq!(snapshot(m), case["initial"]);
        let mp = m.start.economy.players[0].mp;
        let result = m.execute_command(0, &command, false).unwrap();
        assert_eq!(
            encode(result.map(|d| json!(d.name())).unwrap_or(Value::Null)),
            row["result"]
        );
        assert_eq!(snapshot(m), row["snapshot"]);
        assert!(m.field_works.is_active(1));
        assert!(m.world.game.units[&1].dig);
        assert!(m.start.economy.players[0].mp < mp);
        assert_eq!(mp - m.start.economy.players[0].mp, m.story[0].mp_spent);
    }
    assert_eq!(snapshot(&default), snapshot(&conquest));
    assert_eq!(
        default.start.random.snapshot(),
        conquest.start.random.snapshot()
    );
    // A refused mode context cannot replace paid work or acknowledge terrain reads.
    for (mode, game_mode) in [
        (Mode::World, None),
        (Mode::Default, Some(crate::game::ModeKind::Assault)),
        (Mode::Conquest, Some(crate::game::ModeKind::Classic)),
    ] {
        default.options.mode = mode;
        default.world.game.mode = game_mode;
        let before = snapshot(&default);
        let memory = default
            .recipients
            .iter()
            .map(|r| r.remembered().to_vec())
            .collect::<Vec<_>>();
        let fog = default.routing_fog.clone();
        let pending = default
            .world
            .pending(crate::world::Recipient::Player(0))
            .cloned();
        assert!(default.execute_command(0, &command, false).is_err());
        assert_eq!(snapshot(&default), before);
        assert_eq!(
            default
                .recipients
                .iter()
                .map(|r| r.remembered().to_vec())
                .collect::<Vec<_>>(),
            memory
        );
        assert_eq!(default.routing_fog, fog);
        assert_eq!(
            default.world.pending(crate::world::Recipient::Player(0)),
            pending.as_ref()
        );
    }
}

#[path = "engineering_runtime_build_tests.rs"]
mod runtime_builds;
