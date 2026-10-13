use serde_json::{Value, json};
use std::collections::BTreeMap;
use ww2_core::{
    game::LOCAL_DEFENDER_OWNER, match_runtime::Match, match_world_phases::WorldIncomeUnit,
};
#[path = "common/world_scene.rs"]
mod world_scene;

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/world-phases.json")).unwrap()
}
fn ids(values: &Value) -> Vec<u32> {
    values
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_u64().unwrap() as u32)
        .collect()
}
fn load(f: &Value, case: &Value) -> Match {
    let mut m = world_scene::make_match(f, case["input"]["teams"].as_array().unwrap());
    world_scene::reset(&mut m, f, &case["input"]);
    m
}
fn view(m: &Match, order: &[u32]) -> Value {
    let mut v = world_scene::projection(m);
    v["units"] = json!(
        order
            .iter()
            .filter_map(|id| m.world.game.units.get(id))
            .map(|u| {
                [
                    i64::from(u.id),
                    if u.owner == LOCAL_DEFENDER_OWNER {
                        -1
                    } else {
                        u.owner as i64
                    },
                ]
            })
            .collect::<Vec<_>>()
    );
    v
}
// Controlled restored-scene actions match the recorder, not a claimed World
// death/convoy runner. Retained facts come from the actual object at removal.
fn actions(m: &mut Match, case: &Value, phase: &str) -> BTreeMap<u32, WorldIncomeUnit> {
    let mut retired = BTreeMap::new();
    for a in case["actions"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|a| a["phase"] == phase)
    {
        match a["op"].as_str().unwrap() {
            "health" => {
                m.world
                    .game
                    .units
                    .get_mut(&(a["id"].as_u64().unwrap() as u32))
                    .unwrap()
                    .hp = a["hp"].as_f64().unwrap();
            }
            "remove" => {
                let id = a["id"].as_u64().unwrap() as u32;
                let unit = m.world.game.units.remove(&id).unwrap();
                retired.insert(id, WorldIncomeUnit::from(&unit));
                m.grid.remove_unit(id);
            }
            "birth" => world_scene::update_unit(m, &a["born"], true),
            op => panic!("unknown phase scene action {op}"),
        }
    }
    retired
}
fn final_captured(
    m: &Match,
    order: &[u32],
    retired: &BTreeMap<u32, WorldIncomeUnit>,
) -> Vec<WorldIncomeUnit> {
    order
        .iter()
        .map(|id| {
            m.world
                .game
                .units
                .get(id)
                .map(WorldIncomeUnit::from)
                .unwrap_or_else(|| *retired.get(id).expect("explicit final retired object"))
        })
        .collect()
}
#[test]
fn original_split_phases_preserve_final_captured_facts_and_current_membership() {
    let f = fixture();
    let mut count = 0;
    for case in f["cases"].as_array().unwrap() {
        let mut m = load(&f, case);
        let captured_order = ids(&case["capturedOrder"]);
        let retired = actions(&mut m, case, "beforeIncome");
        let captured = final_captured(&m, &captured_order, &retired);
        let incoming = captured
            .iter()
            .map(|u| {
                json!({"id":u.id,"owner":if u.owner==LOCAL_DEFENDER_OWNER{-1}else{u.owner as i64},"type":m.rules.kinds[u.kind].name,"hp":format!("{:016x}",u.hp.to_bits())})
            })
            .collect::<Vec<_>>();
        assert_eq!(
            json!(incoming),
            case["capturedFacts"],
            "{} facts",
            case["name"]
        );
        let income_order = ids(&case["incomeOrder"]);
        assert_eq!(
            view(&m, &income_order),
            case["before"],
            "{} input",
            case["name"]
        );
        let dt = case["dt"].as_f64().unwrap();
        m.apply_world_income(&captured, dt).unwrap();
        assert_eq!(
            view(&m, &income_order),
            case["afterIncome"],
            "{} income",
            case["name"]
        );
        actions(&mut m, case, "between");
        let current = ids(&case["regionOrder"]);
        assert_eq!(
            view(&m, &current),
            case["beforeRegions"],
            "{} current actors",
            case["name"]
        );
        let wallets = m
            .start
            .economy
            .players
            .iter()
            .map(|p| (p.mp, p.mun, p.fuel, p.inc, p.fuel_inc, p.upkeep))
            .collect::<Vec<_>>();
        let report = m.apply_world_regions(&current, dt).unwrap();
        assert_eq!(
            json!(report.removed.iter().map(|u| u.id).collect::<Vec<_>>()),
            case["removed"],
            "{} actual removals",
            case["name"]
        );
        assert_eq!(
            view(&m, &current),
            case["afterRegions"],
            "{} regions",
            case["name"]
        );
        assert_eq!(
            m.start
                .economy
                .players
                .iter()
                .map(|p| (p.mp, p.mun, p.fuel, p.inc, p.fuel_inc, p.upkeep))
                .collect::<Vec<_>>(),
            wallets,
            "region phase must not charge income again"
        );
        count += captured.len();
        // When no effects intervene and every captured object remains current,
        // the old atomic wrapper still matches the same original final result.
        if case["actions"].as_array().unwrap().is_empty() {
            let mut combined = load(&f, case);
            combined
                .apply_world_capture_income_and_elimination(&captured_order, &current, dt)
                .unwrap();
            assert_eq!(view(&combined, &current), case["afterRegions"]);
        }
    }
    assert_eq!(count, 20);
}

#[test]
fn malformed_income_facts_and_current_orders_leave_their_phase_unchanged() {
    let f = fixture();
    let case = &f["cases"][4];
    let mut m = load(&f, case);
    let order = ids(&case["capturedOrder"]);
    let facts = final_captured(&m, &order, &BTreeMap::new());
    let before = view(&m, &order);
    for change in 0..6 {
        let mut malformed = facts.clone();
        match change {
            0 => malformed.push(malformed[0]),
            1 => malformed[0].id = 0,
            2 => malformed[0].owner = m.world.game.players.len(),
            3 => malformed[0].kind = m.rules.kinds.len(),
            4 => malformed[0].hp = f64::NAN,
            _ => malformed[0].hp += 1.0,
        }
        assert!(m.apply_world_income(&malformed, 0.05).is_err());
        assert_eq!(view(&m, &order), before);
    }
    for dt in [f64::NAN, f64::INFINITY, -0.05] {
        assert!(m.apply_world_income(&facts, dt).is_err());
        assert_eq!(view(&m, &order), before);
    }
    let mut incomplete = order.clone();
    incomplete.pop();
    assert!(m.apply_world_regions(&incomplete, 0.05).is_err());
    assert_eq!(view(&m, &order), before);
    let actual = m.world.game.units.get_mut(&order[0]).unwrap();
    let hp = actual.hp;
    actual.hp = f64::NAN;
    assert!(m.apply_world_income(&[], 0.05).is_err());
    m.world.game.units.get_mut(&order[0]).unwrap().hp = hp;
    assert_eq!(view(&m, &order), before);
    m.options.logistics = true;
    assert!(m.apply_world_income(&facts, 0.05).is_err());
    assert!(m.apply_world_regions(&order, 0.05).is_err());
    m.options.logistics = false;
    assert_eq!(view(&m, &order), before);
    m.start.economy.players[0].mp = f64::MAX;
    let wallets = m.start.economy.clone();
    assert!(m.apply_world_income(&facts, f64::MAX).is_err());
    assert_eq!(m.start.economy, wallets);
}

#[test]
fn combined_refusal_is_atomic_but_separate_income_is_an_explicit_committed_phase() {
    let f = fixture();
    let case = &f["cases"][2];
    let mut split = load(&f, case);
    let order = ids(&case["capturedOrder"]);
    let facts = final_captured(&split, &order, &BTreeMap::new());
    split.apply_world_income(&facts, 0.05).unwrap();
    actions(&mut split, case, "between");
    let fort = json!({"id":999,"owner":1,"type":"pillbox","x":193.,"z":65.,"hp":500.,"built":1.,"retreating":false,"riding":0});
    world_scene::update_unit(&mut split, &fort, true);
    let mut current = ids(&case["regionOrder"]);
    current.push(999);
    let after_income = view(&split, &current);
    assert!(
        split
            .apply_world_regions(&current, 0.05)
            .unwrap_err()
            .contains("fort")
    );
    assert_eq!(view(&split, &current), after_income);
    let mut combined = load(&f, case);
    actions(&mut combined, case, "between");
    world_scene::update_unit(&mut combined, &fort, true);
    let before = view(&combined, &current);
    assert!(
        combined
            .apply_world_capture_income_and_elimination(&order, &current, 0.05)
            .unwrap_err()
            .contains("fort")
    );
    assert_eq!(view(&combined, &current), before);
    assert_ne!(combined.start.economy, split.start.economy);
}
