use serde_json::{Value, json};
use ww2_core::{
    economy::{Faction, Mode, TechState},
    economy_state::{ArmySize, EconomyState, ModeContext, PendingDrop, PlayerEconomy},
    production::{Context, Denial, Error, ProductionState, UnitView},
};
fn bits(v: f64) -> String {
    format!("{:016x}", v.to_bits())
}
fn money(w: &EconomyState) -> Value {
    let p = &w.players[0];
    json!([bits(p.mp), bits(p.mun), bits(p.fuel)])
}
fn wallets(mp: f64) -> EconomyState {
    EconomyState {
        players: vec![
            PlayerEconomy {
                mp,
                team: 0,
                ..Default::default()
            },
            PlayerEconomy {
                team: 0,
                ..Default::default()
            },
            PlayerEconomy {
                team: 1,
                ..Default::default()
            },
        ],
    }
}
fn context<'a>(units: &'a [UnitView<'a>]) -> Context<'a> {
    Context {
        mode: ModeContext::default(),
        army: ArmySize::Standard,
        tech_enabled: false,
        naval: false,
        sudden_death: false,
        units,
        drops: &[],
        world: None,
    }
}
#[test]
fn original_buy_commands() {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/recruitment.json")).unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let mut input = fixture["base"].clone();
        for (k, v) in case["options"].as_object().unwrap() {
            input[k] = v.clone();
        }
        let mut state = ProductionState::new(vec![
            TechState::new(
                input["tier"].as_u64().unwrap() as u8,
                0,
                0,
                0
            )
            .unwrap();
            3
        ])
        .unwrap();
        let units: Vec<_> = input["units"]
            .as_array()
            .unwrap()
            .iter()
            .map(|u| {
                let id = u["id"].as_u64().unwrap() as u32;
                let queue: Vec<_> = u["queue"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|t| t.as_str().unwrap())
                    .collect();
                state.reconcile_queue(id, &queue).unwrap();
                UnitView {
                    id,
                    owner: u["owner"].as_u64().map(|s| s as usize),
                    kind: u["type"].as_str().unwrap(),
                    hp: u["hp"].as_f64().unwrap(),
                    built: u["built"].as_f64().unwrap(),
                }
            })
            .collect();
        let drops: Vec<_> = input["strikes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| PendingDrop {
                owner: d["owner"].as_u64().unwrap() as usize,
                kind: d["kind"].as_str().unwrap(),
                live: d["live"].as_bool().unwrap(),
            })
            .collect();
        let mut c = context(&units);
        c.mode.kind = Mode::from_kind(input["mode"]["kind"].as_str());
        c.tech_enabled = input["tech"].as_bool().unwrap();
        c.naval = input["naval"].as_bool().unwrap();
        c.drops = &drops;
        c.army = match input["army"].as_str().unwrap() {
            "standard" => ArmySize::Standard,
            "large" => ArmySize::Large,
            "massive" => ArmySize::Massive,
            "endless" => ArmySize::Endless,
            _ => panic!(),
        };
        let mut w = wallets(input["mp"].as_f64().unwrap());
        w.players[0].mun = input["mun"].as_f64().unwrap();
        w.players[0].fuel = input["fuel"].as_f64().unwrap();
        let original_state = state.clone();
        let outcome = state.prepare_recruit(
            &c,
            &mut w,
            0,
            Faction::try_from(case["faction"].as_u64().unwrap() as u8).unwrap(),
            case["kind"].as_str().unwrap(),
            case["from"].as_u64().map(|id| id as u32),
        );
        let (result, completed) = match outcome {
            Ok(pending) => {
                let r = pending.commit().request;
                (
                    Value::Null,
                    json!([{ "building":r.building,"owner":r.owner,"kind":r.kind }]),
                )
            }
            Err(Error::Denied(reason)) => {
                let s = match reason {
                    Denial::Blocked => "blocked",
                    Denial::Needs => "needs",
                    Denial::Tier => "tier",
                    Denial::Mp => "mp",
                    Denial::Fuel => "fuel",
                    Denial::Pop => "pop",
                    Denial::Max => "max",
                    other => panic!("Unexpected {other:?}"),
                };
                (json!(s), json!([]))
            }
            Err(e) => panic!("{}: {e}", case["name"]),
        };
        assert_eq!(
            json!({"result":result,"money":money(&w),"completed":completed}),
            case["expected"],
            "{}",
            case["name"]
        );
        assert_eq!(state, original_state, "instant purchases never create jobs");
    }
}
#[test]
fn aborted_placement_and_invalid_inputs_never_charge() {
    let state = ProductionState::new(vec![TechState::new(1, 0, 0, 0).unwrap(); 3]).unwrap();
    let units = [UnitView {
        id: 1,
        owner: Some(0),
        kind: "hq",
        hp: 100.0,
        built: 1.0,
    }];
    let c = context(&units);
    let mut w = wallets(500.0);
    let original = w.clone();
    {
        let pending = state
            .prepare_recruit(&c, &mut w, 0, Faction::Usa, "rifle", None)
            .unwrap();
        assert_eq!(pending.request().building, 1);
        assert_eq!(pending.request().spend.fuel, 0.0);
        // Failed staged placement drops the reservation before wallet publication.
    }
    assert_eq!(w, original);
    let mut bad = c;
    bad.mode.kind = Mode::Horde;
    assert!(matches!(
        state.prepare_recruit(&bad, &mut w, 0, Faction::Usa, "rifle", None),
        Err(Error::InvalidInput(_))
    ));
    let duplicate = [units[0], units[0]];
    bad = context(&duplicate);
    assert!(matches!(
        state.prepare_recruit(&bad, &mut w, 0, Faction::Usa, "rifle", None),
        Err(Error::InvalidInput(_))
    ));
    assert!(
        state
            .prepare_recruit(&c, &mut w, 3, Faction::Usa, "rifle", None)
            .is_err()
    );
    assert_eq!(w, original);
    let completed = state
        .prepare_recruit(&c, &mut w, 0, Faction::Usa, "rifle", None)
        .unwrap()
        .commit();
    assert_eq!(
        w.players[0].mp,
        original.players[0].mp - completed.request.spend.mp
    );
    assert_eq!(completed.request.building, 1);
}
