#[path = "common/startup_projection.rs"]
mod projection;
#[path = "common/startup.rs"]
mod startup;
use projection::{bits, equivalent, terrain};
use serde_json::{Value, json};
use ww2_core::{
    economy::{Faction, Mode, Technology},
    economy_state::ArmySize,
    game::ModeKind,
    map::AuthoritativeMap,
    match_runtime::{Match, MatchOptions, PlayerConfig, StartPlan},
    rules::Rules,
    weather::WeatherChoice,
};
fn options(c: &Value) -> MatchOptions {
    MatchOptions {
        seed: c["seed"].as_str().unwrap().parse().unwrap(),
        mode: Mode::Assault,
        players: c["players"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| PlayerConfig {
                name: p["name"].as_str().unwrap().to_owned(),
                team: p["team"].as_i64().unwrap() as i32,
                faction: Faction::try_from(p["faction"].as_u64().unwrap() as u8).unwrap(),
            })
            .collect(),
        shuffle: c["shuffle"].as_bool().unwrap(),
        army: match c["army"].as_str().unwrap() {
            "large" => ArmySize::Large,
            "massive" => ArmySize::Massive,
            "endless" => ArmySize::Endless,
            _ => ArmySize::Standard,
        },
        weather: WeatherChoice::from_reference(c["weather"].as_str()),
        weather_seed: Some(31),
        living_weather: c["living"].as_bool().unwrap(),
        map_key: None,
        tech: c["tech"].as_bool().unwrap(),
        supply: false,
        logistics: false,
        sandbox: false,
    }
}
fn fixture() -> Value {
    fn unpack(v: &Value, table: &Value) -> Value {
        match v {
            Value::Object(o) if o.len() == 1 && o.contains_key("$fixtureRef") => {
                unpack(&table[o["$fixtureRef"].as_u64().unwrap() as usize], table)
            }
            Value::Array(a) => Value::Array(a.iter().map(|v| unpack(v, table)).collect()),
            Value::Object(o) => Value::Object(
                o.iter()
                    .map(|(k, v)| (k.clone(), unpack(v, table)))
                    .collect(),
            ),
            _ => v.clone(),
        }
    }
    let compact: Value = serde_json::from_str(include_str!("fixtures/assault-start.json")).unwrap();
    assert_eq!(compact["format"], 2);
    unpack(&compact["data"], &compact["table"])
}
#[test]
fn explicit_assault_start_matches_original_create_game() {
    let fixture = fixture();
    assert_eq!(
        fixture["reference"],
        "e4e579a838aabae3b1d9dadbc17a0a87147bc52d"
    );
    let mut units = 0;
    for (index, c) in fixture["cases"].as_array().unwrap().iter().enumerate() {
        let source = &fixture["maps"][c["map"].as_u64().unwrap() as usize]["map"];
        let map = AuthoritativeMap::from_json(&serde_json::to_vec(source).unwrap()).unwrap();
        let mut m =
            Match::new_assault(&map, options(c), c["defender"].as_i64().unwrap() as i32).unwrap();
        let expected = &c["expected"];
        let state = m.assault.unwrap();
        assert_eq!(m.world.game.mode, Some(ModeKind::Assault));
        assert!(m.world_runtime.is_none());
        assert!(
            equivalent(
                &json!({"kind":"assault","defenderTeam":state.defender_team,"attackerTeam":state.attacker_team,"timeLeft":state.time_left,"total":state.total}),
                &expected["mode"]
            ),
            "case{index} mode"
        );
        let players:Vec<_>=m.start.players.iter().zip(&m.start.economy.players).map(|(p,e)|json!({"team":p.config.team,"spawn":[bits(p.spawn.x),bits(p.spawn.z)],"mp":bits(e.mp),"inc":bits(e.inc),"vp":bits(e.vp)})).collect();
        assert_eq!(json!(players), expected["players"], "case{index} players");
        assert!(
            m.options
                .players
                .iter()
                .zip(&m.start.players)
                .all(|(a, b)| a.team == b.config.team)
        );
        let points:Vec<_>=m.start.points.iter().map(|p|json!({"id":p.id,"at":[bits(p.at.x),bits(p.at.z)],"vp":bits(p.vp),"mp":bits(p.mp),"kind":p.kind,"needs":p.needs.map_or(-1,|v|v as i64)})).collect();
        assert_eq!(json!(points), expected["points"], "case{index} points");
        assert_eq!(bits(m.start.win_vp), expected["winVp"]);
        let raw = expected["units"].as_array().unwrap();
        assert_eq!(m.world.game.units.len(), raw.len());
        assert_eq!(json!(m.next_unit_id), expected["nextId"]);
        for raw in raw {
            let unit = simtest::load::unit(
                raw,
                &m.rules,
                m.start.players.len(),
                m.world.game.w * m.world.game.h,
            )
            .unwrap();
            assert_eq!(
                simtest::replay::diff(&m.world.game.units[&unit.id], &unit),
                None,
                "case{index} unit{}",
                unit.id
            );
            units += 1;
            assert!(
                m.grid
                    .in_order(&m.world.game.units, (unit.x, unit.z), 0.0, |_, _| true)
                    .contains(&unit.id)
            );
        }
        let controls: Vec<_> = m
            .world
            .game
            .units
            .keys()
            .map(|id| {
                let c = &m.commands.units[id];
                json!([
                    id,
                    bits(c.aim),
                    c.auto_cast,
                    c.auto_retreat,
                    c.attack_id,
                    []
                ])
            })
            .collect();
        assert_eq!(
            json!(controls),
            expected["controls"],
            "case{index} controls"
        );
        let production: Vec<_> = m
            .world
            .game
            .units
            .values()
            .filter(|u| u.cells.is_some())
            .map(|u| {
                let p = m.production.building(u.id).unwrap();
                assert!(p.jobs().is_empty());
                json!([
                    u.id,
                    p.queue().collect::<Vec<_>>(),
                    [],
                    bits(p.progress()),
                    m.building_rallies[&u.id].map(|p| [p.x, p.z])
                ])
            })
            .collect();
        assert_eq!(
            json!(production),
            expected["production"],
            "case{index} production"
        );
        let tech:Vec<_>=(0..m.start.players.len()).map(|i| { let p=m.production.player(i).unwrap(); let t=p.tech(); assert!(p.jobs().is_empty()); if m.options.tech { json!([t.tier(),{"inf":t.level(Technology::Infantry),"guns":t.level(Technology::Guns),"armor":t.level(Technology::Armor)},[]]) } else {Value::Null} }).collect();
        assert_eq!(json!(tech), expected["tech"], "case{index} tech");
        let mut actual = terrain(&m);
        actual["wear"] = projection::runs(
            m.world
                .game
                .wear
                .as_ref()
                .unwrap()
                .iter()
                .map(|v| json!(v.to_bits())),
        );
        actual["surfaceWear"] =
            projection::runs(m.world.surface_wear.iter().map(|v| json!(v.to_bits())));
        for (key, value) in expected["terrain"].as_object().unwrap() {
            assert!(
                equivalent(&actual[key], value),
                "case{index} terrain {key}: actual{} expected{}",
                actual[key],
                value
            );
        }
        assert_eq!(json!(m.start.environment_seed), expected["environmentSeed"]);
        assert_eq!(
            json!([bits(m.start.wind.0), bits(m.start.wind.1)]),
            expected["wind"]
        );
        assert_eq!(bits(m.start.initial_rain), expected["rain"]);
        assert_eq!(
            m.start.initial_wet.map(bits).unwrap_or(Value::Null),
            expected["wet"]
        );
        assert_eq!(
            json!({"now":m.start.weather.now.name(),"next":m.start.weather.next.map(|v|v.name()),"at":m.start.weather.at}),
            expected["weather"]
        );
        assert!(m.finish.is_none());
        assert!(m.timeline.is_empty());
        assert_eq!(m.capture_ticks.len(), m.start.points.len());
        assert!(m.story.iter().all(|r| *r == Default::default()));
        // Constructor availability must not quietly admit the incomplete mode to a Conquest phase.
        assert!(m.apply_conquest_victory().is_err());
        assert!(m.sample_conquest_tick().is_err());
        assert!(m.apply_point_economy(&[], false).is_err());
    }
    assert_eq!(fixture["cases"].as_array().unwrap().len(), 43);
    assert_eq!(units, 702);
}
#[test]
fn explicit_mode_admission_rejects_invalid_or_unimplemented_setup() {
    let fixture = fixture();
    let c = &fixture["cases"][0];
    let source = &fixture["maps"][c["map"].as_u64().unwrap() as usize]["map"];
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(source).unwrap()).unwrap();
    let o = options(c);
    assert!(Match::new(&map, o.clone()).is_err());
    assert!(StartPlan::prepare(&map, &o, &Rules::compiled().unwrap()).is_err());
    assert!(Match::new_assault(&map, o.clone(), 6).is_err());
    let mut wrong = o.clone();
    wrong.mode = Mode::Conquest;
    assert!(Match::new_assault(&map, wrong, 1).is_err());
    let mut wrong = o.clone();
    wrong.players[1].team = 99;
    assert!(Match::new_assault(&map, wrong, 1).is_err());
    let mut wrong = o.clone();
    wrong.logistics = true;
    let normalized = Match::new_assault(&map, wrong, 1).unwrap();
    assert!(!normalized.options.logistics);
    let mut wrong = o;
    wrong.players.clear();
    assert!(Match::new_assault(&map, wrong, 1).is_err());
}
