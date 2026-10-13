use serde_json::{Value, json};
use ww2_core::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::{Point, Strike, StrikeKind},
    map::AuthoritativeMap,
    match_runtime::{Match, MatchOptions, PlayerConfig, RecruitError, RecruitRouting},
    observed_memory::ObservedMemory,
    pathfinding::PathStatus,
    route_knowledge::{
        CellMetadata, InitialTerrain, LiveRouting, Material, RememberedCell, RememberedSection,
        RouteKnowledge,
    },
    weather::WeatherChoice,
};
fn bits(n: f64) -> Value {
    json!(format!("{:016x}", n.to_bits()))
}
fn point(v: &Value) -> Point {
    Point {
        x: v["x"].as_f64().unwrap(),
        z: v["z"].as_f64().unwrap(),
    }
}
fn position(p: Option<Point>) -> Value {
    p.map_or(Value::Null, |p| json!([bits(p.x), bits(p.z)]))
}
fn make(map: &Value) -> Match {
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(map).unwrap()).unwrap();
    Match::new(
        &map,
        MatchOptions {
            seed: 701,
            mode: Mode::Conquest,
            players: vec![
                PlayerConfig {
                    name: "A".into(),
                    team: 0,
                    faction: Faction::Usa,
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
            weather_seed: Some(217),
            living_weather: false,
            map_key: None,
            tech: false,
            supply: true,
            logistics: false,
            sandbox: false,
        },
    )
    .unwrap()
}
fn routes(m: &Match, slot: usize) -> (ObservedMemory, RouteKnowledge) {
    let initial = m.world.initial();
    let identity = 1000 + slot as u64;
    let shoals = m
        .world
        .shoal
        .as_ref()
        .map(|v| v.iter().map(|n| *n != 0).collect::<Vec<_>>());
    (
        ObservedMemory::new(&m.world, slot, identity).unwrap(),
        RouteKnowledge::observed(
            InitialTerrain {
                width: m.world.game.w,
                height: m.world.game.h,
                chars: &initial.chars,
                ground: Some(&initial.ground),
                objects: Some(&initial.objects),
                heights: initial.height.as_deref(),
                shoals: shoals.as_deref(),
                house_kinds: Some(&m.world.game.house),
                toughness: m.world.game.tough.as_deref(),
                trench_front: initial.trench_front.as_deref(),
            },
            identity,
            m.rules.clone(),
        )
        .unwrap(),
    )
}
fn material(v: &Value) -> Option<Material> {
    match v.as_str() {
        Some("soil") => Some(Material::Soil),
        Some("road") => Some(Material::Road),
        Some("wood") => Some(Material::Wood),
        Some("stone") => Some(Material::Stone),
        Some("concrete") => Some(Material::Concrete),
        Some("steel") => Some(Material::Steel),
        Some("water") => Some(Material::Water),
        None => None,
        _ => panic!("material"),
    }
}
fn row(v: &Value) -> RememberedCell {
    let r = &v["row"];
    let d = &r[4];
    let section = &d["section"];
    RememberedCell {
        cell: r[0].as_u64().unwrap() as usize,
        revision: v["revision"].as_u64().unwrap(),
        character: r[1].as_str().unwrap().as_bytes()[0],
        height: r[2].as_i64().unwrap() as i8,
        state: r[3].as_u64().unwrap() as u8,
        metadata: CellMetadata {
            ground: d["ground"].as_str().map(|s| s.as_bytes()[0]),
            object: d["object"].as_str().map(|s| s.as_bytes()[0]),
            material: material(&d["material"]),
            section: (!section.is_null()).then(|| RememberedSection {
                hp: section["hp"].as_f64().unwrap(),
                max_hp: section["maxHp"].as_f64(),
                failed: section["state"].as_str() == Some("failed"),
                material: material(&section["material"]),
            }),
            trench_front: d["trenchFront"].as_f64(),
            mine_owned: d["mineOwned"].as_bool().unwrap_or(false),
        },
        cover_basis: None,
    }
}
fn reason(e: RecruitError) -> Value {
    use ww2_core::production::{Denial, Error};
    if let RecruitError::Admission(Error::Denied(d)) = e {
        json!(match d {
            Denial::Blocked => "blocked",
            Denial::Needs => "needs",
            Denial::Tier => "tier",
            Denial::Mp => "mp",
            Denial::Pop => "pop",
            Denial::Max => "max",
            _ => panic!("denial"),
        })
    } else {
        panic!("{e:?}")
    }
}
#[test]
fn original_match_buy_placement_path_and_grid() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/match-recruitment.json")).unwrap();
    let mut failed_paths = 0;
    let mut admitted = 0;
    for case in fixture["cases"].as_array().unwrap() {
        let mut m = make(&case["map"]);
        m.start.economy.players[0].mp = 5000.125;
        let mut recipients: Vec<_> = (0..2).map(|slot| routes(&m, slot)).collect();
        let mut winner = false;
        for (step, stage) in case["stages"].as_array().unwrap().iter().enumerate() {
            m.world.game.tick += 1;
            let action = &stage["action"];
            let slot = action["slot"].as_u64().unwrap_or(0) as usize;
            match action["op"].as_str().unwrap() {
                "player" => {
                    let fields = &action["fields"];
                    if let Some(v) = fields["mp"].as_f64() {
                        m.start.economy.players[slot].mp = v;
                    }
                    if let Some(v) = fields["away"].as_bool() {
                        m.start.economy.players[slot].away = v;
                    }
                    if let Some(v) = fields["out"].as_bool() {
                        m.start.economy.players[slot].out = v;
                    }
                    if !fields["rally"].is_null() {
                        m.player_rallies[slot] = Some(point(&fields["rally"]));
                    }
                }
                "maker" => {
                    let fields = &action["fields"];
                    let index = fields["type"]
                        .as_str()
                        .map(|name| m.rules.kind(name).unwrap());
                    let u = m
                        .world
                        .game
                        .units
                        .get_mut(&(action["id"].as_u64().unwrap() as u32))
                        .unwrap();
                    if let Some(v) = fields["hp"].as_f64() {
                        u.hp = v;
                    }
                    if let Some(v) = fields["built"].as_f64() {
                        u.built = Some(v);
                    }
                    if let Some(v) = index {
                        u.kind = v;
                    }
                }
                "rally" => {
                    m.building_rallies.insert(
                        action["id"].as_u64().unwrap() as u32,
                        Some(point(&action["to"])),
                    );
                }
                "winner" => winner = action["value"].as_bool().unwrap(),
                "buy" => {
                    let r = &stage["routing"];
                    let rows: Vec<_> = r["rows"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .map(|id| row(&case["rows"][id.as_u64().unwrap() as usize - 1]))
                        .collect();
                    let (memory, route) = &mut recipients[slot];
                    route.apply_rows(step as u64 + 1, &rows).unwrap();
                    let query = route.query();
                    let mut fires = vec![false; m.world.game.w * m.world.game.h];
                    for c in r["fires"].as_array().into_iter().flatten() {
                        fires[c.as_u64().unwrap() as usize] = true;
                    }
                    let bunkers: Vec<_> = r["bunkers"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .map(point)
                        .collect();
                    let live = LiveRouting {
                        fires: &fires,
                        bunkers: &bunkers,
                        wetness: r["wet"].as_f64().unwrap_or(0.0),
                        roads: r["roads"].as_bool().unwrap_or(false),
                    };
                    let routing = (!r.is_null()).then(|| {
                        RecruitRouting::new(memory, m.world.game.players[slot].team, &query, live)
                            .unwrap()
                    });
                    let result = m.recruit(
                        slot,
                        action["kind"].as_str().unwrap(),
                        action["from"].as_u64().map(|id| id as u32),
                        winner,
                        routing,
                    );
                    let (reason, unit) = match result {
                        Ok(done) => {
                            admitted += 1;
                            assert_eq!(
                                m.combat.get(done.unit),
                                Some(&ww2_core::match_combat::UnitCombat::default()),
                                "recruits must enter combat with canonical default metadata"
                            );
                            assert_eq!(
                                m.abilities.get(done.unit),
                                Some(&ww2_core::match_abilities::AbilityUnit::default()),
                                "recruits must enter with canonical default ability state"
                            );
                            assert_eq!(
                                m.recovery.get(done.unit),
                                Some(&ww2_core::match_recovery::RecoveryUnit::default()),
                                "recruits must enter with canonical default recovery state"
                            );
                            assert_eq!(
                                m.firing.get(done.unit),
                                Some(&ww2_core::match_firing::FiringUnit::default()),
                                "recruits must publish canonical target and timer defaults"
                            );
                            let u = &m.world.game.units[&done.unit];
                            let control = &m.commands.units[&done.unit];
                            let failed = done.route_status == Some(PathStatus::Unreachable);
                            failed_paths += usize::from(failed);
                            (
                                Value::Null,
                                json!({"id":u.id,"owner":u.owner,"kind":m.rules.kinds[u.kind].name,"pose":[bits(u.x),bits(u.z),bits(u.rot),bits(control.aim),bits(u.ghost)],"hp":bits(u.hp),"path":u.path.iter().map(|p|position(Some(*p))).collect::<Vec<_>>(),"goal":position(u.world_goal),"routeEnd":position(control.route_end),"drift":u.drift,"auto":control.auto_cast,"autoRetreat":control.auto_retreat,"grid":m.grid.in_order(&m.world.game.units,(u.x,u.z),20.0,|_,_|true),"producer":done.purchase.request.building,"routeFailed":failed}),
                            )
                        }
                        Err(e) => (reason(e), Value::Null),
                    };
                    assert_eq!(
                        json!({"result":reason,"next":m.next_unit_id,"money":m.start.economy.players.iter().map(|p|bits(p.mp)).collect::<Vec<_>>(),"unit":unit}),
                        stage["expected"],
                        "{} step {step}",
                        case["name"]
                    );
                }
                _ => panic!("action"),
            }
        }
    }
    assert!(admitted > 0 && failed_paths > 0);
}
fn unchanged(m: &Match) -> Value {
    json!({"next":m.next_unit_id,"money":m.start.economy.players.iter().map(|p|bits(p.mp)).collect::<Vec<_>>(),"units":m.world.game.units.keys().collect::<Vec<_>>(),"metadata":format!("{:?}",m.recruit_metadata),"firing":format!("{:?}",m.firing),"controls":format!("{:?}",m.commands.units),"unit_state":format!("{:?}",m.world.game.units),"wallets":format!("{:?}",m.start.economy),"grid":m.grid.in_order(&m.world.game.units,(0.0,0.0),1000.0,|_,_|true)})
}
#[test]
fn malformed_or_unsupported_staging_never_publishes() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/match-recruitment.json")).unwrap();
    let mut m = make(&fixture["cases"][0]["map"]);
    m.start.economy.players[0].mp = 5000.0;
    let before = unchanged(&m);
    m.player_rallies[0] = Some(Point { x: 35.0, z: 35.0 });
    assert!(matches!(
        m.recruit(0, "rifle", None, false, None),
        Err(RecruitError::InvalidState(_))
    ));
    assert_eq!(unchanged(&m), before);
    let (memory, route) = routes(&m, 1);
    let query = route.query();
    let fires = vec![false; m.world.game.w * m.world.game.h];
    let live = LiveRouting {
        fires: &fires,
        bunkers: &[],
        wetness: 0.0,
        roads: false,
    };
    let wrong = RecruitRouting::new(&memory, 1, &query, live).unwrap();
    assert!(matches!(
        m.recruit(0, "rifle", None, false, Some(wrong)),
        Err(RecruitError::InvalidState(_))
    ));
    assert_eq!(unchanged(&m), before);
    m.player_rallies[0] = None;
    m.next_unit_id = u32::MAX;
    let before = unchanged(&m);
    assert!(m.recruit(0, "rifle", None, false, None).is_err());
    assert_eq!(unchanged(&m), before);
    m.next_unit_id = 1;
    let before = unchanged(&m);
    assert!(m.recruit(0, "rifle", None, false, None).is_err());
    assert_eq!(unchanged(&m), before);
    m.next_unit_id = 100;
    let hq = m
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 0 && m.rules.kinds[u.kind].name == "hq")
        .unwrap()
        .id;
    let air = m.rules.kind("airfield").unwrap();
    m.world.game.units.get_mut(&hq).unwrap().kind = air;
    let before = unchanged(&m);
    assert!(matches!(
        m.recruit(0, "fighter", None, false, None),
        Err(RecruitError::Placement(
            ww2_core::produced_units::Error::UnsupportedAir
        ))
    ));
    assert_eq!(unchanged(&m), before);
    m.world.game.units.get_mut(&hq).unwrap().kind = m.rules.kind("hq").unwrap();
    m.world.game.strikes.push(Strike {
        live: false,
        kind: StrikeKind::Other,
        owner: 0,
        x: 1.0,
        z: 1.0,
        dir: 0.0,
    });
    let before = unchanged(&m);
    assert!(matches!(
        m.recruit(0, "rifle", None, false, None),
        Err(RecruitError::InvalidState(_))
    ));
    assert_eq!(unchanged(&m), before);
    m.world.game.strikes[0].kind = StrikeKind::Recon;
    assert!(m.recruit(0, "rifle", None, false, None).is_ok());
    m.player_rallies[0] = None;
    m.firing.initialize(m.next_unit_id);
    let before = unchanged(&m);
    assert!(matches!(
        m.recruit(0, "rifle", None, false, None),
        Err(RecruitError::InvalidState(_))
    ));
    assert_eq!(unchanged(&m), before);
}
