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

#[test]
fn classic_price_boundary_three_paid_rifles_then_atomic_mp_refusal() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/base13-paid-price.json");
    let fixture: Value = serde_json::from_str(
        &std::fs::read_to_string(path)
            .expect("Root must grant authentic BASE13 Source generation before this test"),
    )
    .unwrap();
    assert_eq!(
        fixture["reference"],
        "e4e579a838aabae3b1d9dadbc17a0a87147bc52d"
    );
    assert_eq!(fixture["price"], json!({"mp": 100, "fuel": 0}));
    assert_eq!(fixture["initialCount"], 3);
    let input = &fixture["input"];
    assert_eq!(input["mode"], json!({"kind": "classic"}));
    assert_eq!(input["army"], "standard");
    assert_eq!(input["tech"], false);
    assert_eq!(a(&input["players"]).len(), 1);
    assert_eq!(a(&input["units"]).len(), 1);
    assert_eq!(input["units"][0]["id"], 1);
    assert_eq!(input["units"][0]["type"], "hq");
    assert_eq!(input["players"][0]["mp"], 301);
    assert!(a(&input["units"][0]["queue"]).is_empty());
    let mut state = ProductionState::new(vec![TechState::new(1, 0, 0, 0).unwrap()]).unwrap();
    state.initialize_building(1).unwrap();
    let mut wallets = EconomyState {
        players: vec![PlayerEconomy {
            team: 0,
            mp: 301.0,
            ..Default::default()
        }],
    };
    let projection = |state: &ProductionState, wallets: &EconomyState| {
        let payer = &wallets.players[0];
        let maker = state.building(1).unwrap();
        let jobs: Vec<_> = maker
            .rows()
            .into_iter()
            .map(|row| {
                json!([
                    row.job.id,
                    row.job.kind,
                    bits(row.job.charge.mp),
                    bits(row.job.charge.fuel),
                    if row.status == JobStatus::Active {
                        "active"
                    } else {
                        "waiting"
                    }
                ])
            })
            .collect();
        json!({"next": state.next_job_id(),
            "money": [bits(payer.mp), bits(payer.mun), bits(payer.fuel)],
            "progress": bits(maker.progress()), "jobs": jobs})
    };
    // Compare all five original client buyCount observations with real admission
    // attempts on copies of the current canonical component, without wallet edits.
    let affordable = |state: &ProductionState, wallets: &EconomyState| {
        let mut probe = state.clone();
        let mut money = wallets.clone();
        let mut count = 0;
        context(input, |c| {
            for _ in 0..5 {
                match probe.queue_unit(c, &mut money, 0, Faction::Usa, "rifle", Some(1)) {
                    Ok(_) => count += 1,
                    Err(Error::Denied(Denial::Mp)) => break,
                    other => panic!("unexpected paid-count admission: {other:?}"),
                }
            }
        });
        count
    };
    assert_eq!(projection(&state, &wallets), fixture["initial"]);
    assert_eq!(affordable(&state, &wallets), 3);
    assert_eq!(a(&fixture["trace"]).len(), 4);
    let mut accepted = 0;
    for (index, row) in a(&fixture["trace"]).iter().enumerate() {
        assert_eq!(
            row["action"],
            json!({"slot": 0, "kind": "rifle", "from": 1})
        );
        let before = projection(&state, &wallets);
        context(input, |c| {
            let actual = state.queue_unit(c, &mut wallets, 0, Faction::Usa, "rifle", Some(1));
            let result = match actual {
                Ok(job) => {
                    accepted += 1;
                    assert_eq!(job, (index + 1) as u64);
                    Value::Null
                }
                Err(error) => reason(error),
            };
            assert_eq!(result, row["result"]);
            assert_eq!(result, if index < 3 { Value::Null } else { json!("mp") });
        });
        let actual = projection(&state, &wallets);
        assert_eq!(actual, row["expected"], "paid buy {index}");
        let remaining = affordable(&state, &wallets);
        assert_eq!(json!(remaining), row["remainingCount"]);
        assert_eq!(remaining, [2, 1, 0, 0][index]);
        assert_eq!(
            wallets.players[0].mp,
            if index < 3 {
                301.0 - 100.0 * (index + 1) as f64
            } else {
                1.0
            }
        );
        assert_eq!(wallets.players[0].mun, 0.0);
        assert_eq!(wallets.players[0].fuel, 0.0);
        assert_eq!(state.building(1).unwrap().rows().len(), (index + 1).min(3));
        if index == 3 {
            assert_eq!(
                actual, before,
                "refused buy cannot change wallet, jobs, progress or next ID"
            );
        }
    }
    assert_eq!(accepted, 3);
    assert_eq!(json!(accepted), fixture["initialCount"]);
    assert_eq!(state.next_job_id(), 3);
}

// External authentic captures remain absent until a new exact Root Source grant.
// Card sentences are original client observations. Native queries below compare
// admission and count only; they do not implement a native card-text provider.
fn base13_control_input(makers: &[&str], fuel: u32) -> Value {
    let units: Vec<_> = makers
        .iter()
        .enumerate()
        .map(|(index, kind)| {
            json!({"id": index + 1, "owner": 0, "type": kind, "hp": 1000, "built": 1,
                "x": 0, "z": 0, "cd": 0, "flags": 0, "queue": [], "productionJobs": [], "prog": 0})
        })
        .collect();
    json!({"mode": {"kind": "classic", "suddenDeath": false}, "army": "standard",
        "tech": false, "naval": false,
        "players": [{"slot": 0, "team": 0, "faction": 0, "tier": 1,
            "armory": {"inf": 0, "guns": 0, "armor": 0}, "lab": [],
            "mp": 10000, "mun": 0, "fuel": fuel}],
        "units": units, "strikes": [], "regions": [], "nextProductionJob": 0})
}

fn base13_action(kind: &str, from: Option<u32>) -> Value {
    let mut action = json!({"t": "buy", "unit": kind, "slot": 0});
    if let Some(id) = from {
        action["from"] = json!(id);
    }
    action
}

fn base13_buy(kind: &str, from: Option<u32>, denial: Option<&str>) -> Value {
    json!({"op": "buy", "action": base13_action(kind, from), "result": denial})
}

fn base13_observe(kind: &str, from: Option<u32>, count: usize, sentence: &str) -> Value {
    json!({"op": "observe", "action": base13_action(kind, from), "count": count,
        "card": {"ok": sentence.is_empty(), "reason": sentence}})
}

fn base13_controls_projection(
    state: &ProductionState,
    wallets: &EconomyState,
    makers: &[&str],
) -> Value {
    let payer = &wallets.players[0];
    let buildings: Vec<_> = makers
        .iter()
        .enumerate()
        .map(|(index, _)| {
            let id = (index + 1) as u32;
            let maker = state.building(id).unwrap();
            let jobs: Vec<_> = maker
                .rows()
                .into_iter()
                .map(|row| {
                    json!([row.job.id, row.job.kind, bits(row.job.charge.mp),
                        bits(row.job.charge.fuel),
                        if row.status == JobStatus::Active { "active" } else { "waiting" }])
                })
                .collect();
            json!({"id": id, "progress": bits(maker.progress()),
                "queue": maker.queue().collect::<Vec<_>>(), "jobs": jobs})
        })
        .collect();
    json!({"next": state.next_job_id(),
        "money": [bits(payer.mp), bits(payer.mun), bits(payer.fuel)], "buildings": buildings})
}

fn base13_control(
    name: &str,
    makers: &[&str],
    fuel: u32,
    steps: &[Value],
    calls: usize,
    probes: usize,
) {
    let path = std::env::var_os("BASE13_CONTROLS_INPUT")
        .expect("ABSENT: new exact Root Source grant must produce external BASE13 controls");
    let path = std::fs::canonicalize(path).expect("authentic external BASE13 controls must exist");
    let repository = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap();
    assert!(!path.starts_with(repository));
    let fixture: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(
        fixture["reference"],
        "e4e579a838aabae3b1d9dadbc17a0a87147bc52d"
    );
    assert_eq!(fixture["schema"], "base13-paid-controls-v1");
    assert_eq!(
        fixture["inputPolicy"],
        "independent authored inputs; no live rewrites; strict present values; no comparison mappings"
    );
    let scenarios = a(&fixture["scenarios"]);
    let names: Vec<_> = scenarios
        .iter()
        .map(|row| row["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        [
            "queue-room",
            "no-barracks",
            "selected-full",
            "tank-fuel-low",
            "tank-fuel-paid"
        ]
    );
    let scenario = scenarios.iter().find(|row| row["name"] == name).unwrap();
    // Validate every declared input against authored literals before setup.
    // Missing, null, false and zero remain distinct. No recorded state is loaded.
    let input = base13_control_input(makers, fuel);
    assert_eq!(scenario["input"], input);
    let mut state = ProductionState::new(vec![TechState::new(1, 0, 0, 0).unwrap()]).unwrap();
    for index in 0..makers.len() {
        state.initialize_building((index + 1) as u32).unwrap();
    }
    let mut wallets = EconomyState {
        players: vec![PlayerEconomy {
            team: 0,
            mp: 10000.0,
            mun: 0.0,
            fuel: f64::from(fuel),
            ..Default::default()
        }],
    };
    assert_eq!(
        base13_controls_projection(&state, &wallets, makers),
        scenario["initial"]
    );
    assert_eq!(a(&scenario["trace"]).len(), steps.len());
    let mut actual_calls = 0;
    let mut actual_probes = 0;
    for (row, step) in a(&scenario["trace"]).iter().zip(steps) {
        assert_eq!(row["op"], step["op"]);
        assert_eq!(row["action"], step["action"]);
        let action = &step["action"];
        let kind = action["unit"].as_str().unwrap();
        // Only an absent from means automatic selection. Present null is invalid.
        let from = action
            .get("from")
            .map(|id| u32::try_from(id.as_u64().unwrap()).unwrap());
        let before = state.clone();
        let money_before = wallets.clone();
        if step["op"] == "buy" {
            actual_calls += 1;
            context(&input, |c| {
                let result = state.queue_unit(c, &mut wallets, 0, Faction::Usa, kind, from);
                let actual = result.map_or_else(reason, |_| Value::Null);
                assert_eq!(actual, step["result"]);
                assert_eq!(
                    &actual,
                    row.get("result")
                        .expect("Source Buy result must be present, including null")
                );
                if !actual.is_null() {
                    assert_eq!(
                        state, before,
                        "refusal preserves the entire production component"
                    );
                    assert_eq!(wallets, money_before, "refusal preserves every wallet field");
                }
            });
        } else {
            assert_eq!(step["op"], "observe");
            assert_eq!(row["card"], step["card"]);
            assert_eq!(row["count"], step["count"]);
            let mut probe = state.clone();
            let mut money = wallets.clone();
            let mut count = 0;
            context(&input, |c| {
                for _ in 0..5 {
                    actual_probes += 1;
                    let prior = probe.clone();
                    let prior_money = money.clone();
                    match probe.queue_unit(c, &mut money, 0, Faction::Usa, kind, from) {
                        Ok(_) => count += 1,
                        Err(Error::Denied(_)) => {
                            assert_eq!(probe, prior);
                            assert_eq!(money, prior_money);
                            break;
                        }
                        other => panic!("unexpected paid-control admission: {other:?}"),
                    }
                }
            });
            assert_eq!(json!(count), row["count"]);
            assert_eq!(json!(count > 0), row["card"]["ok"]);
            assert_eq!(state, before);
            assert_eq!(wallets, money_before);
        }
        assert_eq!(
            base13_controls_projection(&state, &wallets, makers),
            row["expected"]
        );
    }
    assert_eq!(actual_calls, calls);
    assert_eq!(actual_probes, probes);
}

#[test]
#[ignore = "held until new exact Root Source and native grants for external BASE13 controls"]
fn classic_queue_room_after_three_paid_rifles() {
    let mut steps = vec![base13_observe("rifle", Some(1), 5, "")];
    steps.extend((0..3).map(|_| base13_buy("rifle", Some(1), None)));
    steps.push(base13_observe("rifle", Some(1), 2, ""));
    steps.extend((0..2).map(|_| base13_buy("rifle", Some(1), None)));
    steps.push(base13_observe(
        "rifle",
        Some(1),
        0,
        "The training queue is full",
    ));
    steps.push(base13_buy("rifle", Some(1), Some("queueFull")));
    base13_control("queue-room", &["hq"], 0, &steps, 6, 9);
}

#[test]
#[ignore = "held until new exact Root Source and native grants for external BASE13 controls"]
fn classic_no_barracks_mg_card_refusal() {
    let steps = [
        base13_observe("mg", None, 0, "Needs a Barracks"),
        base13_buy("mg", None, Some("needs")),
    ];
    base13_control("no-barracks", &["hq"], 0, &steps, 1, 1);
}

#[test]
#[ignore = "held until new exact Root Source and native grants for external BASE13 controls"]
fn classic_full_selected_maker_does_not_fall_back() {
    let mut steps: Vec<_> = (0..5)
        .map(|_| base13_buy("rifle", Some(1), None))
        .collect();
    steps.extend([
        base13_observe("rifle", None, 5, ""),
        base13_observe("rifle", Some(2), 5, ""),
        base13_observe("rifle", Some(1), 0, "The training queue is full"),
        base13_buy("rifle", Some(1), Some("queueFull")),
        base13_buy("rifle", None, None),
        base13_buy("rifle", Some(2), None),
    ]);
    base13_control("selected-full", &["hq", "hq"], 0, &steps, 8, 11);
}

#[test]
#[ignore = "held until new exact Root Source and native grants for external BASE13 controls"]
fn classic_tank_fuel_denial_and_paid_debit() {
    let low = [
        base13_observe("tank", Some(1), 0, "Needs 60 fuel"),
        base13_buy("tank", Some(1), Some("fuel")),
    ];
    base13_control("tank-fuel-low", &["motorpool"], 10, &low, 1, 1);
    let paid = [
        base13_observe("tank", Some(1), 1, ""),
        base13_buy("tank", Some(1), None),
        base13_observe("tank", Some(1), 0, "Needs 60 fuel"),
        base13_buy("tank", Some(1), Some("fuel")),
    ];
    base13_control("tank-fuel-paid", &["motorpool"], 100, &paid, 2, 3);
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
