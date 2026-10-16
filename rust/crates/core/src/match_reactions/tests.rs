use crate::{
    catalog::Catalog,
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::Point,
    map::AuthoritativeMap,
    match_commands::{Command, UnitControl},
    match_runtime::{Match, MatchOptions, PlayerConfig},
    unit_spawn,
    weather::WeatherChoice,
    world::World,
};
use serde_json::{Value, json};
fn bits(n: f64) -> Value {
    json!(format!("{:016x}", n.to_bits()))
}
fn point(v: &Value) -> Point {
    Point {
        x: v["x"].as_f64().unwrap_or(f64::NAN),
        z: v["z"].as_f64().unwrap_or(f64::NAN),
    }
}
fn optional(v: &Value) -> Option<Point> {
    (!v.is_null()).then(|| point(v))
}
fn truth(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::Number(v) => v.as_f64().unwrap() != 0.0,
        _ => true,
    }
}
fn packed(v: Option<Point>) -> Value {
    v.map_or(Value::Null, |p| json!([bits(p.x), bits(p.z)]))
}
fn fixture() -> Value {
    serde_json::from_str(include_str!("../../tests/fixtures/match-reactions.json")).unwrap()
}
fn make(f: &Value, c: &Value) -> Match {
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&f["map"]).unwrap()).unwrap();
    let mut m = Match::new(
        &map,
        MatchOptions {
            seed: 71,
            mode: Mode::Conquest,
            players: (0..2)
                .map(|team| PlayerConfig {
                    name: format!("P{team}"),
                    team,
                    faction: Faction::try_from(team as u8).unwrap(),
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
    // This oracle uses authored terrain without the startup buildings. Replace
    // that terrain and retain the constructor's canonical player query rows.
    let players = m.world.game.players.clone();
    m.world = World::from_map(&map).unwrap();
    m.world.game.players = players;
    m.world.game.tick = 100;
    m.building_cells.clear();
    m.commands.units.clear();
    m.firing = Default::default();
    m.combat = Default::default();
    for (p, v) in m
        .start
        .players
        .iter_mut()
        .zip(c["spawns"].as_array().unwrap())
    {
        p.spawn = point(v);
    }
    for v in c["input"].as_array().unwrap() {
        let id = v["id"].as_u64().unwrap() as u32;
        let kind = m.rules.kind(v["kind"].as_str().unwrap()).unwrap();
        let mut u = unit_spawn::initial_unit(
            &m.rules,
            Catalog::compiled().unwrap(),
            kind,
            id,
            v["owner"].as_u64().unwrap() as usize,
            point(v),
        )
        .unwrap();
        u.rot = v["rot"].as_f64().unwrap();
        u.hp = v["hp"].as_f64().unwrap();
        u.built = v["built"].as_f64();
        u.hit_from = optional(&v["hitFrom"]);
        u.path = v["path"].as_array().unwrap().iter().map(point).collect();
        u.world_goal = optional(&v["worldGoal"]);
        u.amove = optional(&v["amove"]);
        u.reverse = optional(&v["reverse"]);
        u.drift = truth(&v["drift"]);
        u.retreating = truth(&v["retreating"]);
        u.attack = v["attack"].as_u64().unwrap() != 0;
        u.hold_pos = truth(&v["holdPos"]);
        u.garrison = v["garrison"]
            .as_i64()
            .filter(|n| *n >= 0)
            .map(|n| n as usize);
        u.board = v["board"].as_u64().unwrap() as u32;
        u.riding = truth(&v["riding"]);
        u.build = truth(&v["build"]);
        u.dig = truth(&v["dig"]);
        u.entrench = truth(&v["entrench"]);
        let mut control = UnitControl::new(&u, &m.rules).unwrap();
        control.build_target = v["build"]
            .as_u64()
            .filter(|id| *id != 0)
            .map(|id| u32::try_from(id).unwrap());
        control.react = v["react"].as_f64().unwrap();
        control.hold_fire = truth(&v["holdFire"]);
        control.auto_retreat = truth(&v["autoRetreat"]);
        control.attack_id = v["attack"].as_u64().unwrap() as u32;
        control.nade = optional(&v["nade"]);
        control.enter = v["enter"].as_i64().filter(|n| *n >= 0).map(|n| n as usize);
        control.fire_at = v["fireAt"].as_i64().filter(|n| *n >= 0).map(|n| n as usize);
        control.rally_drift = v["drift"] == "rally";
        for _ in v["orders"].as_array().unwrap() {
            control.orders.push_back(Command::Unsupported);
        }
        m.commands.insert(id, control);
        m.firing.initialize(id).target_id = v["target"].as_u64().unwrap() as u32;
        let meta = m.combat.initialize(id);
        meta.hit_at = v["hitAt"].as_f64().unwrap();
        meta.hit_by = v["hitBy"].as_u64().filter(|n| *n != 0).map(|n| n as u32);
        m.world.game.units.insert(id, u);
    }
    m.world.game.players[0].visible = c["visible"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_u64().unwrap() as u32)
        .collect();
    for owner in 0..2 {
        m.world
            .register_recipient(crate::world::Recipient::Player(owner));
    }
    m.grid.rebuild(&m.world.game.units);
    m.initialize_recipient_routing().unwrap();
    let ids = m.world.game.units.keys().copied().collect::<Vec<_>>();
    m.begin_firing_tick(&ids).unwrap();
    m
}
fn snapshot(m: &Match, id: u32) -> Value {
    let u = &m.world.game.units[&id];
    let c = &m.commands.units[&id];
    json!({"path":u.path.iter().map(|p|packed(Some(*p))).collect::<Vec<_>>(),"worldGoal":packed(u.world_goal),"amove":packed(u.amove),"reverse":packed(u.reverse),"react":bits(c.react),"drift":u.drift,"target":m.firing.get(id).unwrap().target_id,"attack":c.attack_id,"retreating":u.retreating,"orders":c.orders.len(),"garrison":u.garrison.map_or(-1,|n|n as i64),"holdFire":c.hold_fire,"holdPos":u.hold_pos,"autoRetreat":c.auto_retreat,"nade":packed(c.nade),"enter":c.enter.map_or(-1,|n|n as i64),"fireAt":c.fire_at.map_or(-1,|n|n as i64),"board":u.board,"build":u.build,"dig":u.dig,"entrench":u.entrench})
}
#[test]
fn source_reactions_retreat_and_spacing_match_canonical_state() {
    let f = fixture();
    assert_eq!(f["reference"], "e4e579a838aabae3b1d9dadbc17a0a87147bc52d");
    assert_eq!(f["cases"].as_array().unwrap().len(), 51);
    for c in f["cases"].as_array().unwrap() {
        let mut m = make(&f, c);
        let id = c["unit"].as_u64().unwrap() as u32;
        if c["name"] == "blocked-riding" {
            let before = snapshot(&m, id);
            assert!(m.react_unit(id).is_err());
            assert_eq!(snapshot(&m, id), before);
            continue;
        }
        match c["action"].as_str().unwrap() {
            "home" => {}
            "retreat" => m.retreat_unit(id).unwrap(),
            "automatic" => {
                m.automatic_retreat_unit(id).unwrap();
            }
            _ => m.react_unit(id).unwrap(),
        }
        let mut expected = c["expected"].clone();
        expected["drift"] = json!(truth(&expected["drift"]));
        assert_eq!(snapshot(&m, id), expected, "{}", c["name"]);
        assert_eq!(
            packed(Some(m.reaction_home(id).unwrap())),
            c["home"],
            "{} home",
            c["name"]
        );
        let claims = m
            .firing
            .target_claims()
            .map(|((id, team), value)| json!([u64::from(id) * 8 + team as u64, bits(value)]))
            .collect::<Vec<_>>();
        assert_eq!(json!(claims), c["claims"], "{} claims", c["name"]);
    }
}

#[test]
fn completed_recruit_rally_does_not_relabel_a_later_cover_route() {
    let f: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/rally-drift.json")).unwrap();
    let input = f["initial"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| {
            let mut row = json!({"hitAt":-1e9,"hitBy":0,"hitFrom":null,"path":[],
                "worldGoal":null,"amove":null,"reverse":null,"react":0,"drift":false,
                "target":0,"attack":0,"retreating":false,"orders":[],"garrison":-1,
                "holdFire":false,"holdPos":false,"autoRetreat":true,"nade":null,
                "enter":-1,"fireAt":-1,"board":0,"riding":false,"build":false,
                "dig":false,"entrench":false});
            for (key, value) in v.as_object().unwrap() {
                row[key] = value.clone();
            }
            row
        })
        .collect::<Vec<_>>();
    let mut m = make(
        &f,
        &json!({"input":input,"spawns":f["spawns"],"visible":[f["shooter"]]}),
    );
    m.abilities = Default::default();
    m.recovery = Default::default();
    m.logistics = Default::default();
    m.recruit_metadata.clear();
    m.next_unit_id = f["id"].as_u64().unwrap() as u32;
    m.start.economy.players[0].mp = 1000.0;
    m.world.game.reveal = true;
    let id = m.next_unit_id;
    let maker = f["maker"].as_u64().unwrap() as u32;
    // The authored Barracks replaces the constructor's producers. Source gives
    // it an empty queue and zero progress before the actual Buy.
    m.production.initialize_building(maker).unwrap();
    m.building_rallies.insert(maker, Some(point(&f["rally"])));
    let command = Command::Buy {
        kind: "rifle".into(),
        from: Some(maker),
    };
    assert!(m.execute_command(0, &command, false).unwrap().is_none());
    let project = |m: &Match| {
        let u = &m.world.game.units[&id];
        let c = &m.commands.units[&id];
        // The shared core uses a boolean presence and a rally subtype. Other
        // reaction subtypes are not represented by the current Unit model.
        json!({"at":packed(Some(u.at())),"path":u.path.iter().map(|p|packed(Some(*p))).collect::<Vec<_>>(),
            "goal":packed(u.world_goal),"amove":packed(u.amove),
            "drift":if u.drift && c.rally_drift {json!("rally")} else {json!(u.drift)},
            "target":m.firing.get(id).unwrap().target_id})
    };
    let compare = |m: &Match, stage: usize| {
        let mut expected = f["stages"][stage]["expected"].clone();
        if expected["drift"] == "cover" {
            expected["drift"] = json!(true);
        }
        assert_eq!(project(m), expected, "{}", f["stages"][stage]["name"]);
    };
    compare(&m, 0);
    for _ in 0..f["movements"].as_u64().unwrap() {
        m.world.game.tick += 1;
        m.begin_path_tick(4096, 4).unwrap();
        m.move_unit_with_command_routes(id, 1.0).unwrap();
    }
    assert_eq!(m.world.game.tick, f["finalTick"].as_i64().unwrap());
    assert!(m.world.game.units[&id].path.is_empty());
    m.prepare_unit_commands(id, false).unwrap();
    compare(&m, 1);
    let shooter = f["shooter"].as_u64().unwrap() as u32;
    let threat = point(&f["threat"]);
    let enemy = m.world.game.units.get_mut(&shooter).unwrap();
    enemy.x = threat.x;
    enemy.z = threat.z;
    m.grid.update(shooter, enemy);
    m.world.game.units.get_mut(&id).unwrap().hit_from = Some(threat);
    let hit = m.combat.get_mut(id).unwrap();
    hit.hit_at = m.world.game.tick as f64;
    hit.hit_by = Some(shooter);
    m.commands.units.get_mut(&id).unwrap().hold_fire = true;
    m.react_unit(id).unwrap();
    compare(&m, 2);
    assert!(!m.commands.units[&id].rally_drift);
    m.commands.units.get_mut(&id).unwrap().hold_fire = false;
    assert!(m.can_shoot_target(id, shooter).unwrap());
    m.react_unit(id).unwrap();
    compare(&m, 3);
}
#[test]
fn reaction_timer_and_unsupported_exit_preserve_canonical_state() {
    let f = fixture();
    let mut m = make(&f, &f["cases"][0]);
    let id = 1;
    m.decrement_reaction_timer(id, 0.05).unwrap();
    assert_eq!(m.commands.units[&id].react, -0.05);
    let before = snapshot(&m, id);
    assert!(m.decrement_reaction_timer(id, f64::NAN).is_err());
    assert_eq!(snapshot(&m, id), before);
    m.world.game.units.get_mut(&id).unwrap().riding = true;
    let before = snapshot(&m, id);
    assert!(m.retreat_unit(id).is_err());
    assert_eq!(snapshot(&m, id), before);
    m.world.game.units.get_mut(&id).unwrap().riding = false;
    for (build, entrench, dig) in [
        (true, false, false),
        (false, true, false),
        (false, false, true),
    ] {
        let u = m.world.game.units.get_mut(&id).unwrap();
        u.build = build;
        u.entrench = entrench;
        u.dig = dig;
        let before = snapshot(&m, id);
        assert!(m.retreat_unit(id).is_err());
        assert_eq!(snapshot(&m, id), before);
        assert!(!m.field_works.is_active(id));
    }
    m.world.game.units.get_mut(&id).unwrap().dig = false;
    m.world.game.units.get_mut(&id).unwrap().path_gen = u64::MAX;
    let before = snapshot(&m, id);
    assert!(m.react_unit(id).is_err());
    assert_eq!(snapshot(&m, id), before);
    assert_eq!(m.world.game.units[&id].path_gen, u64::MAX);
}

#[test]
fn invalid_neighbor_rejects_before_reaction_timer_or_route_changes() {
    let f = fixture();
    let mut m = make(&f, &f["cases"][0]);
    m.world.game.units.get_mut(&2).unwrap().path = vec![Point {
        x: f64::NAN,
        z: 25.0,
    }];
    let before = snapshot(&m, 1);
    assert!(m.react_unit(1).is_err());
    assert_eq!(snapshot(&m, 1), before);
    let case = f["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "idle-spacing")
        .unwrap();
    let mut m = make(&f, case);
    m.world.game.units.get_mut(&1).unwrap().kind = usize::MAX;
    let before = snapshot(&m, 2);
    assert!(m.react_unit(2).is_err());
    assert_eq!(snapshot(&m, 2), before);
}

#[test]
fn accepted_retreat_cancels_paid_work_after_route_preflight_without_refund() {
    let f = fixture();
    let mut m = make(&f, &f["cases"][0]);
    let id = 1;
    let u = m.world.game.units[&id].clone();
    m.start.economy.players[u.owner].mp = 1000.0;
    let prepared = m
        .field_works
        .prepare_start(
            &m.world,
            &mut m.start.economy.players[u.owner],
            crate::field_works::StartRequest {
                control_eligible: true,
                owner: u.owner,
                unit: id,
                kind: crate::field_works::FortKind::Trench,
                at: u.at(),
                direction: Some(0.0),
            },
            |_| true,
        )
        .unwrap();
    prepared.commit(&mut m.field_works);
    m.world.game.units.get_mut(&id).unwrap().dig = true;
    let paid = m.start.economy.players[u.owner].mp;
    let job = m.field_works.jobs()[&id].clone();
    m.world.game.units.get_mut(&id).unwrap().path_gen = u64::MAX;
    let before = snapshot(&m, id);
    assert!(m.retreat_unit(id).is_err());
    assert_eq!(snapshot(&m, id), before);
    assert_eq!(m.field_works.jobs()[&id], job);
    assert_eq!(m.start.economy.players[u.owner].mp, paid);
    m.world.game.units.get_mut(&id).unwrap().path_gen = 0;
    m.retreat_unit(id).unwrap();
    assert!(!m.field_works.is_active(id));
    assert!(!m.world.game.units[&id].dig);
    assert!(m.world.game.units[&id].retreating);
    assert_eq!(m.start.economy.players[u.owner].mp, paid);
}
