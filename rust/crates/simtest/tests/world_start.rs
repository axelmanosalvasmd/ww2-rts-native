//! Bounded original World startup and native seed continuity.
#[path = "common/startup.rs"]
mod startup;
use serde_json::{Value, json};
use ww2_core::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::LOCAL_DEFENDER_OWNER,
    map::AuthoritativeMap,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    random::{MatchRandom, StreamId},
    weather::WeatherChoice,
};
fn bits(n: f64) -> Value {
    json!(format!("{:016x}", n.to_bits()))
}
fn equivalent(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => a.as_f64() == b.as_f64(),
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| equivalent(a, b))
        }
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len()
                && a.iter()
                    .all(|(k, a)| b.get(k).is_some_and(|b| equivalent(a, b)))
        }
        _ => a == b,
    }
}
fn hash(bytes: impl IntoIterator<Item = u8>) -> Value {
    let mut h = 0xcbf29ce484222325u64;
    for b in bytes {
        h = (h ^ u64::from(b)).wrapping_mul(0x100000001b3);
    }
    json!(format!("{h:016x}"))
}
fn options(c: &Value) -> MatchOptions {
    MatchOptions {
        seed: c["seed"].as_u64().unwrap(),
        mode: Mode::World,
        players: c["players"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| PlayerConfig {
                name: p["name"].as_str().unwrap().into(),
                team: p["team"].as_i64().unwrap() as i32,
                faction: Faction::try_from(p["faction"].as_u64().unwrap() as u8).unwrap(),
            })
            .collect(),
        shuffle: c["settings"]["shuffle"].as_bool().unwrap(),
        army: match c["settings"]["army"].as_str().unwrap() {
            "standard" => ArmySize::Standard,
            "large" => ArmySize::Large,
            "massive" => ArmySize::Massive,
            "endless" => ArmySize::Endless,
            _ => unreachable!(),
        },
        weather: WeatherChoice::from_reference(c["settings"]["weather"].as_str()),
        weather_seed: c["weatherSeed"].as_u64().map(|v| v as u32),
        living_weather: c["settings"]["living"].as_bool().unwrap(),
        map_key: None,
        tech: c["settings"]["tech"].as_bool().unwrap(),
        supply: true,
        logistics: false,
        sandbox: false,
    }
}
fn map(c: &Value) -> AuthoritativeMap {
    let mut m = c["map"].clone();
    if let Some(runs) = m["world"]["regionMapRuns"].as_array() {
        let values: Vec<_> = runs
            .iter()
            .flat_map(|r| std::iter::repeat_n(r[0].clone(), r[1].as_u64().unwrap() as usize))
            .collect();
        m["world"].as_object_mut().unwrap().remove("regionMapRuns");
        m["world"]["regionMap"] = json!(values);
    }
    AuthoritativeMap::from_json(&serde_json::to_vec(&m).unwrap()).unwrap()
}
fn external_owner(owner: usize) -> Value {
    if owner == LOCAL_DEFENDER_OWNER {
        json!(-1)
    } else {
        json!(owner)
    }
}
#[test]
fn original_world_startup() {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/world-start.json")).unwrap();
    assert_eq!(
        fixture["reference"],
        "e4e579a838aabae3b1d9dadbc17a0a87147bc52d"
    );
    let mut units = 0;
    for (index, c) in fixture["cases"].as_array().unwrap().iter().enumerate() {
        let m = map(c);
        let o = options(c);
        let instance = Match::new_world(&m, o.clone(), MatchRandom::new(o.seed)).unwrap();
        let e = &c["expected"];
        let game = &instance.world.game;
        let runtime = instance.world_runtime.as_ref().unwrap();
        let players:Vec<_>=instance.start.players.iter().zip(&instance.start.economy.players).zip(&game.players).map(|((p,w),g)|json!({
            "spawn":[bits(p.spawn.x),bits(p.spawn.z)],"mp":bits(w.mp),"inc":bits(w.inc),"mun":bits(w.mun),"fuel":bits(w.fuel),"visible":g.visible,
        })).collect();
        assert_eq!(json!(players), e["players"], "case {index} commanders");
        for query in e["regionQueries"].as_array().unwrap() {
            let at = ww2_core::game::Point {
                x: query[0].as_f64().unwrap(),
                z: query[1].as_f64().unwrap(),
            };
            let actual = instance.world_region_index(at).unwrap();
            assert_eq!(json!(actual), query[2], "case {index} region query {at:?}");
            assert_eq!(
                json!(actual.map(|i| instance.world.geography().unwrap().regions[i].id)),
                query[3]
            );
            assert_eq!(
                instance.world_region_at(at).unwrap().map(|r| r.team),
                actual.map(|i| runtime.regions[i].team)
            );
        }
        for (i, region) in instance.world_economy_regions().unwrap().iter().enumerate() {
            assert_eq!(region.team, runtime.regions[i].team);
            use ww2_core::economy_state::RegionKind;
            let expected = match instance.world.geography().unwrap().regions[i]
                .kind
                .as_deref()
            {
                Some("city") => RegionKind::City,
                Some("industrial") => RegionKind::Industrial,
                Some("resource") => RegionKind::Resource,
                _ => RegionKind::Other,
            };
            assert_eq!(region.kind, expected);
        }

        assert_eq!(
            json!(
                runtime
                    .regions
                    .iter()
                    .map(|r| json!([r.team, bits(r.progress), r.capper.map_or(-1, |x| x as i64)]))
                    .collect::<Vec<_>>()
            ),
            e["world"],
            "case {index} regions"
        );
        assert_eq!(
            json!(
                runtime
                    .nodes
                    .iter()
                    .map(|n| json!([
                        n.cell,
                        bits(n.at.x),
                        bits(n.at.z),
                        n.depot.unwrap_or(0),
                        bits(n.rate),
                        n.fuel
                    ]))
                    .collect::<Vec<_>>()
            ),
            e["nodes"]
        );
        assert_eq!(
            json!({"faction":runtime.local_defender.faction as u8,"spawn":[bits(runtime.local_defender.spawn.x),bits(runtime.local_defender.spawn.z)],"visible":runtime.local_defender.visible}),
            e["actor"],
            "case {index} neutral actor"
        );
        let tech:Vec<_>=(0..instance.start.players.len()).map(|slot| {
            let research=instance.production.player(slot).unwrap();
            assert!(research.jobs().is_empty());
            let t=research.tech();
            if !instance.options.tech { Value::Null } else {json!([t.tier(),{"inf":t.level(ww2_core::economy::Technology::Infantry),"guns":t.level(ww2_core::economy::Technology::Guns),"armor":t.level(ww2_core::economy::Technology::Armor)},[]])}
        }).collect();
        assert_eq!(json!(tech), e["tech"], "case {index} research");
        assert_eq!(game.units.len(), e["units"].as_array().unwrap().len());
        assert_eq!(instance.next_unit_id as usize, game.units.len() + 1);
        for raw in e["units"].as_array().unwrap() {
            let mut raw_for_loader = raw.clone();
            let neutral = raw["owner"] == json!(-1);
            if neutral {
                raw_for_loader["owner"] = json!(0);
            }
            let mut expected = simtest::load::unit(
                &raw_for_loader,
                &instance.rules,
                o.players.len(),
                game.w * game.h,
            )
            .unwrap();
            if neutral {
                expected.owner = LOCAL_DEFENDER_OWNER;
            }
            let actual = &game.units[&expected.id];
            let firing = instance.firing.get(actual.id).unwrap();
            assert_eq!(json!(firing.target_id), raw["targetId"]);
            assert_eq!(
                bits(firing.cooldown),
                bits(raw["cooldown"].as_f64().unwrap())
            );
            assert_eq!(
                bits(firing.retarget),
                bits(raw["retarget"].as_f64().unwrap())
            );
            let combat = instance.combat.get(actual.id).unwrap();
            assert_eq!(bits(combat.xp), bits(raw["xp"].as_f64().unwrap()));
            assert_eq!(bits(combat.hit_at), bits(raw["hitAt"].as_f64().unwrap()));
            assert_eq!(json!(combat.cargo), raw["cargo"]);
            assert_eq!(
                instance.abilities.get(actual.id),
                Some(&ww2_core::match_abilities::AbilityUnit::default()),
                "case {index} unit {} ability birth metadata",
                actual.id
            );
            assert_eq!(
                instance.recovery.get(actual.id),
                Some(&ww2_core::match_recovery::RecoveryUnit::default()),
                "case {index} unit {} recovery birth metadata",
                actual.id
            );
            assert_eq!(
                simtest::replay::diff(actual, &expected),
                None,
                "case {index} unit {}",
                expected.id
            );
            if let Some(region) = raw.get("region") {
                let index = runtime.guard_bases[&actual.id];
                assert_eq!(
                    json!(instance.world.geography().unwrap().regions[index].id),
                    *region
                );
            }
            let control = &instance.commands.units[&actual.id];
            assert_eq!(control.auto_cast, raw["auto"].as_bool().unwrap());
            assert_eq!(control.auto_retreat, raw["autoRetreat"].as_bool().unwrap());
            assert!(equivalent(
                &control
                    .guard_home
                    .map(|p| json!({"x":p.x,"z":p.z}))
                    .unwrap_or(Value::Null),
                &raw.get("guardHome").cloned().unwrap_or(Value::Null)
            ));
            if raw.get("queue").is_some() {
                let production = instance.production.building(actual.id).unwrap();
                assert!(production.jobs().is_empty());
                assert_eq!(production.progress(), 0.0);
                assert_eq!(instance.building_rallies[&actual.id], None);
            }
            if neutral {
                assert_eq!(game.team_of(actual), -1);
            }
            units += 1;
        }
        let ghosts:Vec<_>=runtime.known_buildings.iter().map(|(team,rows)|json!([team,rows.iter().map(|r|json!({"id":r.id,"type":instance.rules.kinds[r.kind].name,"owner":external_owner(r.owner),"x":r.at.x,"z":r.at.z,"built":r.built})).collect::<Vec<_>>()])).collect();
        // Input teams are ascending in these cases; shared-team insertion is checked by source rows.
        assert!(
            equivalent(&json!(ghosts), &e["ghostTeams"]),
            "case {index} ghosts"
        );
        let rails = if e["rails"].is_null() {
            Value::Null
        } else {
            json!({
                "stations":runtime.rails.stations.iter().map(|s|json!({"region":s.region,"c":s.cell,"x":s.x,"z":s.z,"next":s.next})).collect::<Vec<_>>(),
                "lines":runtime.rails.lines.iter().map(|l|json!({"a":l.a,"b":l.b,"cells":l.cells,"regions":l.regions,"bridges":l.bridges})).collect::<Vec<_>>(),
                "railAt":runtime.rails.rail_at,"stationAt":runtime.rails.station_at,"yard":runtime.rails.yard,
            })
        };
        assert!(equivalent(&rails, &e["rails"]), "case {index} rails");
        assert!(
            instance
                .start
                .random
                .snapshot()
                .streams
                .iter()
                .all(|(stream, _)| *stream == StreamId::Weather)
        );
        assert_eq!(
            instance.start.environment_seed,
            e["environmentSeed"].as_u64().unwrap() as u32
        );
        assert_eq!(
            json!([bits(instance.start.wind.0), bits(instance.start.wind.1)]),
            e["wind"]
        );
        assert_eq!(
            json!({"now":instance.start.weather.now.name(),"next":instance.start.weather.next.map(|v|v.name()),"at":instance.start.weather.at}),
            e["weather"]
        );
        assert_eq!(
            instance.start.initial_wet.map(bits).unwrap_or(Value::Null),
            e["wet"]
        );
        assert_eq!(bits(game.rain), e["rain"]);
        assert_eq!(
            instance
                .start
                .living
                .map(|w| json!({"raining":w.raining,"next":bits(w.next)}))
                .unwrap_or(Value::Null),
            e["living"]
        );
        let w = &instance.world;
        let terrain = json!({"chars":hash(game.chars.iter().copied()),"ground":hash(game.ground.as_ref().unwrap().iter().copied()),"objects":hash(game.objects.as_ref().unwrap().iter().copied()),
            "flags":hash(game.flags.iter().flat_map(|v|v.to_le_bytes())),"height":game.height.as_ref().map(|h|hash(h.iter().map(|v|*v as u8))),
            "hp":hash(game.cell_hp.iter().flat_map(|v|v.to_bits().to_le_bytes())),"wear":hash(game.wear.as_ref().unwrap().iter().flat_map(|v|v.to_bits().to_le_bytes())),"state":hash(w.cell_state.iter().copied()),
            "versions":[game.terrain_version as u64,w.versions.obstruction,w.versions.infantry,w.versions.vehicle,w.versions.naval],
            "log":w.cell_log().iter().map(startup::row).collect::<Vec<_>>(),"newCells":w.new_cells().iter().map(startup::row).collect::<Vec<_>>(),"journal":w.journal(),
            "buildingCells":instance.building_cells.iter().map(|(c,b)|json!([c,b.unit,external_owner(b.owner),bits(b.at.x),bits(b.at.z)])).collect::<Vec<_>>(),
        });
        for (key, expected) in e["terrain"].as_object().unwrap() {
            assert!(
                equivalent(&terrain[key], expected),
                "case {index} terrain {key}"
            );
        }
        assert_eq!(instance.logistics.iter().count(), 0);
        assert_eq!(
            instance
                .world
                .pending(ww2_core::world::Recipient::Player(0)),
            None
        );
    }
    assert_eq!(fixture["cases"].as_array().unwrap().len(), 5);
    assert!(units > 1000);
}

#[test]
fn generated_world_retains_random_history_and_replays() {
    use ww2_core::world_generation::{WorldSize, generate_world};
    let fixture: Value = serde_json::from_str(include_str!("fixtures/world-start.json")).unwrap();
    let mut o = options(&fixture["cases"][0]);
    o.seed = 91234;
    let mut random = MatchRandom::new(o.seed);
    let generated =
        generate_world(&mut random, WorldSize::Huge, &[o.players[0].team], true).unwrap();
    let terrain_state = random
        .snapshot()
        .streams
        .iter()
        .find(|(s, _)| *s == StreamId::Terrain)
        .copied()
        .unwrap();
    let m = AuthoritativeMap::from_world(generated.into_world_map_input()).unwrap();
    let first = Match::new_world(&m, o.clone(), random.clone()).unwrap();
    let second = Match::new_world(&m, o, random).unwrap();
    assert_eq!(
        first.start.random.snapshot(),
        second.start.random.snapshot()
    );
    assert!(
        first
            .start
            .random
            .snapshot()
            .streams
            .contains(&terrain_state)
    );
    assert_eq!(
        format!("{:?}", first.world.game),
        format!("{:?}", second.world.game)
    );
    assert_eq!(
        first.world_runtime.as_ref().unwrap().regions,
        second.world_runtime.as_ref().unwrap().regions
    );
    let actor = first.world_actor_view(LOCAL_DEFENDER_OWNER).unwrap();
    assert_eq!(actor.team, -1);
    assert_eq!(actor.faction, Faction::Usa);
    assert!(first.world_actor_view(LOCAL_DEFENDER_OWNER - 1).is_err());
    assert!(
        first
            .world_region_index(ww2_core::game::Point {
                x: f64::NAN,
                z: 0.0
            })
            .is_err()
    );
    assert_eq!(
        first.start.economy.players.len(),
        first.world.game.players.len()
    );
    assert!(
        first
            .production
            .player(first.world.game.players.len())
            .is_none()
    );
    assert!(
        first
            .world
            .game
            .allied(LOCAL_DEFENDER_OWNER, LOCAL_DEFENDER_OWNER)
    );
    assert!(!first.world.game.allied(LOCAL_DEFENDER_OWNER, 0));
    let mut ordinary = first.world.game.clone();
    ordinary.mode = None;
    assert!(!ordinary.allied(LOCAL_DEFENDER_OWNER, LOCAL_DEFENDER_OWNER));
}

#[test]
fn incomplete_world_inputs_are_explicit_and_do_not_mutate_the_map() {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/world-start.json")).unwrap();
    let c = &fixture["cases"][0];
    let m = map(c);
    let o = options(c);
    let before = m.source().clone();
    assert!(Match::new(&m, o.clone()).is_err());
    let mut invalid = o.clone();
    invalid.logistics = true;
    assert!(Match::new_world(&m, invalid, MatchRandom::new(o.seed)).is_err());
    let mut invalid = o.clone();
    invalid.mode = Mode::Conquest;
    assert!(Match::new_world(&m, invalid, MatchRandom::new(o.seed)).is_err());
    assert!(Match::new_world(&m, o.clone(), MatchRandom::new(o.seed + 1)).is_err());
    let mut input = m.source().clone();
    input["world"]["rails"] = json!({"stations":[{"region":1000,"x":0,"y":0}],"lines":[]});
    let invalid = AuthoritativeMap::from_json(&serde_json::to_vec(&input).unwrap()).unwrap();
    assert!(Match::new_world(&invalid, o.clone(), MatchRandom::new(o.seed)).is_err());

    let mut instance = Match::new_world(&m, o.clone(), MatchRandom::new(o.seed)).unwrap();
    let log = instance.world.cell_log().to_vec();
    let new = instance.world.new_cells().to_vec();
    let start = new.len() - 2;
    let fields = [
        (new[start].cell, Some(255), None),
        (new[start + 1].cell, None, Some(1000)),
    ];
    assert!(instance.world.annotate_recent_rows(start, &fields).is_err());
    assert_eq!(instance.world.cell_log(), log);
    assert_eq!(instance.world.new_cells(), new);
    instance.world_runtime.as_mut().unwrap().regions.pop();
    assert!(
        instance
            .world_region_index(ww2_core::game::Point { x: 1.0, z: 1.0 })
            .is_err()
    );
    assert!(instance.world_economy_regions().is_err());
    assert_eq!(m.source(), &before);
}
