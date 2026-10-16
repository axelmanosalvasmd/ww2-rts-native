use serde_json::{Value, json};
use ww2_core::{
    economy::{Faction, Mode, Price, TechState, Technology},
    economy_state::{
        ArmySize, EconomyState, ModeContext, PendingDrop, PlayerEconomy, Region, RegionKind,
        WorldContext,
    },
    production::*,
};
fn a(v: &Value) -> &[Value] {
    v.as_array().unwrap()
}
fn n(v: &Value) -> usize {
    v.as_u64().unwrap() as usize
}
fn f(v: &Value) -> f64 {
    v.as_f64().unwrap()
}
fn b(v: &Value) -> bool {
    v.as_bool().unwrap_or(false)
}
fn bits(v: f64) -> String {
    format!("{:016x}", v.to_bits())
}
fn kind(v: &Value) -> Technology {
    match v.as_str().unwrap() {
        "tier" => Technology::Tier,
        "inf" => Technology::Infantry,
        "guns" => Technology::Guns,
        "armor" => Technology::Armor,
        _ => panic!(),
    }
}
fn name(t: Technology) -> &'static str {
    match t {
        Technology::Tier => "tier",
        Technology::Infantry => "inf",
        Technology::Guns => "guns",
        Technology::Armor => "armor",
    }
}
fn reason(e: Error) -> Value {
    if let Error::Denied(d) = e {
        json!(match d {
            Denial::Blocked => "blocked",
            Denial::Needs => "needs",
            Denial::Tier => "tier",
            Denial::Mp => "mp",
            Denial::Mun => "mun",
            Denial::Fuel => "fuel",
            Denial::Pop => "pop",
            Denial::Max => "max",
            Denial::QueueFull => "queueFull",
            Denial::SuddenDeath => "suddenDeath",
            Denial::Unseen => "unseen",
            Denial::NotWaiting => "notWaiting",
        })
    } else {
        panic!("unexpected error {e:?}")
    }
}
fn apply(result: Result<(), Error>) -> Value {
    result.map_or_else(reason, |()| Value::Null)
}
fn context(config: &Value, run: impl FnOnce(&Context<'_>)) {
    let units: Vec<_> = a(&config["units"])
        .iter()
        .map(|u| UnitView {
            id: n(&u["id"]) as u32,
            owner: u["owner"].as_u64().map(|v| v as usize),
            kind: u["type"].as_str().unwrap(),
            hp: f(&u["hp"]),
            built: f(&u["built"]),
        })
        .collect();
    let drops: Vec<_> = a(&config["strikes"])
        .iter()
        .map(|d| PendingDrop {
            owner: n(&d["owner"]),
            kind: d["kind"].as_str().unwrap(),
            live: b(&d["live"]),
        })
        .collect();
    let regions: Vec<_> = a(&config["regions"])
        .iter()
        .map(|r| Region {
            team: r["team"].as_i64().unwrap() as i32,
            kind: RegionKind::Other,
        })
        .collect();
    run(&Context {
        mode: ModeContext {
            kind: Mode::from_kind(config["mode"]["kind"].as_str()),
            defender_team: None,
            income_excluded_slot: config["mode"]["slot"].as_u64().map(|s| s as usize),
        },
        army: ArmySize::Standard,
        tech_enabled: b(&config["tech"]),
        naval: b(&config["naval"]),
        sudden_death: b(&config["mode"]["suddenDeath"]),
        units: &units,
        drops: &drops,
        world: Some(WorldContext { regions: &regions }),
    });
}
fn snapshot(
    state: &ProductionState,
    wallets: &EconomyState,
    config: &Value,
    completed: Vec<Value>,
) -> Value {
    let players:Vec<_>=wallets.players.iter().enumerate().map(|(slot,p)|{let r=state.player(slot).unwrap();let t=r.tech();json!({"money":[bits(p.mp),bits(p.mun),bits(p.fuel)],"tier":t.tier(),"armory":{"inf":t.level(Technology::Infantry),"guns":t.level(Technology::Guns),"armor":t.level(Technology::Armor)},"lab":r.jobs().iter().map(|j|json!([name(j.kind),bits(j.progress),bits(j.time)])).collect::<Vec<_>>()})}).collect();
    let buildings:Vec<_>=a(&config["units"]).iter().map(|u|{let id=n(&u["id"]) as u32;let q=state.building(id);json!({"id":id,"progress":bits(q.map_or(0.0,BuildingProduction::progress)),"jobs":q.map_or_else(Vec::new,|q|q.rows().into_iter().map(|r|json!([r.job.id,r.job.kind,bits(r.job.charge.mp),bits(r.job.charge.fuel),if r.status==JobStatus::Active{"active"}else{"waiting"}])).collect::<Vec<_>>())})}).collect();
    let mut access = Vec::new();
    context(config, |c| {
        for slot in 0..wallets.players.len() {
            access.push(json!([
                state.production_buildings(c, wallets, slot, None).unwrap(),
                state
                    .production_buildings(c, wallets, slot, Some("rifle"))
                    .unwrap(),
                state.tech_labs(c, wallets, slot, Technology::Tier).unwrap(),
                state
                    .tech_labs(c, wallets, slot, Technology::Infantry)
                    .unwrap()
            ]));
        }
    });
    json!({"next":state.next_job_id(),"players":players,"buildings":buildings,"completed":completed,"access":access})
}
#[test]
fn production_research_and_refunds_match_original_source_traces() {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/production.json")).unwrap();
    assert_eq!(a(&fixture["scenarios"]).len(), 12);
    replay_production_fixture(&fixture, false);
}

fn replay_production_fixture(fixture: &Value, dynamic_makers: bool) {
    for split in [false, true] {
        for scenario in a(&fixture["scenarios"]) {
            let mut config = scenario["input"].clone();
            let tech = a(&config["players"])
                .iter()
                .map(|p| {
                    TechState::new(
                        n(&p["tier"]) as u8,
                        n(&p["armory"]["inf"]) as u8,
                        n(&p["armory"]["guns"]) as u8,
                        n(&p["armory"]["armor"]) as u8,
                    )
                    .unwrap()
                })
                .collect();
            let mut state = ProductionState::new(tech).unwrap();
            for unit in a(&config["units"]) {
                state.initialize_building(n(&unit["id"]) as u32).unwrap();
            }
            let mut wallets = EconomyState {
                players: a(&config["players"])
                    .iter()
                    .map(|p| PlayerEconomy {
                        team: p["team"].as_i64().unwrap() as i32,
                        mp: f(&p["mp"]),
                        mun: f(&p["mun"]),
                        fuel: f(&p["fuel"]),
                        ..Default::default()
                    })
                    .collect(),
            };
            for (tick, row) in a(&scenario["trace"]).iter().enumerate() {
                let action = &row["action"];
                let mut result = Value::Null;
                let mut completed = vec![];
                match action["op"].as_str().unwrap() {
                    "unit" => {
                        let id = n(&action["id"]);
                        let u = config["units"]
                            .as_array_mut()
                            .unwrap()
                            .iter_mut()
                            .find(|u| n(&u["id"]) == id)
                            .unwrap();
                        for (k, v) in action["fields"].as_object().unwrap() {
                            u[k] = v.clone();
                        }
                    }
                    "config" => {
                        for (k, v) in action["fields"].as_object().unwrap() {
                            config[k] = v.clone();
                        }
                    }
                    "player" => {
                        let slot = n(&action["slot"]);
                        let p = &mut wallets.players[slot];
                        for (k, v) in action["fields"].as_object().unwrap() {
                            match k.as_str() {
                                "mp" => p.mp = f(v),
                                "mun" => p.mun = f(v),
                                "fuel" => p.fuel = f(v),
                                "away" => p.away = b(v),
                                "out" => p.out = b(v),
                                _ => panic!("field {k}"),
                            }
                        }
                    }
                    "queue" => {
                        state
                            .reconcile_queue(
                                n(&action["id"]) as u32,
                                &a(&action["types"])
                                    .iter()
                                    .map(|s| s.as_str().unwrap())
                                    .collect::<Vec<_>>(),
                            )
                            .unwrap();
                    }
                    "paid" => {
                        state
                            .record_paid(
                                n(&action["id"]) as u32,
                                action["kind"].as_str().unwrap(),
                                Price {
                                    mp: f(&action["charge"]["mp"]),
                                    fuel: f(&action["charge"]["fuel"]),
                                },
                            )
                            .unwrap();
                    }
                    "clear" => state.clear_queues(),
                    op => context(&config, |c| {
                        result = match op {
                            "buy" => apply(
                                state
                                    .queue_unit(
                                        c,
                                        &mut wallets,
                                        n(&action["slot"]),
                                        Faction::try_from(n(
                                            &config["players"][n(&action["slot"])]["faction"]
                                        )
                                            as u8)
                                        .unwrap(),
                                        action["kind"].as_str().unwrap(),
                                        if dynamic_makers {
                                            source_maker(action)
                                        } else {
                                            action["from"].as_u64().map(|i| i as u32)
                                        },
                                    )
                                    .map(|_| ()),
                            ),
                            "tech" => apply(
                                state
                                    .request_research(
                                        c,
                                        &mut wallets,
                                        n(&action["slot"]),
                                        kind(&action["kind"]),
                                    )
                                    .map(|_| ()),
                            ),
                            "cancel" => apply(
                                state
                                    .cancel(
                                        c,
                                        &mut wallets,
                                        n(&action["slot"]),
                                        n(&action["id"]) as u32,
                                        action["job"].as_u64().unwrap(),
                                    )
                                    .map(|_| ()),
                            ),
                            "step" => {
                                for _ in 0..action["repeat"].as_u64().unwrap_or(1) {
                                    let dt = f(&action["dt"]);
                                    let units = if split {
                                        state.advance_research(c, &wallets, dt).unwrap();
                                        c.units
                                            .iter()
                                            .filter_map(|u| {
                                                state
                                                    .prepare_building_step(c, &wallets, u.id, dt)
                                                    .unwrap()
                                                    .commit()
                                            })
                                            .collect()
                                    } else {
                                        state.advance(c, &wallets, dt).unwrap().units
                                    };
                                    completed.extend(units.into_iter().map(|u| json!({"building":u.building,"owner":u.owner,"kind":u.kind,"job_id":u.job_id})));
                                }
                                Value::Null
                            }
                            _ => panic!("op {op}"),
                        };
                    }),
                }
                assert_eq!(
                    result, row["result"],
                    "{} action {tick}: {action}",
                    scenario["name"]
                );
                assert_eq!(
                    snapshot(&state, &wallets, &config, completed),
                    row["expected"],
                    "{} action {tick}: {action}",
                    scenario["name"]
                );
            }
        }
    }
}
#[test]
fn invalid_inputs_do_not_change_canonical_jobs_or_wallets() {
    let mut state = ProductionState::new(vec![TechState::new(1, 0, 0, 0).unwrap()]).unwrap();
    assert!(state.initialize_building(0).is_err());
    let empty = state.initialize_building(1).unwrap();
    assert!(empty.jobs().is_empty());
    assert_eq!(empty.progress(), 0.0);
    state
        .record_paid(
            1,
            "rifle",
            Price {
                mp: 12.5,
                fuel: 1.0,
            },
        )
        .unwrap();
    let initial = state.clone();
    state.initialize_building(1).unwrap();
    assert_eq!(state, initial);
    let wallets = EconomyState {
        players: vec![PlayerEconomy::default()],
    };
    let units = [UnitView {
        id: 1,
        owner: Some(0),
        kind: "hq",
        hp: 100.0,
        built: 1.0,
    }];
    let c = Context {
        mode: ModeContext {
            kind: Mode::Classic,
            ..Default::default()
        },
        army: ArmySize::Standard,
        tech_enabled: true,
        naval: false,
        sudden_death: false,
        units: &units,
        drops: &[],
        world: None,
    };
    for dt in [-1.0, f64::NAN, f64::INFINITY] {
        assert!(state.advance(&c, &wallets, dt).is_err());
        assert_eq!(state, initial);
    }
    assert!(
        state
            .record_paid(
                1,
                "rifle",
                Price {
                    mp: f64::NAN,
                    fuel: 0.0
                }
            )
            .is_err()
    );
    assert!(state.reconcile_queue(1, &["not-a-unit"]).is_err());
    assert_eq!(state, initial);
    let duplicate = [units[0], units[0]];
    let invalid = Context {
        units: &duplicate,
        ..c
    };
    let mut funds = wallets.clone();
    assert!(
        state
            .queue_unit(&invalid, &mut funds, 0, Faction::Usa, "rifle", Some(1))
            .is_err()
    );
    assert_eq!(funds, wallets);
    assert_eq!(state, initial);
}

#[test]
fn if25_production_shortest_queue_keeps_producer_input_order_on_ties() {
    for ids in [[9, 2], [2, 9]] {
        let mut state = ProductionState::new(vec![TechState::new(1, 0, 0, 0).unwrap()]).unwrap();
        let mut wallets = EconomyState {
            players: vec![PlayerEconomy {
                mp: 10_000.0,
                ..Default::default()
            }],
        };
        // Classic rifles train at HQs. The Barracks rifle override belongs
        // only to the separate skirmish recruitment path.
        let units = ids.map(|id| UnitView {
            id,
            owner: Some(0),
            kind: "hq",
            hp: 100.0,
            built: 1.0,
        });
        let c = Context {
            mode: ModeContext {
                kind: Mode::Classic,
                ..Default::default()
            },
            army: ArmySize::Standard,
            tech_enabled: false,
            naval: false,
            sudden_death: false,
            units: &units,
            drops: &[],
            world: None,
        };
        for expected in [ids[0], ids[1], ids[0]] {
            let prepared = state
                .prepare_queue_unit(&c, &mut wallets, 0, Faction::Usa, "rifle", None)
                .unwrap();
            assert_eq!(prepared.building(), expected);
            prepared.commit();
        }
        assert_eq!(state.building(ids[0]).unwrap().jobs().len(), 2);
        assert_eq!(state.building(ids[1]).unwrap().jobs().len(), 1);
    }
}

#[test]
fn aborted_research_and_training_guards_preserve_canonical_state() {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/production.json")).unwrap();
    let config = &fixture["scenarios"][0]["input"];
    let mut state = ProductionState::new(vec![TechState::new(1, 0, 0, 0).unwrap(); 3]).unwrap();
    state
        .record_paid(
            1,
            "rifle",
            Price {
                mp: 123.5,
                fuel: 7.25,
            },
        )
        .unwrap();
    let mut wallets = EconomyState {
        players: (0..3)
            .map(|slot| PlayerEconomy {
                team: if slot == 2 { 1 } else { 0 },
                mp: 4000.0,
                mun: 400.0,
                fuel: 400.0,
                ..Default::default()
            })
            .collect(),
    };
    let before = state.clone();
    let money = wallets.clone();
    context(config, |c| {
        let research = state
            .prepare_research(c, &mut wallets, 0, Technology::Tier)
            .unwrap();
        assert!(research.cost().mp > 0.0);
        drop(research);
        let training = state.prepare_building_step(c, &wallets, 1, 100.0).unwrap();
        assert_eq!(training.completed().unwrap().kind, "rifle");
        drop(training);
    });
    assert_eq!(state, before);
    assert_eq!(wallets, money);
}

// A present noncanonical Source maker is an invalid typed ID, never omission.
// Tagged NaN/Infinity retain their Source identity in the fixture. The u32
// production API cannot represent them; this adapter does not add a JSON API.
fn source_maker(action: &Value) -> Option<u32> {
    action.get("from").map(|value| {
        value
            .as_f64()
            .filter(|n| n.is_finite() && n.fract() == 0.0 && *n > 0.0 && *n <= u32::MAX as f64)
            .map_or(0, |n| n as u32)
    })
}

#[test]
fn base_production_existing_runtime_predicates_match_source() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/base-production-predicates.json")).unwrap();
    assert_eq!(
        fixture["reference"],
        "e4e579a838aabae3b1d9dadbc17a0a87147bc52d"
    );
    assert_eq!(a(&fixture["scenarios"]).len(), 5);
    assert_eq!(a(&fixture["scenarios"][0]["trace"]).len(), 10);
    let variants = a(&fixture["scenarios"][0]["trace"])[..8]
        .iter()
        .map(|row| row["action"]["variant"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        variants,
        [
            "foreign",
            "nonmaker",
            "numeric-string",
            "NaN",
            "Infinity",
            "null",
            "zero",
            "object"
        ]
    );
    replay_production_fixture(&fixture, true);
}
