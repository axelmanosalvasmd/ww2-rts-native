use serde_json::{Value, json};
use std::io::Read;
use ww2_core::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::{Point, Unit},
    map::AuthoritativeMap,
    match_combat::CombatState,
    match_commands::UnitControl,
    match_firing::{FiringState, LaunchEvent},
    match_runtime::{Match, MatchOptions, PlayerConfig},
    random::StreamId,
    unit_spawn,
    weather::WeatherChoice,
};
fn fixture() -> Value {
    let mut text = String::new();
    flate2::read::GzDecoder::new(&include_bytes!("fixtures/match-firing.json.gz")[..])
        .read_to_string(&mut text)
        .unwrap();
    serde_json::from_str(&text).unwrap()
}
fn bits(n: f64) -> Value {
    json!(format!("{:016x}", n.to_bits()))
}
fn vector(p: ww2_core::projectiles::Point3) -> Value {
    json!([bits(p.x), bits(p.y), bits(p.z)])
}
fn point(v: &Value) -> Point {
    Point {
        x: v["x"].as_f64().unwrap(),
        z: v["z"].as_f64().unwrap(),
    }
}
fn make(c: &Value) -> Match {
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&c["map"]).unwrap()).unwrap();
    let mut m = Match::new(
        &map,
        MatchOptions {
            seed: 701,
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
            supply: true,
            logistics: false,
            sandbox: false,
        },
    )
    .unwrap();
    m.world.game.units.clear();
    m.start.environment_seed = 123;
    m.start.wind = (0.0, 0.5);
    m.next_unit_id = 1000;
    m.world.game.rain = 0.0;
    m.world.game.wet = Some(0.0);
    m.firing = FiringState::default();
    m.combat = CombatState::default();
    m.commands.units.clear();
    for v in c["input"].as_array().unwrap() {
        let id = v["id"].as_u64().unwrap() as u32;
        let kind = m
            .rules
            .kinds
            .iter()
            .position(|k| k.name == v["kind"].as_str().unwrap())
            .unwrap();
        let mut u = unit_spawn::initial_unit(
            &m.rules,
            ww2_core::catalog::Catalog::compiled().unwrap(),
            kind,
            id,
            v["owner"].as_u64().unwrap() as usize,
            point(v),
        )
        .unwrap();
        u.rot = v["rot"].as_f64().unwrap();
        u.hp = v["hp"].as_f64().unwrap();
        u.still = v["still"].as_f64();
        u.supp = v["supp"].as_f64().unwrap();
        u.shot_at = v["shotAt"].as_f64();
        u.hit_from = (!v["hitFrom"].is_null()).then(|| point(&v["hitFrom"]));
        let control = UnitControl::new(&u, &m.rules).unwrap();
        m.commands.insert(id, control);
        let control = m.commands.units.get_mut(&id).unwrap();
        control.aim = v["aim"].as_f64().unwrap();
        control.hold_fire = v["hold"].as_bool().unwrap_or(false);
        control.attack_id = v["attack"].as_u64().unwrap() as u32;
        let state = m.firing.initialize(id);
        state.target_id = v["target"].as_u64().unwrap() as u32;
        state.cooldown = v["cooldown"].as_f64().unwrap();
        state.retarget = v["retarget"].as_f64().unwrap();
        let meta = m.combat.initialize(id);
        meta.xp = v["xp"].as_f64().unwrap();
        meta.hit_at = v["hitAt"].as_f64().unwrap();
        meta.hit_by = v["hitBy"].as_u64().map(|n| n as u32);
        if !v["logistics"].is_null() {
            let r = &v["logistics"];
            m.logistics.insert(
                id,
                ww2_core::logistics::Reserves {
                    ammo: r["ammo"].as_f64().unwrap(),
                    ammo_max: 20.0,
                    ammo_price: 1.0,
                    ammo_credit: Some(0.0),
                    provisions: 120.0,
                    fuel: None,
                    emergency: None,
                    warning: None,
                    forced: false,
                    stranded: false,
                    exhausted: false,
                    supply_rate: None,
                    cut_for: None,
                },
            );
        }
        m.world.game.units.insert(id, u);
    }
    for v in c["cells"].as_array().unwrap() {
        m.world
            .set_cell(
                v[0].as_u64().unwrap() as usize,
                v[1].as_str().unwrap().as_bytes()[0],
            )
            .unwrap();
    }
    m.world.game.players[0].visible = c["visible"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n.as_u64().unwrap() as u32)
        .collect();
    m.world.game.players[1].visible = vec![1];
    m.grid.rebuild(&m.world.game.units);
    m
}
fn launch(e: LaunchEvent, m: &Match) -> Value {
    e.to_json(&m.rules)
}
fn snapshot(m: &mut Match) -> Value {
    let units = m
        .world
        .game
        .units
        .values()
        .map(|u: &Unit| {
            let s = m.firing.get(u.id).unwrap();
            json!([
                u.id,
                s.target_id,
                bits(s.cooldown),
                bits(s.retarget),
                bits(m.commands.units[&u.id].aim),
                bits(u.rot),
                u.shot_at,
                bits(u.hp),
                bits(u.supp),
                bits(m.combat.get(u.id).unwrap().xp),
                m.combat.get(u.id).unwrap().hit_by,
                bits(m.combat.get(u.id).unwrap().hit_at),
                m.combat.get(u.id).unwrap().last_hit,
                u.hit_from.map(|p| json!({"x":p.x,"z":p.z})),
                m.combat
                    .get(u.id)
                    .unwrap()
                    .death_impulse
                    .map(|p| json!({"dir":p.direction,"impulse":p.magnitude}))
            ])
        })
        .collect::<Vec<_>>();
    let claims = m
        .firing
        .target_claims()
        .map(|((id, team), v)| json!([u64::from(id) * 8 + team as u64, bits(v)]))
        .collect::<Vec<_>>();
    let flights=m.firing.flights.values().map(|p|json!({"id":p.id,"shooter":p.shooter,"owner":p.owner,"kind":m.rules.kinds[p.source_kind].name,"origin":vector(p.solution.origin),"velocity":vector(p.solution.velocity),"gravity":bits(p.solution.gravity),"stamp":bits(p.solution.stamp),"launched":bits(p.launched),"last":bits(p.last),"expires":bits(p.expires),"energy":bits(p.energy),"sequence":p.sequence,"maxContacts":p.max_contacts,"near":bits(p.near),"startCell":p.start_cell,"intended":p.intended,"damageScale":bits(p.damage_scale),"suppression":bits(p.suppression),"suppressionScale":bits(p.suppression_scale),"ap":p.armor_piercing,"apMultiplier":bits(p.ap_multiplier),"apNaval":p.ap_naval,"explosive":p.explosive,"terrain":bits(p.terrain),"scar":bits(p.scar),"blastRadius":p.blast_radius.map(bits),"blastSource":p.blast_source,"effect":p.effect,"sent":p.sent})).collect::<Vec<_>>();
    let shots = m
        .firing
        .take_events()
        .into_iter()
        .map(|e| launch(e, m))
        .collect::<Vec<_>>();
    let salvos=m.firing.salvos.iter().map(|s|json!({"owner":s.owner,"at":[bits(s.at.x),bits(s.at.z)],"left":s.left,"next":bits(s.next),"every":bits(s.every),"spread":bits(s.spread)})).collect::<Vec<_>>();
    let sparse = |a: &[f32]| {
        a.iter()
            .enumerate()
            .filter(|(_, v)| **v != 0.0)
            .map(|(i, v)| json!([i, bits(f64::from(*v))]))
            .collect::<Vec<_>>()
    };
    let terrain = json!({"chars":String::from_utf8(m.world.game.chars.clone()).unwrap(),"hp":sparse(&m.world.game.cell_hp),"wear":sparse(m.world.game.wear.as_deref().unwrap()),"scar":sparse(&m.world.scar),"burnt":m.world.burnt.iter().enumerate().filter(|(_,v)|**v!=0).map(|(i,v)|json!([i,v])).collect::<Vec<_>>(),"versions":[m.world.game.terrain_version as u64,m.world.versions.obstruction,m.world.versions.infantry,m.world.versions.vehicle,m.world.versions.naval]});
    let retained = m
        .combat
        .iter()
        .filter(|(id, _)| {
            !m.world.game.units.contains_key(id)
                && m.firing.flights.values().any(|p| p.shooter == *id)
        })
        .map(|(id, c)| json!([id, bits(c.xp)]))
        .collect::<Vec<_>>();
    let ammo = m
        .world
        .game
        .units
        .keys()
        .filter_map(|id| m.logistics.get(*id).map(|r| json!([id, bits(r.ammo)])))
        .collect::<Vec<_>>();
    json!({"ammo":ammo,"retained":retained,"environment":[m.start.environment_seed,m.next_unit_id],"terrain":terrain,"units":units,"claims":claims,"flights":flights,"shots":shots,"salvos":salvos})
}
fn compare(a: &Value, b: &Value, path: &str) {
    if b.is_number() {
        assert_eq!(
            a.as_f64().unwrap().to_bits(),
            b.as_f64().unwrap().to_bits(),
            "{path}"
        );
        return;
    }
    match b {
        Value::Array(b) => {
            let a = a.as_array().unwrap();
            assert_eq!(a.len(), b.len(), "{path}");
            for (i, b) in b.iter().enumerate() {
                compare(&a[i], b, &format!("{path}[{i}]"));
            }
        }
        Value::Object(b) => {
            let a = a.as_object().unwrap();
            assert_eq!(a.len(), b.len(), "{path}");
            for (k, b) in b {
                compare(
                    a.get(k).unwrap_or_else(|| panic!("missing {path}.{k}")),
                    b,
                    &format!("{path}.{k}"),
                );
            }
        }
        _ => assert_eq!(a, b, "{path}"),
    }
}
#[test]
fn assembled_match_targeting_and_launches_match_original_source() {
    let fixture: Value = fixture();
    let mut phases = 0;
    for c in fixture["cases"].as_array().unwrap() {
        let mut m = make(c);
        let run = c["variation"].as_str().unwrap().starts_with("flight");
        let mut last = 0;
        for s in c["steps"].as_array().unwrap() {
            let tick = s["tick"].as_i64().unwrap();
            for current in if run { last + 1..=tick } else { tick..=tick } {
                m.world.game.tick = current - 1;
                m.capture_firing_prior().unwrap();
                m.world.game.tick = current;
                if run && current == 2 {
                    if c["variation"] == "flight-wall" {
                        m.world
                            .set_cell(c["lateWall"].as_u64().unwrap() as usize, b'B')
                            .unwrap();
                    }
                    if c["variation"] == "flight-bounce" {
                        let z = m.world.game.units[&1].z;
                        let t = m.world.game.units.get_mut(&2).unwrap();
                        t.x = 35.0;
                        t.z = z + 1.18;
                        m.world.game.tick = current - 1;
                        m.capture_firing_prior().unwrap();
                        m.world.game.tick = current;
                        m.grid.rebuild(&m.world.game.units);
                        for p in m.firing.flights.values_mut() {
                            p.solution.velocity.y = 0.0;
                        }
                    }
                    if c["variation"] == "flight-dead-shooter" {
                        m.world.game.units.remove(&1);
                        m.firing.remove(1);
                        m.commands.units.remove(&1);
                        m.grid.rebuild(&m.world.game.units);
                    }
                    if c["variation"] == "flight-expiry" {
                        for p in m.firing.flights.values_mut() {
                            p.solution.velocity.y = 80.0;
                        }
                    }
                }
                if c["variation"] == "flight-motion" && current > 1 {
                    let t = m.world.game.units.get_mut(&2).unwrap();
                    t.z += 0.15;
                    t.rot += 0.025;
                    m.grid.rebuild(&m.world.game.units);
                }
                m.begin_firing_tick(&[1, 2, 3]).unwrap();
                m.advance_firing_timers(1, 0.05).unwrap();
                if !run || current == 1 {
                    m.apply_unit_firing(1, c["variation"] == "moving", 0.05)
                        .unwrap();
                }
                if run {
                    m.step_firing_flights().unwrap();
                    m.advance_firing_salvos(0.05).unwrap();
                }
            }
            last = tick;
            compare(
                &snapshot(&mut m),
                &s["expected"],
                &format!("{} {} tick {}", c["shooter"], c["variation"], s["tick"]),
            );
            let state = m
                .start
                .random
                .snapshot()
                .streams
                .into_iter()
                .find(|(id, _)| *id == StreamId::Combat)
                .map(|(_, v)| v);
            if let Some(state) = state {
                assert_eq!(format!("{state:016x}"), s["random"].as_str().unwrap());
            }
            phases += 1;
        }
    }
    assert_eq!(phases, 588);
}

#[test]
fn unsupported_modes_and_later_bad_payloads_leave_firing_unchanged() {
    let fixture: Value = fixture();
    let c = &fixture["cases"][0];
    let mut m = make(c);
    m.world.game.tick = 1;
    m.begin_firing_tick(&[1, 2, 3]).unwrap();
    m.advance_firing_timers(1, 0.05).unwrap();
    m.apply_unit_firing(1, false, 0.05).unwrap();
    assert!(!m.firing.flights.is_empty());
    let first = m.firing.flights.values().next().unwrap().clone();
    let mut invalid = first.clone();
    invalid.id = 100;
    invalid.near = -1.0;
    m.firing.flights.insert(100, invalid);
    let before = m.firing.clone();
    let world = m.world.game.clone();
    let combat = m.combat.clone();
    let random = m.start.random.snapshot();
    m.world.game.tick = 2;
    assert!(m.step_firing_flights().is_err());
    assert_eq!(m.firing, before);
    assert_eq!(m.combat, combat);
    assert_eq!(m.start.random.snapshot(), random);
    m.world.game.tick = 1;
    assert_eq!(m.world.game.units, world.units);
    assert_eq!(m.world.game.chars, world.chars);
    m.options.mode = Mode::World;
    m.world.game.units.get_mut(&1).unwrap().owner = u32::MAX as usize;
    let before = m.firing.clone();
    assert!(m.begin_firing_tick(&[1, 2, 3]).is_err());
    assert!(m.advance_firing_timers(1, 0.05).is_err());
    assert!(m.apply_unit_firing(1, false, 0.05).is_err());
    assert!(m.can_shoot_target(1, 2).is_err());
    assert_eq!(m.firing, before);
}

#[test]
fn equal_fraction_contacts_use_lower_id_despite_grid_history() {
    let fixture = fixture();
    let c = &fixture["cases"][0];
    for reverse in [false, true] {
        let mut m = make(c);
        m.world.game.tick = 1;
        m.begin_firing_tick(&[1, 2, 3]).unwrap();
        m.apply_unit_firing(1, false, 0.05).unwrap();
        let first = *m.firing.flights.keys().next().unwrap();
        m.firing.flights.retain(|id, _| *id == first);
        let p = m.firing.flights.get_mut(&first).unwrap();
        p.solution.velocity = ww2_core::projectiles::Point3::new(230.0, 0.0, 0.0);
        p.near = 0.0;
        let kind = m.world.game.units[&2].kind;
        for id in [2, 3] {
            let t = m.world.game.units.get_mut(&id).unwrap();
            t.kind = kind;
            t.x = 35.0;
            t.z = 15.0;
            t.rot = 0.0;
            t.hp = 100.0;
        }
        m.grid.rebuild(&m.world.game.units);
        if reverse {
            m.grid.remove_unit(2);
            m.grid.update(2, &m.world.game.units[&2]);
        }
        let order = m
            .grid
            .in_order(&m.world.game.units, (35.0, 15.0), 1.0, |id, _| id != 1);
        assert_eq!(order, if reverse { vec![3, 2] } else { vec![2, 3] });
        m.capture_firing_prior().unwrap();
        m.world.game.tick = 2;
        m.step_firing_flights().unwrap();
        let contact = m
            .firing
            .take_events()
            .into_iter()
            .find_map(|e| match e {
                LaunchEvent::Contact(c) => Some(c),
                _ => None,
            })
            .unwrap();
        assert_eq!(contact.target, Some(2));
        assert!(m.world.game.units[&2].hp < 100.0);
        assert_eq!(m.world.game.units[&3].hp, 100.0);
    }
}
