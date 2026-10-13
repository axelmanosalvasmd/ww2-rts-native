use serde_json::{Value, json};
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
    match e {
        LaunchEvent::Flight {
            id,
            kind,
            effect,
            shooter,
            owner,
            at,
            velocity,
            gravity,
            time,
            expires,
        } => {
            let mut v = json!({"k":"flight","kind":m.rules.kinds[kind].name,"flight":id,"seq":0,"f":shooter,"fo":owner,"x":at.x,"y":at.y,"z":at.z,"v":[velocity.x,velocity.y,velocity.z],"gravity":gravity,"time":time,"expires":expires,"local":true});
            if let Some(effect) = effect {
                v["effect"] = json!(effect);
            }
            v
        }
        LaunchEvent::Salvo {
            shooter,
            owner,
            at,
            count,
            spread,
        } => {
            json!({"f":shooter,"fo":owner,"x":at.x,"z":at.z,"k":"salvo","n":count,"r":spread,"physical":true,"local":true})
        }
    }
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
                u.shot_at
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
    json!({"units":units,"claims":claims,"flights":flights,"shots":shots,"salvos":salvos})
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
    let fixture: Value = serde_json::from_str(include_str!("fixtures/match-firing.json")).unwrap();
    let mut phases = 0;
    for c in fixture["cases"].as_array().unwrap() {
        let mut m = make(c);
        for s in c["steps"].as_array().unwrap() {
            m.world.game.tick = s["tick"].as_i64().unwrap();
            m.begin_firing_tick(&[1, 2, 3]).unwrap();
            m.advance_firing_timers(1, 0.05).unwrap();
            m.apply_unit_firing(1, c["variation"] == "moving", 0.05)
                .unwrap();
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
    assert_eq!(phases, 264);
}
