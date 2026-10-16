//! Original World helper transitions on a real native World Match.
use crate::shared::world_scene;
use serde_json::{Value, json};
use world_scene::{make_match, projection, reset, sorted_units, update_region, update_unit};
use ww2_core::{economy::Mode, economy_state::ArmySize};

#[test]
fn original_world_capture_income_transitions_and_explicit_consequences() {
    let f: Value = serde_json::from_str(include_str!("fixtures/world-region.json")).unwrap();
    assert_eq!(f["reference"], "e4e579a838aabae3b1d9dadbc17a0a87147bc52d");
    let mut transitions = 0;
    for count in [2, 3] {
        let teams: Vec<_> = if count == 2 {
            vec![json!(0), json!(1)]
        } else {
            vec![json!(0), json!(0), json!(1)]
        };
        let mut m = make_match(&f, &teams);
        for case in f["cases"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| c["input"]["teams"].as_array().unwrap().len() == count)
        {
            reset(&mut m, &f, &case["input"]);
            for step in case["steps"].as_array().unwrap() {
                for id in step["deleteUnits"].as_array().into_iter().flatten() {
                    let id = id.as_u64().unwrap() as u32;
                    m.world.game.units.remove(&id);
                    m.grid.remove_unit(id);
                }
                for raw in step["changeUnits"].as_array().into_iter().flatten() {
                    update_unit(&mut m, raw, false);
                }
                for raw in step["addUnits"].as_array().into_iter().flatten() {
                    update_unit(&mut m, raw, true);
                }
                for raw in step["changeRegions"].as_array().into_iter().flatten() {
                    update_region(
                        &mut m.world_runtime.as_mut().unwrap().regions
                            [raw["index"].as_u64().unwrap() as usize],
                        raw,
                    );
                }
                for raw in step["changePlayers"].as_array().into_iter().flatten() {
                    let p = &mut m.start.economy.players[raw["slot"].as_u64().unwrap() as usize];
                    if let Some(v) = raw["out"].as_bool() {
                        p.out = v;
                    }
                }
                m.world.game.tick = step["tick"].as_i64().unwrap_or(m.world.game.tick + 1);
                let before = projection(&m);
                assert_eq!(
                    before,
                    sorted_units(step["before"].clone()),
                    "{} before",
                    case["name"]
                );
                let order: Vec<_> = step["order"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_u64().unwrap() as u32)
                    .collect();
                let result = m.apply_world_capture_income(&order, step["dt"].as_f64().unwrap());
                // The stored support marker predates the native fort-owner actuator.
                if case["supported"].as_bool().unwrap()
                    || case["name"] == "unsupported fort transfer"
                {
                    result.unwrap();
                    assert_eq!(
                        projection(&m),
                        sorted_units(step["after"].clone()),
                        "{} after",
                        case["name"]
                    );
                } else {
                    assert!(result.unwrap_err().starts_with("unsupported World"));
                    assert_eq!(projection(&m), before);
                    assert_ne!(
                        step["before"], step["after"],
                        "unsupported branch must execute in source"
                    );
                }
                transitions += 1;
            }
        }
    }
    assert_eq!(transitions, 39);
}

#[test]
fn malformed_world_phase_is_atomic() {
    let f: Value = serde_json::from_str(include_str!("fixtures/world-region.json")).unwrap();
    let mut m = make_match(&f, &[json!(0), json!(1)]);
    let input = &f["cases"][0]["input"];
    reset(&mut m, &f, input);
    let before = projection(&m);
    assert!(m.apply_world_capture_income(&[1], f64::NAN).is_err());
    assert_eq!(projection(&m), before);
    assert!(m.apply_world_capture_income(&[1, 1], 0.05).is_err());
    assert_eq!(projection(&m), before);
    assert!(m.apply_world_capture_income(&[999], 0.05).is_err());
    assert_eq!(projection(&m), before);
    m.world_runtime.as_mut().unwrap().regions[2].cap_tick = Some(i64::MIN);
    let before = projection(&m);
    assert!(m.apply_world_capture_income(&[1], 0.05).is_err());
    assert_eq!(projection(&m), before);
}

#[test]
fn current_depot_facts_do_not_reuse_retired_tick_rows() {
    use ww2_core::economy_state::{Context, DepotFacts, ModeContext, Node, Unit, WorldContext};
    let f: Value = serde_json::from_str(include_str!("fixtures/world-region.json")).unwrap();
    let case = f["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "explicit retained depot facts")
        .unwrap();
    let mut m = make_match(&f, &[json!(0), json!(1)]);
    reset(&mut m, &f, &case["input"]);
    let regions = m.world_economy_regions().unwrap();
    let rows = [Unit {
        id: 20,
        owner: Some(0),
        kind: "depot",
        hp: 1.0,
        built: 1.0,
        queue: &[],
    }];
    let nodes = [Node {
        depot: Some(20),
        rate: 3.0,
        fuel: false,
        region: Some(0),
    }];
    let ctx = Context {
        mode: ModeContext {
            kind: Mode::World,
            defender_team: None,
            income_excluded_slot: None,
        },
        army: ArmySize::Standard,
        units: &rows,
        drops: &[],
        points: &[],
        nodes: &nodes,
        world: Some(WorldContext { regions: &regions }),
    };
    let before = m.start.economy.clone();
    let fact = DepotFacts {
        id: 20,
        owner: Some(0),
        hp: 1.0,
        built: 1.0,
    };
    assert!(
        m.start
            .economy
            .advance_income_with_depots(&ctx, &[fact, fact], 0.05, false)
            .is_err()
    );
    assert_eq!(m.start.economy, before);
    let bad = DepotFacts {
        built: f64::NAN,
        ..fact
    };
    assert!(
        m.start
            .economy
            .advance_income_with_depots(&ctx, &[bad], 0.05, false)
            .is_err()
    );
    assert_eq!(m.start.economy, before);
    m.start
        .economy
        .advance_income_with_depots(&ctx, &[], 0.05, false)
        .unwrap();
    assert_eq!(
        projection(&m)["players"],
        case["steps"][0]["after"]["players"]
    );
}
