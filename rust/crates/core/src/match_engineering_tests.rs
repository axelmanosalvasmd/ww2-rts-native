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
    let queued = Command::from_json(
        &json!({"t":"dig","ids":[1],"kind":"wire","x":20,"z":20,"queue":true}),
    )
    .unwrap();
    assert_eq!(m.execute_command(0, &queued, false).unwrap(), None);
    assert_eq!(snapshot(&m), before, "queueing preserves real paid work");
    let waiting = m.commands.units[&1].orders.clone();
    assert_eq!(waiting.len(), 1);
    for raw in [
        json!({"t":"dig","ids":[1],"kind":"bridge","x":20,"z":20}),
        json!({"t":"dig","ids":[1],"kind":"wall","x":20,"z":20}),
    ] {
        let command = Command::from_json(&raw).unwrap();
        assert_eq!(
            m.execute_command(0, &command, false).unwrap(),
            Some(Denial::Unsupported)
        );
        assert_eq!(snapshot(&m), before);
        assert_eq!(m.commands.units[&1].orders, waiting);
    }
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

fn queued_dig(kind: &str, x: f64, z: f64) -> Command {
    Command::from_json(&json!({
        "t":"dig","ids":[1],"kind":kind,"x":x,"z":z,"queue":true
    }))
    .unwrap()
}

#[test]
fn queued_dig_preserves_paid_work_and_tallies_only_actual_activation() {
    let fixture: Value =
        serde_json::from_str(include_str!("../tests/fixtures/match-engineering.json")).unwrap();
    let mut m = make(&fixture["traces"][1]);
    assert_eq!(
        m.execute_dig(0, &[1], FortKind::Trench, Point { x: 20., z: 20. }, Some(0.), false).unwrap(),
        None
    );
    let paid = m.field_works.clone();
    let mp = m.start.economy.players[0].mp;
    let spent = m.story[0].mp_spent;
    let walk = Command::from_json(&json!({"t":"move","orders":[[1,24,23]],"queue":true})).unwrap();
    assert_eq!(m.execute_command(0, &walk, false).unwrap(), None);
    assert_eq!(m.execute_command(0, &queued_dig("wire", 20., 25.), false).unwrap(), None);
    assert_eq!(m.field_works, paid);
    assert_eq!(m.start.economy.players[0].mp, mp);
    assert_eq!(m.story[0].mp_spent, spent);
    let waiting = m.commands.units[&1].orders.clone();
    assert_eq!(waiting.len(), 2);
    let stored = waiting.back().unwrap().clone();
    let direction = match stored {
        Command::Dig { ref ids, kind: FortKind::Wire, at, direction: Some(direction), queue: false } => {
            assert_eq!(ids.as_slice(), &[1]);
            assert_eq!(at, Point { x: 20., z: 25. });
            assert_eq!(direction.to_bits(), std::f64::consts::PI.to_bits());
            direction
        }
        _ => panic!("queued work must retain normalized immediate Dig intent"),
    };
    m.start_queued_orders(1, false).unwrap();
    assert_eq!(m.field_works, paid, "busy worker keeps its real paid job");
    assert_eq!(m.commands.units[&1].orders, waiting);
    // The canonical rifle starts exactly three metres from its paid trench.
    // Complete it through the real timed provider without moving the actor.
    for _ in 0..260 {
        m.apply_field_work_unit(1).unwrap();
    }
    assert!(!m.world.game.units[&1].dig);
    assert!(m.field_works.jobs().is_empty());
    m.start_queued_orders(1, false).unwrap();
    assert!(!m.world.game.units[&1].path.is_empty());
    assert_eq!(m.start.economy.players[0].mp, mp);
    for _ in 0..160 {
        m.world.game.tick += 1;
        m.begin_path_tick(4096, 20).unwrap();
        m.start_queued_orders(1, false).unwrap();
        if m.field_works.is_active(1) {
            break;
        }
        m.world.game.units.get_mut(&1).unwrap().repath -= 0.05;
        m.move_unit_with_command_routes(1, 1.).unwrap();
    }
    assert!(m.world.game.units[&1].x > 22., "the actor actually moved before activation");
    assert_eq!(m.field_works.jobs()[&1].kind, FortKind::Wire);
    assert_eq!(m.field_works.jobs()[&1].at, Point { x: 20., z: 25. });
    assert_eq!(m.field_works.jobs()[&1].direction.to_bits(), direction.to_bits());
    assert_eq!(mp - m.start.economy.players[0].mp, 25.);
    assert_eq!(m.story[0].mp_spent - spent, 25.);
    assert!(m.commands.units[&1].orders.is_empty());
    let activated = m.field_works.clone();
    m.start_queued_orders(1, false).unwrap();
    assert_eq!(m.field_works, activated);
    assert_eq!(mp - m.start.economy.players[0].mp, 25., "no second debit");
    assert_eq!(m.story[0].mp_spent - spent, 25., "no second tally");
    optional_queued_source_fixture();
}

#[test]
fn queued_dig_validates_placement_and_capacity_without_reserving_manpower() {
    let fixture: Value =
        serde_json::from_str(include_str!("../tests/fixtures/match-engineering.json")).unwrap();
    let mut m = make(&fixture["traces"][1]);
    m.start.economy.players[0].mp = 0.;
    let spent = m.story[0].mp_spent;
    let unseen = Point { x: 70., z: 70. };
    assert!(!m.recipient_sees(0, unseen).unwrap());
    assert_eq!(m.execute_command(0, &queued_dig("trench", unseen.x, unseen.z), false).unwrap(), Some(Denial::NotVisible));
    assert!(m.commands.units[&1].orders.is_empty());
    for count in 1..=8 {
        assert_eq!(m.execute_command(0, &queued_dig("trench", 20., 20.), false).unwrap(), None);
        assert_eq!(m.commands.units[&1].orders.len(), count);
        assert_eq!(m.start.economy.players[0].mp, 0.);
        assert_eq!(m.story[0].mp_spent, spent);
        assert!(m.field_works.jobs().is_empty());
    }
    let waiting = m.commands.units[&1].orders.clone();
    assert_eq!(m.execute_command(0, &queued_dig("trench", 20., 20.), false).unwrap(), Some(Denial::QueueFull));
    assert_eq!(m.commands.units[&1].orders, waiting);
    // Visibility refusal precedes capacity refusal, even with eight orders.
    assert_eq!(m.execute_command(0, &queued_dig("trench", unseen.x, unseen.z), false).unwrap(), Some(Denial::NotVisible));
    assert_eq!(m.commands.units[&1].orders, waiting);
    let mut wrong_first = queued_dig("trench", 20., 20.);
    if let Command::Dig { ids, .. } = &mut wrong_first {
        *ids = vec![999, 1];
    }
    assert_eq!(m.execute_command(0, &wrong_first, false).unwrap(), Some(Denial::NoBuilders));
    assert_eq!(m.commands.units[&1].orders, waiting);
    m.start_queued_orders(1, false).unwrap();
    assert!(m.commands.units[&1].orders.is_empty(), "unaffordable orders are skipped");
    assert!(m.field_works.jobs().is_empty());
    assert_eq!(m.start.economy.players[0].mp, 0.);
    assert_eq!(m.story[0].mp_spent, spent);
}

#[test]
fn queued_dig_errors_preserve_queue_drift_wallet_story_and_views() {
    let fixture: Value =
        serde_json::from_str(include_str!("../tests/fixtures/match-engineering.json")).unwrap();
    let mut invalid_wallet = make(&fixture["traces"][1]);
    invalid_wallet.start.economy.players[0].mp = f64::NAN;
    let before = snapshot(&invalid_wallet);
    let controls = invalid_wallet.commands.units.clone();
    let units = invalid_wallet.world.game.units.clone();
    let invalid_mp_bits = invalid_wallet.start.economy.players[0].mp.to_bits();
    assert!(invalid_wallet.execute_command(0, &queued_dig("trench", 20., 20.), false).is_err());
    assert_eq!(snapshot(&invalid_wallet), before);
    assert_eq!(invalid_wallet.start.economy.players[0].mp.to_bits(), invalid_mp_bits);
    assert_eq!(invalid_wallet.commands.units, controls);
    assert_eq!(invalid_wallet.world.game.units, units);
    assert!(invalid_wallet.field_works.jobs().is_empty());
    for fault in ["story", "path", "metadata", "selection", "capacity"] {
        let mut m = make(&fixture["traces"][1]);
        assert_eq!(m.execute_command(0, &queued_dig("trench", 20., 20.), false).unwrap(), None);
        let tail = Command::from_json(&json!({"t":"move","orders":[[1,30,23]],"queue":true})).unwrap();
        assert_eq!(m.execute_command(0, &tail, false).unwrap(), None);
        assert_eq!(m.commands.units[&1].orders.len(), 2);
        let unit = m.world.game.units.get_mut(&1).unwrap();
        unit.drift = true;
        unit.path = vec![Point { x: 22., z: 23. }];
        unit.reverse = Some(Point { x: 18., z: 23. });
        match fault {
            "story" => m.story[0].mp_spent = f64::NAN,
            "path" => m.world.game.units.get_mut(&1).unwrap().path_gen = u64::MAX,
            "metadata" => { m.firing.remove(1); }
            "selection" => {
                let Command::Dig { ids, .. } = m.commands.units.get_mut(&1).unwrap().orders.front_mut().unwrap() else { unreachable!() };
                *ids = vec![2];
            }
            "capacity" => {
                let control = m.commands.units.get_mut(&1).unwrap();
                let admitted = control.orders.front().unwrap().clone();
                control.orders.resize(9, admitted);
            }
            _ => unreachable!(),
        }
        let before = snapshot(&m);
        let controls = m.commands.units.clone();
        let units = m.world.game.units.clone();
        let firing = m.firing.get(1).copied();
        let ability = m.abilities.get(1).copied();
        let path_gen = m.world.game.units[&1].path_gen;
        let work = m.field_works.clone();
        let story_bits = m.story.iter().map(|story| {
            StoryKey::ALL.map(|key| story.value(key).to_bits())
        }).collect::<Vec<_>>();
        let currency_bits = m.start.economy.players.iter().map(|player| {
            [player.mp.to_bits(), player.mun.to_bits(), player.fuel.to_bits()]
        }).collect::<Vec<_>>();
        let memory = m.recipients.iter().map(|r| r.remembered().to_vec()).collect::<Vec<_>>();
        let fog = m.routing_fog.clone();
        let pending = m.world.pending(crate::world::Recipient::Player(0)).cloned();
        let error = m.start_queued_orders(1, false).unwrap_err();
        if fault == "capacity" {
            assert!(error.contains("capacity"), "{error}");
        }
        assert_eq!(snapshot(&m), before, "{fault}");
        assert_eq!(m.commands.units, controls, "{fault}");
        assert_eq!(m.world.game.units, units, "{fault}");
        assert_eq!(m.firing.get(1), firing.as_ref(), "{fault}");
        assert_eq!(m.abilities.get(1), ability.as_ref(), "{fault}");
        assert_eq!(m.world.game.units[&1].path_gen, path_gen, "{fault}");
        assert_eq!(m.field_works, work, "{fault}");
        assert_eq!(m.story.iter().map(|story| {
            StoryKey::ALL.map(|key| story.value(key).to_bits())
        }).collect::<Vec<_>>(), story_bits, "{fault}");
        assert_eq!(m.start.economy.players.iter().map(|player| {
            [player.mp.to_bits(), player.mun.to_bits(), player.fuel.to_bits()]
        }).collect::<Vec<_>>(), currency_bits, "{fault}");
        assert_eq!(m.recipients.iter().map(|r| r.remembered().to_vec()).collect::<Vec<_>>(), memory, "{fault}");
        assert_eq!(m.routing_fog, fog, "{fault}");
        assert_eq!(m.world.pending(crate::world::Recipient::Player(0)), pending.as_ref(), "{fault}");
    }
}

#[test]
fn queued_dig_uses_real_single_fort_reservations_and_rechecks_at_activation() {
    let fixture: Value =
        serde_json::from_str(include_str!("../tests/fixtures/match-engineering.json")).unwrap();
    let mut m = make(&fixture["traces"][1]);
    // Two unpaid hospital intents reserve no site or currency. The first real
    // paid job must prevent the second one from starting after it completes.
    let order = queued_dig("aid", 20., 20.);
    let mp = m.start.economy.players[0].mp;
    let spent = m.story[0].mp_spent;
    assert_eq!(m.execute_command(0, &order, false).unwrap(), None);
    assert_eq!(m.execute_command(0, &order, false).unwrap(), None);
    assert_eq!(m.start.economy.players[0].mp, mp);
    assert_eq!(m.story[0].mp_spent, spent);
    assert!(m.world.aid.is_empty());
    assert!(m.field_works.jobs().is_empty());
    m.start_queued_orders(1, false).unwrap();
    assert_eq!(m.field_works.jobs()[&1].kind, FortKind::Aid);
    assert_eq!(m.commands.units[&1].orders.len(), 1);
    let paid = m.field_works.clone();
    assert_eq!(m.execute_command(0, &queued_dig("aid", 20., 25.), false).unwrap(), Some(Denial::Maximum));
    assert_eq!(m.field_works, paid);
    assert_eq!(m.commands.units[&1].orders.len(), 1);
    for _ in 0..260 {
        m.apply_field_work_unit(1).unwrap();
    }
    assert_eq!(m.world.aid.len(), 1);
    m.start_queued_orders(1, false).unwrap();
    assert!(m.commands.units[&1].orders.is_empty());
    assert!(m.field_works.jobs().is_empty());
    assert_eq!(mp - m.start.economy.players[0].mp, 100.);
    assert_eq!(m.story[0].mp_spent - spent, 100.);
}

// Optional private evidence uses the existing selected check, never another gate.
// The twelve committed paid-work traces and their consumer remain unchanged.
fn optional_queued_source_fixture() {
    let Some(path) = std::env::var_os("WW2_ENGINEERING_QUEUED_FIXTURE") else {
        return;
    };
    assert!(std::path::Path::new(&path).is_absolute());
    let fixture: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let baseline_bytes = include_str!("../tests/fixtures/match-engineering.json");
    assert_eq!(fixture["baselineBytes"].as_str().unwrap(), baseline_bytes);
    let baseline: Value = serde_json::from_str(baseline_bytes).unwrap();
    assert_eq!(baseline["traces"].as_array().unwrap().len(), 12);
    for key in ["reference", "node", "arch"] {
        assert_eq!(fixture[key], baseline[key], "private queued provenance {key}");
    }
    let cases = fixture["traces"].as_array().unwrap();
    let expected = [
        ("queued-paid-activation-and-retained-tail", 5),
        ("queued-zero-mp-skip-and-trailing-move", 4),
        ("queued-placement-before-capacity-at-zero-mp", 12),
        ("queued-hospital-unpaid-and-paid-reservations", 7),
    ];
    assert_eq!(cases.len(), expected.len());
    for (case, (name, rows)) in cases.iter().zip(expected) {
        assert_eq!(case["name"], name);
        assert_eq!(case["map"], baseline["traces"][1]["map"]);
        assert_eq!(case["units"], baseline["traces"][1]["units"]);
        assert_eq!(case["rows"].as_array().unwrap().len(), rows);
        let mut m = make(case);
        assert_eq!(queued_source_snapshot(&m), case["initial"], "{name} initial");
        for (index, row) in case["rows"].as_array().unwrap().iter().enumerate() {
            let action = &row["action"];
            let mut result = Value::Null;
            match action["op"].as_str().unwrap() {
                "command" => {
                    let command = Command::from_json(&action["cmd"]).unwrap();
                    assert_eq!(action["slot"].as_u64().unwrap_or(0), 0);
                    result = m
                        .execute_command(0, &command, false)
                        .unwrap()
                        .map(|denial| json!(denial.name()))
                        .unwrap_or(Value::Null);
                }
                "queued" => {
                    assert_eq!(action["id"], 1);
                    m.start_queued_orders(1, false).unwrap();
                }
                "step" => {
                    assert_eq!(action["id"], 1);
                    for _ in 0..action["n"].as_u64().unwrap() {
                        m.apply_field_work_unit(1).unwrap();
                    }
                }
                "money" => m.start.economy.players[0].mp = action["mp"].as_f64().unwrap(),
                op => panic!("unrepresented private queued action {op}"),
            }
            assert_eq!(encode(result), row["result"], "{name} {index} result");
            assert_eq!(queued_source_snapshot(&m), row["snapshot"], "{name} {index} state");
        }
    }
}

fn queued_source_snapshot(m: &Match) -> Value {
    // Exact stored Source commands, before recipient projection. Only these
    // two command families occur; reject unrepresented native intent fields.
    let orders = m.world.game.units.keys().map(|id| {
        let orders = m.commands.units[id].orders.iter().map(|command| match command {
            Command::Dig { ids, kind, at, direction: Some(direction), queue: false } => {
                json!({"t":"dig","ids":ids,"kind":kind.as_str(),"x":at.x,"z":at.z,"dir":direction})
            }
            Command::Move { orders, control_ids, attack_move, queue, together, face } => {
                assert!(control_ids.is_none() && !*attack_move && !*queue && !*together && face.is_none());
                assert_eq!(orders.len(), 1);
                let order = &orders[0];
                json!({"t":"move","orders":[[order.unit,order.at.x,order.at.z]],"x":order.at.x,"z":order.at.z})
            }
            command => panic!("unrepresented private queued intent {command:?}"),
        }).collect::<Vec<_>>();
        json!({"id":id,"orders":orders})
    }).collect::<Vec<_>>();
    json!({
        "paid":snapshot(m),
        "orders":encode(json!(orders)),
        "aid":encode(json!(m.world.aid.iter().map(|(cell, owner)| json!([cell,owner])).collect::<Vec<_>>()))
    })
}
