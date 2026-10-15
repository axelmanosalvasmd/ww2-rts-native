use crate::shared::startup_projection as projection;
use projection::{bits, equivalent, terrain};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use ww2_core::{
    economy::{Faction, Mode, Technology},
    economy_state::ArmySize,
    game::{CELL, ModeKind, Point},
    map::AuthoritativeMap,
    match_runtime::{Match, MatchOptions, PlayerConfig, StartPlan},
    observed_memory::{FootprintFact, ObservationFacts, ObservationFrame, ObservedMemory},
    pathfinding::{PathResult, PathRules, PathStatus, Pathfinding},
    route_knowledge::{InitialTerrain, LiveRouting, RouteKnowledge},
    rules::Rules,
    vision::team_sees,
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
        supply: c["supply"].as_bool().unwrap_or(false),
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
// Source findPath first derives observedPathView for the actual asking rifle's owner.
// Compose the same existing components here without admitting Match recipient routing.
struct HillFacts {
    visible: Vec<bool>,
    footprints: BTreeMap<usize, FootprintFact>,
}
impl ObservationFacts for HillFacts {
    fn cell_visible(&mut self, cell: usize, _: i32) -> bool {
        self.visible[cell]
    }
    fn mine_known(&mut self, _: usize, _: i32) -> bool {
        false // The actual Hill constructor is checked to contain no mines below.
    }
    fn mine_owned(&mut self, _: usize, _: i32) -> bool {
        false
    }
    fn footprint(&mut self, cell: usize, _: usize, _: i32) -> Option<FootprintFact> {
        self.footprints.get(&cell).copied()
    }
}
fn hill_raw_route(m: &mut Match, actor_id: u32, target: Point) -> PathResult {
    let g = &m.world.game;
    let owner = g.units[&actor_id].owner;
    let team = g.players[owner].team;
    let n = g.w * g.h;
    assert!(m.world.mines.is_empty());
    assert!(m.world.mine_seen.iter().next().is_none());
    assert!(m.world.fires.is_empty());
    let mut facts = HillFacts {
        visible: vec![false; n],
        footprints: BTreeMap::new(),
    };
    for row in m.world.cell_log() {
        let c = row.cell;
        facts.visible[c] = team_sees(
            g,
            &m.rules,
            &m.grid,
            team,
            Point {
                x: (c % g.w) as f64 * CELL + CELL / 2.0,
                z: (c / g.w) as f64 * CELL + CELL / 2.0,
            },
        );
        if let Some(b) = m.building_cells.get(&c) {
            let live = g.units.get(&b.unit);
            let at = live.map_or(b.at, |u| u.at());
            facts.footprints.insert(
                c,
                FootprintFact {
                    identity: u64::from(b.unit),
                    allied: g.allied(live.map_or(b.owner, |u| u.owner), owner),
                    live: live.is_some(),
                    visible: g.players[owner].sees(b.unit),
                    ruin_visible: team_sees(g, &m.rules, &m.grid, team, at),
                },
            );
        }
    }
    let initial = m.world.initial();
    let shoals = m
        .world
        .shoal
        .as_ref()
        .map(|v| v.iter().map(|&v| v != 0).collect::<Vec<_>>());
    let mut routes = RouteKnowledge::observed(
        InitialTerrain {
            width: g.w,
            height: g.h,
            chars: &initial.chars,
            ground: Some(&initial.ground),
            objects: Some(&initial.objects),
            heights: initial.height.as_deref(),
            shoals: shoals.as_deref(),
            house_kinds: Some(&g.house),
            toughness: g.tough.as_deref(),
            trench_front: initial.trench_front.as_deref(),
        },
        owner as u64,
        m.rules.clone(),
    )
    .unwrap();
    let mut memory = ObservedMemory::new(&m.world, owner, owner as u64).unwrap();
    // Refresh acknowledges remembered log rows, as the Source query does. No terrain setter is used.
    memory
        .refresh(
            &mut m.world,
            ObservationFrame {
                team,
                skip_fog: false,
            },
            &mut facts,
        )
        .unwrap();
    memory.apply_routes(&mut routes).unwrap();
    let g = &m.world.game;
    let actor = &g.units[&actor_id];
    let bunkers = g
        .units
        .values()
        .filter(|u| {
            m.rules.kinds[u.kind].name == "bunker"
                && u.hp > 0.0
                && (g.reveal || g.allied(u.owner, owner) || g.players[owner].sees(u.id))
        })
        .map(|u| u.at())
        .collect::<Vec<_>>();
    let fires = vec![false; n]; // Canonical empty fires were asserted above.
    let query = routes.query();
    Pathfinding::new()
        .find_path(
            query
                .path_terrain(LiveRouting {
                    fires: &fires,
                    bunkers: &bunkers,
                    wetness: g.wet.unwrap_or(0.0),
                    roads: m.world.roads,
                })
                .unwrap(),
            PathRules::compiled().unwrap(),
            &m.rules.kinds[actor.kind],
            actor.at(),
            target,
            None,
        )
        .unwrap()
}
#[test]
fn explicit_assault_start_matches_original_create_game() {
    let fixture = fixture();
    assert_eq!(
        fixture["reference"],
        "e4e579a838aabae3b1d9dadbc17a0a87147bc52d"
    );
    let mut units = 0;
    let mut named_xl = std::collections::BTreeSet::new();
    let mut named_hill = std::collections::BTreeSet::new();
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
        assert!(
            equivalent(
                &m.scenario.as_ref().map_or(Value::Null, |s| s.state_json()),
                &expected["scenario"]
            ),
            "case{index} legacy trigger state"
        );
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
            let firing = m.firing.get(unit.id).expect("canonical firing metadata");
            assert_eq!(
                bits(firing.cooldown),
                bits(raw["cooldown"].as_f64().unwrap())
            );
            assert_eq!(
                bits(firing.retarget),
                bits(raw["retarget"].as_f64().unwrap())
            );
            assert_eq!(firing.target_id as u64, raw["targetId"].as_u64().unwrap());
            let combat = m.combat.get(unit.id).expect("canonical combat metadata");
            assert_eq!(bits(combat.xp), bits(raw["xp"].as_f64().unwrap()));
            assert_eq!(bits(combat.hit_at), bits(raw["hitAt"].as_f64().unwrap()));
            assert_eq!(
                combat.hit_by.unwrap_or(0) as u64,
                raw["hitBy"].as_u64().unwrap()
            );
            assert!(
                combat.cargo.is_empty() && combat.carrier.is_none() && combat.last_hit.is_none()
            );
            if index < 43 {
                units += 1;
            }
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
        if let Some(selected) = c.get("selectedCheck") {
            assert!(index >= 43, "named cases must follow the original corpus");
            assert!(m.options.supply && m.options.living_weather);
            let map_name = fixture["maps"][c["map"].as_u64().unwrap() as usize]["name"]
                .as_str()
                .unwrap();
            assert_eq!(
                map_name,
                format!("{}.json", selected["map"].as_str().unwrap())
            );
            match selected["kind"].as_str().unwrap() {
                "xl" => {
                    assert!(named_xl.insert(map_name.to_owned()));
                    assert_eq!(m.start.players.len(), 6);
                    assert!(!m.options.shuffle);
                    assert_eq!(map.spawns().len(), 6);
                    assert_eq!(map.defend(), Some([0usize, 1, 2].as_slice()));
                    assert_eq!(bits(state.time_left), selected["clock"]);
                    let bunkers: Vec<_> = m
                        .world
                        .game
                        .units
                        .values()
                        .filter(|unit| m.rules.kinds[unit.kind].name == "bunker")
                        .map(|unit| unit.id)
                        .collect();
                    assert_eq!(bunkers.len(), 3);
                    assert_eq!(json!(bunkers), selected["bunkers"]);
                    assert_eq!(state.total, 3);
                }
                "hill" => {
                    assert!(m.options.shuffle);
                    assert_eq!(m.start.players.len(), 3);
                    assert!(named_hill.insert(selected["shuffleCase"].as_u64().unwrap()));
                    let game = &m.world.game;
                    let levels: Vec<_> = m
                        .start
                        .players
                        .iter()
                        .map(|player| {
                            game.height.as_ref().unwrap()
                                [game.cell_of(player.spawn.x, player.spawn.z).unwrap()]
                        })
                        .collect();
                    assert_eq!(levels, [0i8, 0, 4]);
                    assert_eq!(json!(levels), selected["levels"]);
                    let route = &selected["route"];
                    let actor = &game.units[&(route["actor"].as_u64().unwrap() as u32)];
                    assert_eq!(actor.owner, 0);
                    assert_eq!(m.rules.kinds[actor.kind].name, "rifle");
                    assert_eq!(json!([bits(actor.x), bits(actor.z)]), route["from"]);
                    let target = m.start.players[2].spawn;
                    assert_eq!(json!([bits(target.x), bits(target.z)]), route["target"]);
                    assert!(!route["path"].as_array().unwrap().is_empty());
                    let actor_id = actor.id;
                    let native = hill_raw_route(&mut m, actor_id, target);
                    assert_eq!(native.status, PathStatus::Found, "case{index} Hill route");
                    assert!(
                        !native.waypoints.is_empty(),
                        "a way up from the HQ doorstep"
                    );
                    assert_eq!(
                        json!(
                            native
                                .waypoints
                                .iter()
                                .map(|p| [bits(p.x), bits(p.z)])
                                .collect::<Vec<_>>()
                        ),
                        route["path"],
                        "case{index} complete Hill waypoint bits"
                    );
                    // Live owned-recipient routing stays refused separately from the raw component query.
                    assert_eq!(
                        m.refresh_recipient_routing(0).unwrap_err(),
                        "owned routing currently supports ordinary Conquest"
                    );
                }
                kind => panic!("unknown selected Assault check {kind}"),
            }
        } else {
            assert!(index < 43, "new cases need their named-check scope");
        }
        // Constructor availability must not quietly admit the incomplete mode to a Conquest phase.
        assert!(m.apply_conquest_victory().is_err());
        assert!(m.sample_conquest_tick().is_err());
        assert!(m.apply_point_economy(&[], false).is_err());
    }
    assert_eq!(fixture["cases"].as_array().unwrap().len(), 57);
    let validation = fixture["selectedValidation"].as_array().unwrap();
    assert_eq!(validation.len(), 2);
    for invalid in validation {
        assert!(
            AuthoritativeMap::from_json(&serde_json::to_vec(&invalid["map"]).unwrap()).is_err()
        );
    }
    // Retain the exact actor-count guard for all original 43 cases.
    assert_eq!(units, 702);
    assert_eq!(
        named_xl,
        [
            "hill-112-xl.json",
            "monte-cassino-xl.json",
            "pegasus-bridge-xl.json",
            "seawall-xl.json",
        ]
        .map(str::to_owned)
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>()
    );
    assert_eq!(
        named_hill,
        (0u64..10).collect::<std::collections::BTreeSet<_>>()
    );
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
