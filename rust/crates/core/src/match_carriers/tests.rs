use super::*;
use crate::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::Player,
    map::AuthoritativeMap,
    match_commands::{MatchCommands, UnitControl},
    match_runtime::{MatchOptions, PlayerConfig},
    unit_spawn,
    weather::WeatherChoice,
    world::World,
};
use serde_json::{Value, json};
fn bits(v: f64) -> Value {
    json!(format!("{:016x}", v.to_bits()))
}
fn pt(at: Option<Point>) -> Value {
    at.map_or(Value::Null, |p| json!([bits(p.x), bits(p.z)]))
}
fn point(v: &Value) -> Point {
    Point {
        x: v["x"].as_f64().unwrap(),
        z: v["z"].as_f64().unwrap(),
    }
}
fn fixture() -> Value {
    serde_json::from_str(include_str!("../../tests/fixtures/carriers.json")).unwrap()
}
fn make(case: &Value) -> Match {
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&case["map"]).unwrap()).unwrap();
    let mut m = Match::new(
        &map,
        MatchOptions {
            seed: 701,
            mode: Mode::Conquest,
            players: [Faction::Usa, Faction::Germany]
                .into_iter()
                .enumerate()
                .map(|(team, faction)| PlayerConfig {
                    name: format!("P{team}"),
                    team: team as i32,
                    faction,
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
    // Source fixture stops createGame immediately after authored structure setup.
    m.world = World::from_map(&map).unwrap();
    m.building_cells.clear();
    m.commands = MatchCommands::new(4096);
    m.combat = Default::default();
    m.firing = Default::default();
    m.abilities = Default::default();
    m.field_works = Default::default();
    m.world.game.players = (0..2)
        .map(|slot| Player {
            slot,
            team: slot as i32,
            visible: Vec::new(),
        })
        .collect();
    m.world.game.tick = 10;
    m.world.game.wet = Some(0.);
    m.world.game.rain = 0.;
    m.start.environment_seed = 123;
    for p in &mut m.start.economy.players {
        p.mp = 500.;
    }
    let catalog = Catalog::compiled().unwrap();
    for row in case["units"].as_array().unwrap() {
        let id = row["id"].as_u64().unwrap() as u32;
        let owner = row["owner"].as_u64().unwrap_or(0) as usize;
        let kind = m.rules.kind(row["type"].as_str().unwrap()).unwrap();
        let mut u =
            unit_spawn::initial_unit(&m.rules, catalog, kind, id, owner, point(row)).unwrap();
        u.still = Some(0.);
        m.commands
            .insert(id, UnitControl::new(&u, &m.rules).unwrap());
        m.commands.units.get_mut(&id).unwrap().follow = 77;
        m.firing.initialize(id);
        m.combat.initialize(id);
        m.abilities.initialize(id);
        m.world.game.units.insert(id, u);
        prime(&mut m, id, row);
    }
    m.grid.rebuild(&m.world.game.units);
    m.initialize_recipient_routing().unwrap();
    m
}
fn prime(m: &mut Match, id: u32, v: &Value) {
    for (key, value) in v.as_object().unwrap() {
        let u = m.world.game.units.get_mut(&id).unwrap();
        match key.as_str() {
            "x" => u.x = value.as_f64().unwrap(),
            "z" => u.z = value.as_f64().unwrap(),
            "rot" => u.rot = value.as_f64().unwrap(),
            "hp" => u.hp = value.as_f64().unwrap(),
            "supp" => u.supp = value.as_f64().unwrap(),
            "repath" => u.repath = value.as_f64().unwrap(),
            "retreating" => u.retreating = value.as_bool().unwrap(),
            "board" => u.board = value.as_u64().unwrap() as u32,
            "path" => {
                u.path = value.as_array().unwrap().iter().map(point).collect();
                u.path_gen += 1;
            }
            "cargo" => {
                m.combat.get_mut(id).unwrap().cargo = value
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_u64().unwrap() as u32)
                    .collect()
            }
            "riding" => {
                let c = value.as_u64().unwrap() as u32;
                u.riding = c != 0;
                m.combat.get_mut(id).unwrap().carrier = (c != 0).then_some(c);
            }
            "id" | "type" | "owner" => {}
            _ => panic!("unknown fixture prime {key}"),
        }
    }
}
fn snapshot(m: &Match) -> Value {
    let units=m.world.game.units.values().map(|u| {
        let c=&m.commands.units[&u.id];let meta=m.combat.get(u.id).unwrap();let f=m.firing.get(u.id).unwrap();
        let flags=if u.riding {262144}else{0}|if meta.cargo.is_empty(){0}else{524288|((meta.cargo.len()as u32)<<23)};
        json!({"id":u.id,"kind":m.rules.kinds[u.kind].name,"owner":u.owner,"pose":[bits(u.x),bits(u.z),bits(u.rot)],"hp":bits(u.hp),"supp":bits(u.supp),"still":bits(u.still.unwrap_or(0.)),"cd":bits(m.abilities.get(u.id).unwrap().cooldown),"cooldown":bits(f.cooldown),"retarget":bits(f.retarget),"repath":bits(u.repath),"board":u.board,"riding":meta.carrier.unwrap_or(0),"cargo":meta.cargo,"path":u.path.iter().map(|p|pt(Some(*p))).collect::<Vec<_>>(),"follow":c.follow,"attack":c.attack_id,"target":f.target_id,"work":u.dig,"orders":c.orders.len(),"face":c.face.is_some(),"nade":pt(c.nade),"enter":c.enter.map_or(-1,|v|v as i64),"fireAt":c.fire_at.map_or(-1,|v|v as i64),"build":u.build,"entrench":u.entrench,"worldGoal":pt(u.world_goal),"amove":pt(u.amove),"traffic":u.traffic.is_some(),"trafficWait":bits(u.traffic_wait),"outcome":c.outcome.map(|o|match o {MoveOutcome::Interrupted=>"interrupted",MoveOutcome::Moving=>"moving",MoveOutcome::Unreachable=>"unreachable",MoveOutcome::Arrived=>"completed"}),"outcomeTick":c.outcome_tick,"flags":flags,"grid":m.grid.in_order(&m.world.game.units,(u.x,u.z),6.,|_,_|true)})
    }).collect::<Vec<_>>();
    json!({"units":units,"money":m.start.economy.players.iter().map(|p|bits(p.mp)).collect::<Vec<_>>(),"shots":[]})
}
fn ids(value: &Value) -> Vec<i64> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_i64().unwrap())
        .collect()
}
#[test]
fn source_commands_boarding_riding_and_unload_match_each_checkpoint() {
    let f = fixture();
    let mut actions = 0;
    for c in f["traces"].as_array().unwrap() {
        let mut m = make(c);
        assert_eq!(snapshot(&m), c["initial"], "{} initial", c["name"]);
        for row in c["rows"].as_array().unwrap() {
            let a = &row["action"];
            let mut result = Value::Null;
            match a["op"].as_str().unwrap() {
                "command" => {
                    let cmd = &a["cmd"];
                    let owner = a["owner"].as_u64().unwrap_or(0) as usize;
                    let denied = match cmd["t"].as_str().unwrap() {
                        "board" => m
                            .carrier_board_command(
                                owner,
                                &ids(&cmd["ids"]),
                                cmd["target"].as_i64(),
                                false,
                            )
                            .unwrap(),
                        "unload" => m
                            .carrier_unload_command(owner, &ids(&cmd["ids"]), false)
                            .unwrap(),
                        _ => m
                            .execute_command(owner, &Command::from_json(cmd).unwrap(), false)
                            .unwrap(),
                    };
                    result = denied.map_or(Value::Null, |d| json!(d.name()));
                }
                "prime" => {
                    prime(&mut m, a["id"].as_u64().unwrap() as u32, &a["fields"]);
                    m.grid.rebuild(&m.world.game.units);
                }
                "mutate" => {
                    m.world
                        .set_cell(
                            a["cell"].as_u64().unwrap() as usize,
                            a["ch"].as_str().unwrap().as_bytes()[0],
                        )
                        .unwrap();
                }
                "early" => {
                    m.apply_unit_riding(
                        a["id"].as_u64().unwrap() as u32,
                        a["dt"].as_f64().unwrap_or(0.05),
                    )
                    .unwrap();
                }
                "phase" => {
                    result = match m
                        .apply_unit_boarding(a["id"].as_u64().unwrap() as u32)
                        .unwrap()
                    {
                        BoardingOutcome::Boarded { carrier } => json!({"boarded":carrier}),
                        BoardingOutcome::Cleared => json!("cleared"),
                        BoardingOutcome::NeedsPath { carrier, at } => {
                            json!({"needsPath":carrier,"at":pt(Some(at))})
                        }
                        BoardingOutcome::Idle => json!("idle"),
                    };
                }
                op => panic!("unknown carrier action {op}"),
            }
            assert_eq!(
                result, row["result"],
                "{} {} action {a}",
                c["name"], actions
            );
            assert_eq!(
                snapshot(&m),
                row["snapshot"],
                "{} {} action {a}",
                c["name"],
                actions
            );
            actions += 1;
        }
    }
    assert_eq!(actions, 66);
}
fn atomic(m: &Match) -> String {
    format!(
        "{:?}{:?}{:?}{:?}{:?}{:?}{:?}",
        m.world,
        m.combat,
        m.firing,
        m.commands.units,
        m.field_works,
        m.start.economy,
        m.start.random.snapshot()
    )
}
#[test]
fn invalid_or_unavailable_carrier_work_preserves_canonical_state() {
    let f = fixture();
    let c = &f["traces"][0];
    for guard in [
        "path capacity",
        "later position",
        "missing metadata",
        "wrong owner",
        "cargo identity",
        "job activity",
        "other mode",
    ] {
        let mut m = make(c);
        match guard {
            "path capacity" => m.world.game.units.get_mut(&1).unwrap().path_gen = u64::MAX,
            "later position" => m.world.game.units.get_mut(&2).unwrap().x = f64::NAN,
            "missing metadata" => {
                m.combat.remove(2);
            }
            "wrong owner" => m.world.game.units.get_mut(&2).unwrap().owner = usize::MAX,
            "cargo identity" => m.combat.get_mut(2).unwrap().cargo = vec![999, 999],
            "job activity" => m.world.game.units.get_mut(&1).unwrap().dig = true,
            "other mode" => m.options.mode = Mode::World,
            _ => unreachable!(),
        }
        let before = atomic(&m);
        assert!(
            m.carrier_board_command(0, &[1], Some(2), false).is_err(),
            "{guard}"
        );
        assert_eq!(atomic(&m), before, "{guard}");
    }
    for field in ["garrison", "build", "entrench", "boat", "rail"] {
        let mut m = make(c);
        match field {
            "garrison" => m.world.game.units.get_mut(&1).unwrap().garrison = Some(0),
            "build" => m.world.game.units.get_mut(&1).unwrap().build = true,
            "entrench" => m.world.game.units.get_mut(&1).unwrap().entrench = true,
            "boat" => m.world.game.units.get_mut(&2).unwrap().kind = m.rules.kind("lcvp").unwrap(),
            "rail" => m.world.game.units.get_mut(&2).unwrap().rail = true,
            _ => unreachable!(),
        }
        let before = atomic(&m);
        assert_eq!(
            m.carrier_board_command(0, &[1], Some(2), false).unwrap(),
            Some(Denial::Unsupported),
            "{field}"
        );
        assert_eq!(atomic(&m), before, "{field}");
    }
    let mut m = make(c);
    m.carrier_board_command(0, &[1], Some(2), false).unwrap();
    m.apply_unit_boarding(1).unwrap();
    m.world.game.units.get_mut(&2).unwrap().hp = 0.;
    let before = atomic(&m);
    assert_eq!(
        m.apply_unit_riding(1, 0.05).unwrap(),
        RidingOutcome::UnsupportedEjection
    );
    assert_eq!(atomic(&m), before);
}
