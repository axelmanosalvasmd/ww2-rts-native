use serde_json::{Value, json};
use std::io::Read;
use ww2_core::{
    catalog::Catalog,
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::Point,
    map::AuthoritativeMap,
    match_combat::CombatState,
    match_commands::UnitControl,
    match_firing::FiringState,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    match_support::{FighterCover, SupportDenial, SupportEvent, SupportState},
    random::StreamId,
    unit_spawn,
    weather::WeatherChoice,
};
fn bits(n: f64) -> Value {
    json!(format!("{:016x}", n.to_bits()))
}
fn fixture() -> Value {
    let mut text = String::new();
    flate2::read::GzDecoder::new(&include_bytes!("fixtures/match-support.json.gz")[..])
        .read_to_string(&mut text)
        .unwrap();
    serde_json::from_str(&text).unwrap()
}
fn make(t: &Value) -> Match {
    make_with_mode(t, Mode::Conquest)
}
fn make_with_mode(t: &Value, mode: Mode) -> Match {
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&t["map"]).unwrap()).unwrap();
    let mut m = Match::new(
        &map,
        MatchOptions {
            seed: 701,
            mode,
            players: vec![
                PlayerConfig {
                    name: "A".into(),
                    team: 0,
                    faction: Faction::try_from(t["faction"].as_u64().unwrap() as u8).unwrap(),
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
    .unwrap();
    m.world.game.units.clear();
    m.start.environment_seed = 123;
    m.start.wind = (0.0, 0.5);
    m.next_unit_id = 1000;
    m.world.game.rain = 0.0;
    m.world.game.wet = Some(0.0);
    m.combat = CombatState::default();
    m.firing = FiringState::default();
    m.commands.units.clear();
    m.logistics = Default::default();
    for row in t["units"].as_array().unwrap() {
        let id = row["id"].as_u64().unwrap() as u32;
        let kind = m.rules.kind(row["kind"].as_str().unwrap()).unwrap();
        let mut u = unit_spawn::initial_unit(
            &m.rules,
            Catalog::compiled().unwrap(),
            kind,
            id,
            row["owner"].as_u64().unwrap() as usize,
            Point {
                x: row["x"].as_f64().unwrap(),
                z: row["z"].as_f64().unwrap(),
            },
        )
        .unwrap();
        u.still = row["still"].as_f64();
        u.riding = row["riding"].as_u64().unwrap_or(0) > 0;
        m.commands
            .insert(id, UnitControl::new(&u, &m.rules).unwrap());
        m.commands.units.get_mut(&id).unwrap().hold_fire =
            row["holdFire"].as_bool().unwrap_or(false);
        let combat = m.combat.initialize(id);
        combat.hit_by = Some(0);
        combat.carrier = row["riding"]
            .as_u64()
            .filter(|id| *id > 0)
            .map(|id| id as u32);
        combat.cargo = row["cargo"]
            .as_array()
            .unwrap()
            .iter()
            .map(|id| id.as_u64().unwrap() as u32)
            .collect();
        m.firing.initialize(id).retarget = row["retarget"].as_f64().unwrap();
        if row["logistics"].is_object() {
            let r = &row["logistics"];
            m.logistics.insert(
                id,
                ww2_core::logistics::Reserves {
                    ammo: r["ammo"].as_f64().unwrap(),
                    ammo_max: r["ammoMax"].as_f64().unwrap(),
                    ammo_price: r["ammoPrice"].as_f64().unwrap(),
                    ammo_credit: r["ammoCredit"].as_f64(),
                    provisions: r["provisions"].as_f64().unwrap(),
                    fuel: r["fuel"].as_f64(),
                    emergency: r["emergency"].as_f64(),
                    warning: r["warning"].as_f64(),
                    forced: r["forced"].as_bool().unwrap(),
                    stranded: r["stranded"].as_bool().unwrap(),
                    exhausted: r["exhausted"].as_bool().unwrap(),
                    supply_rate: None,
                    cut_for: None,
                },
            );
        }
        m.world.game.units.insert(id, u);
    }
    if t["variation"] == "late-house" {
        m.world.sections.clear();
        m.world.structures.clear();
        m.world.game.sections.clear();
    }
    if let Some(c) = t["weakened"].as_u64() {
        m.world.game.cell_hp[c as usize] = 40.0;
        if t["variation"] == "late-bridge" {
            m.world.game.sections.get_mut(&(c as u32)).unwrap().hp = 40.0;
        }
    }
    m.grid.rebuild(&m.world.game.units);
    for p in &mut m.start.economy.players {
        p.mp = if t["variation"] == "poor" { 0.0 } else { 500.0 };
    }
    if t["variation"] == "cooldown" {
        m.start.players[0]
            .support
            .insert(t["kind"].as_str().unwrap().into(), 1.0);
    }
    if t["variation"] == "radio" || t["variation"] == "cut" {
        m.start.points[0].owner = Some(0);
        m.start.points[0].cut = t["variation"] == "cut";
    }
    if t["variation"] == "fighters" {
        m.support.covers.push(FighterCover {
            team: 1,
            owner: 1,
            at: Point { x: 41.0, z: 41.0 },
            radius: 40.0,
            remaining: 60.0,
        });
    }
    m
}
fn snapshot(m: &mut Match) -> Value {
    ww2_core::vision::update_vision(&mut m.world.game, &m.rules, &m.grid);
    let mut fog = ww2_core::fog::Fog::default();
    let fog_cells = m
        .world
        .game
        .players
        .iter()
        .map(|p| fog.team(&m.world.game, &m.rules, p.team).vis.clone())
        .collect::<Vec<_>>();
    let sparse = |a: &[f32]| {
        a.iter()
            .enumerate()
            .filter(|(_, v)| **v != 0.0)
            .map(|(i, v)| json!([i, bits(f64::from(*v))]))
            .collect::<Vec<_>>()
    };
    let shots = m
        .support
        .take_events()
        .into_iter()
        .map(|e| match e {
            SupportEvent::Chutes(p) => json!({"k":"chutes","x":p.x,"z":p.z,"pub":true}),
            SupportEvent::Bomb(p) => json!({"x":p.x,"z":p.z,"k":"bomb","pub":true}),
            SupportEvent::Shell(p) => json!({"x":p.x,"z":p.z,"k":"shell","pub":true}),
            SupportEvent::SmokeShells(p) => json!({"x":p.x,"z":p.z,"k":"smokeshells","pub":true}),
            SupportEvent::Plane {kind,at,direction,owner} => json!({"k":kind.name(),"x":at.x,"z":at.z,"dir":direction,"fo":owner,"pub":true}),
            SupportEvent::Flak {unit,owner,at} => json!({"f":unit,"fo":owner,"x":at.x,"z":at.z,"k":"flak","pub":true}),
            SupportEvent::ShotDown {kind,at,direction,fighters} => json!({"k":"shotdown","x":at.x,"z":at.z,"dir":direction,"kind":kind.name(),"by":if fighters {"fighters"} else {"flak"},"pub":true}),
            SupportEvent::Environment(e) => e.to_json(),
        })
        .collect::<Vec<_>>();
    let random = m
        .start
        .random
        .snapshot()
        .streams
        .into_iter()
        .find(|(k, _)| *k == StreamId::OffMap)
        .map(|(_, v)| format!("{v:016x}"));
    // An unused purpose has the same initial state as the original fixture's untouched stream.
    let initial = "89c878576e39a642".to_owned();
    json!({"out":m.start.economy.players.iter().map(|p|p.out).collect::<Vec<_>>(),"fog":fog_cells,"visible":m.world.game.players.iter().map(|p|p.visible.clone()).collect::<Vec<_>>(),"mp":m.start.economy.players.iter().map(|p|bits(p.mp)).collect::<Vec<_>>(),"cooldowns":m.start.players.iter().map(|p|p.support.iter().map(|(k,v)|(k.clone(),bits(*v))).collect::<serde_json::Map<String,Value>>()).collect::<Vec<_>>(),"strikes":m.world.game.strikes.iter().map(|s|json!({"kind":s.runtime.as_ref().unwrap().kind.name(),"owner":s.owner,"x":bits(s.x),"z":bits(s.z),"dir":bits(s.dir),"t":bits(s.runtime.as_ref().unwrap().delay),"left":bits(s.runtime.as_ref().unwrap().left),"next":bits(s.runtime.as_ref().unwrap().next),"live":s.live})).collect::<Vec<_>>(),"covers":m.support.covers.iter().map(|c|json!({"team":c.team,"owner":c.owner,"x":bits(c.at.x),"z":bits(c.at.z),"r":bits(c.radius),"t":bits(c.remaining)})).collect::<Vec<_>>(),"smokes":m.world.game.smokes.iter().map(|s|json!({"id":s.id,"x":bits(s.x),"z":bits(s.z),"r":bits(s.r),"t":bits(s.remaining.unwrap())})).collect::<Vec<_>>(),"units":m.world.game.units.values().map(|u|{let c=m.combat.get(u.id).unwrap();json!({"id":u.id,"kind":m.rules.kinds[u.kind].name,"owner":u.owner,"x":bits(u.x),"z":bits(u.z),"riding":c.carrier.unwrap_or(0),"cargo":c.cargo,"ammo":m.logistics.get(u.id).map(|r|bits(r.ammo)),"hp":bits(u.hp),"supp":bits(u.supp),"lastHit":c.last_hit,"hitBy":c.hit_by,"hitAt":bits(c.hit_at),"retarget":bits(m.firing.get(u.id).unwrap().retarget)})}).collect::<Vec<_>>(),"terrain":{"chars":String::from_utf8(m.world.game.chars.clone()).unwrap(),"height":m.world.game.height.clone().unwrap_or_else(||vec![0;m.world.game.w*m.world.game.h]),"hp":sparse(&m.world.game.cell_hp),"wear":sparse(m.world.game.wear.as_deref().unwrap()),"scar":sparse(&m.world.scar),"versions":[m.world.game.terrain_version as u64,m.world.versions.obstruction,m.world.versions.infantry,m.world.versions.vehicle,m.world.versions.naval]},"environment":[m.start.environment_seed,m.next_unit_id],"story":m.story.iter().map(|s|[bits(s.support_calls),bits(s.mp_spent),bits(s.planes_downed)]).collect::<Vec<_>>(),"shots":shots,"random":random.unwrap_or(initial)})
}
fn compare(a: &Value, b: &Value, path: &str) {
    match b {
        Value::Number(b) => assert_eq!(a.as_f64(), b.as_f64(), "{path}"),
        Value::Array(b) => {
            let a = a.as_array().unwrap();
            assert_eq!(a.len(), b.len(), "{path}");
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                compare(a, b, &format!("{path}[{i}]"));
            }
        }
        Value::Object(b) => {
            let a = a.as_object().unwrap();
            assert_eq!(a.len(), b.len(), "{path}");
            for (k, b) in b {
                compare(a.get(k).unwrap(), b, &format!("{path}.{k}"));
            }
        }
        _ => assert_eq!(a, b, "{path}"),
    }
}
#[test]
fn actual_support_admission_strikes_and_timers_match_source() {
    let fixture = fixture();
    let mut count = 0;
    for t in fixture["traces"].as_array().unwrap() {
        let mut m = make(t);
        let mut last = 0;
        for (i, frame) in t["frames"].as_array().unwrap().iter().enumerate() {
            let action = &frame["action"];
            let denial = if action["op"] == "admit" {
                let r = &action["request"];
                m.admit_support(
                    0,
                    r["kind"].as_str().unwrap(),
                    Point {
                        x: r["x"].as_f64().unwrap(),
                        z: r["z"].as_f64().unwrap(),
                    },
                    r["dir"].as_f64(),
                )
                .unwrap()
                .map(|d| match d {
                    SupportDenial::Blocked => "blocked",
                    SupportDenial::Cooldown => "cooldown",
                    SupportDenial::Manpower => "mp",
                    SupportDenial::Unseen => "unseen",
                    SupportDenial::Population => "pop",
                })
            } else {
                let tick = action["tick"].as_i64().unwrap();
                for current in last + 1..=tick {
                    let prior = (
                        m.world
                            .game
                            .strikes
                            .iter()
                            .map(|s| s.live)
                            .collect::<Vec<_>>(),
                        m.support.covers.len(),
                    );
                    if t["variation"] == "out-before-delivery" && current == 1 {
                        m.start.economy.players[0].out = true;
                    }
                    m.world.game.tick = current;
                    m.support.events.clear();
                    let ids = m.world.game.units.keys().copied().collect::<Vec<_>>();
                    m.step_support_strikes(
                        if t["variation"].as_str().unwrap().starts_with("late-") {
                            &[]
                        } else {
                            &ids
                        },
                        0.05,
                    )
                    .unwrap();
                    m.advance_support_timers(0.05).unwrap();
                    if current < tick {
                        assert!(
                            m.support.events.is_empty(),
                            "unexpected effect on unsampled tick {current}"
                        );
                        assert_eq!(
                            (
                                m.world
                                    .game
                                    .strikes
                                    .iter()
                                    .map(|s| s.live)
                                    .collect::<Vec<_>>(),
                                m.support.covers.len()
                            ),
                            prior,
                            "unexpected lifetime transition on unsampled tick {current}"
                        );
                    }
                }
                last = tick;
                None
            };
            assert_eq!(json!(denial), frame["denial"]);
            compare(
                &snapshot(&mut m),
                &frame["expected"],
                &format!("{} {} {} frame{i}", t["kind"], t["faction"], t["variation"]),
            );
            count += 1;
        }
    }
    assert_eq!(count, 5922);
}
#[test]
fn unknown_support_and_invalid_membership_do_not_charge_or_publish() {
    let f = fixture();
    let mut m = make(&f["traces"][0]);
    let wallet = m.start.economy.players[0].mp;
    let cooldowns = m.start.players[0].support.clone();
    assert_eq!(
        m.admit_support(0, "unknown", Point { x: 41.0, z: 41.0 }, None)
            .unwrap(),
        Some(SupportDenial::Blocked)
    );
    assert_eq!(m.start.economy.players[0].mp, wallet);
    assert_eq!(m.start.players[0].support, cooldowns);
    assert_eq!(m.support, SupportState::default());
    m.admit_support(0, "artillery", Point { x: 41.0, z: 41.0 }, None)
        .unwrap();
    let before = m.support.clone();
    let strikes = m.world.game.strikes.clone();
    let random = m.start.random.snapshot();
    assert!(m.step_support_strikes(&[2], 0.05).is_err());
    assert_eq!(m.support, before);
    assert_eq!(m.world.game.strikes, strikes);
    assert_eq!(m.start.random.snapshot(), random);
}

#[test]
fn mixed_move_and_support_share_one_recorded_boundary_order() {
    use ww2_core::{
        match_commands::{Command, Denial},
        orders::OrderKey,
    };
    let f = fixture();
    let rows = [
        json!({"t":"support","kind":"artillery","x":41,"z":41,"dir":0.3}),
        json!({"t":"move","orders":[[1,45,41]]}),
        json!({"t":"support","kind":"artillery","x":51,"z":41,"dir":0.3}),
    ];
    let mut expected = None;
    for arrival in [[0, 1, 2], [2, 0, 1], [1, 2, 0]] {
        let mut m = make(&f["traces"][0]);
        for i in arrival {
            m.enqueue_command(
                OrderKey {
                    tick: 1,
                    issuer: 1,
                    sequence: i as u64 + 1,
                },
                Command::from_json(&rows[i]).unwrap(),
            )
            .unwrap();
        }
        let results = m.apply_commands(1, false).unwrap();
        assert_eq!(
            results.iter().map(|r| r.key.sequence).collect::<Vec<_>>(),
            [1, 2, 3]
        );
        assert_eq!(
            results.iter().map(|r| r.denial).collect::<Vec<_>>(),
            [None, None, Some(Denial::Cooldown)]
        );
        assert_eq!(m.start.economy.players[1].mp, 350.0);
        assert_eq!(m.story[1].support_calls, 1.0);
        assert_eq!(m.story[1].mp_spent, 150.0);
        assert_eq!(m.world.game.strikes.len(), 1);
        assert_eq!(
            Point {
                x: m.world.game.strikes[0].x,
                z: m.world.game.strikes[0].z
            },
            Point { x: 41.0, z: 41.0 }
        );
        assert!(!m.world.game.units[&1].path.is_empty());
        let value = (
            m.world.game.units[&1].path.clone(),
            m.support.clone(),
            m.start.players[1].support.clone(),
        );
        if let Some(expected) = &expected {
            assert_eq!(&value, expected);
        } else {
            expected = Some(value);
        }
        let wallet = m.start.economy.players[1].mp;
        assert!(
            Command::from_json(
                &json!({"t":"support","kind":"artillery","x":41,"z":41,"orders":[[1,2]]})
            )
            .is_err()
        );
        assert_eq!(m.start.economy.players[1].mp, wallet);
    }
}

#[test]
fn default_mode_alias_uses_ordinary_support_and_invalid_air_registry_is_explicit() {
    let f = fixture();
    for mode in [Mode::Default, Mode::Conquest] {
        let mut m = make_with_mode(&f["traces"][0], mode);
        assert_eq!(
            m.admit_support(0, "smoke", Point { x: 41.0, z: 41.0 }, None)
                .unwrap(),
            None
        );
        assert_eq!(m.start.economy.players[0].mp, 450.0);
        m.step_support_anti_air(&[1], 0.05).unwrap();
        m.world.game.units.get_mut(&1).unwrap().air = Some(ww2_core::game::PlaneState::Station);
        let before = (
            m.start.economy.players.clone(),
            m.world.game.strikes.clone(),
            m.support.clone(),
        );
        assert!(matches!(
            m.step_support_anti_air(&[1], 0.05),
            Err(ww2_core::match_support::SupportError::Invalid(_))
        ));
        assert_eq!(
            (
                m.start.economy.players.clone(),
                m.world.game.strikes.clone(),
                m.support.clone()
            ),
            before
        );
    }
}

fn protected_support(m: &Match) -> Vec<String> {
    vec![
        format!("{:?}", m.world),
        format!("{:?}", m.environment),
        format!("{:?}", m.start.random.snapshot()),
        format!("{:?}", m.start.economy.players),
        format!("{:?}", m.start.players),
        format!("{:?}", m.commands.units),
        format!("{:?}", m.support),
        format!("{:?}", m.aviation),
        format!("{:?}", m.combat),
        format!("{:?}", m.firing),
        format!("{:?}", m.abilities),
        format!("{:?}", m.recovery),
        format!("{:?}", m.logistics),
        format!("{:?}", m.story),
        format!("{:?}", (m.start.environment_seed, m.next_unit_id)),
    ]
}

#[test]
fn support_before_clock_rejects_malformed_state_without_publication() {
    let f = fixture();
    for malformed in ["height", "clock", "air", "cover", "story"] {
        let mut m = make(&f["traces"][0]);
        let actors = m.world.game.units.keys().copied().collect::<Vec<_>>();
        m.validate_support_tick_phases(&actors, 0.05, 1).unwrap();
        match malformed {
            "height" => m.world.game.height = Some(Vec::new()),
            "air" => {
                m.world.game.units.get_mut(&actors[0]).unwrap().air =
                    Some(ww2_core::game::PlaneState::Station)
            }
            "cover" => m.support.covers.push(FighterCover {
                team: 1,
                owner: 99,
                at: Point { x: 41.0, z: 41.0 },
                radius: 40.0,
                remaining: 60.0,
            }),
            "story" => m.story[0].support_calls = f64::NAN,
            "clock" => (),
            _ => unreachable!(),
        }
        let before = protected_support(&m);
        assert!(
            m.validate_support_tick_phases(&actors, 0.05, if malformed == "clock" { 2 } else { 1 })
                .is_err(),
            "{malformed}"
        );
        assert_eq!(protected_support(&m), before, "{malformed}");
        if malformed == "height" {
            assert!(
                m.admit_support(0, "para", Point { x: 41.0, z: 41.0 }, None)
                    .is_err()
            );
            assert_eq!(protected_support(&m), before);
        }
    }
}

#[test]
fn support_arrival_preflights_shared_ids_and_initializes_actual_birth_owners() {
    let f = fixture();
    let mut m = make(&f["traces"][0]);
    m.admit_support(1, "para", Point { x: 41.0, z: 41.0 }, None)
        .unwrap();
    m.world.game.strikes[0].runtime.as_mut().unwrap().delay = 0.0;
    let actors = m.world.game.units.keys().copied().collect::<Vec<_>>();
    let id = m.next_unit_id;
    m.recovery.initialize(id);
    let before = protected_support(&m);
    assert_eq!(
        m.validate_support_tick_phases(&actors, 0.05, 1),
        Err(ww2_core::match_support::SupportError::Invalid(
            "drop metadata identity occupied"
        ))
    );
    assert!(m.step_support_strikes(&actors, 0.05).is_err());
    assert_eq!(protected_support(&m), before);
    m.recovery.remove(id);
    m.validate_support_tick_phases(&actors, 0.05, 1).unwrap();
    m.step_support_strikes(&actors, 0.05).unwrap();
    assert_eq!(m.next_unit_id, id + 3);
    for born in id..m.next_unit_id {
        assert_eq!(
            m.abilities.get(born),
            Some(&ww2_core::match_abilities::AbilityUnit::default())
        );
        assert_eq!(
            m.recovery.get(born),
            Some(&ww2_core::match_recovery::RecoveryUnit::default())
        );
        assert!(m.combat.get(born).is_some());
        assert!(m.firing.get(born).is_some());
    }
}
