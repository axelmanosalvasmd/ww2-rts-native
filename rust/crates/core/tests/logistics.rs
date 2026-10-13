use serde_json::{Value, json};
use ww2_core::{
    catalog::Catalog,
    logistics::{
        self, Context, Definition, LogisticsState, Mode, RESERVE_RULES, Reserves, Stock, UnitFacts,
    },
};
fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/logistics.json")).unwrap()
}
fn number(v: &Value) -> f64 {
    match v["$number"].as_str() {
        Some("NaN") => f64::NAN,
        Some("Infinity") => f64::INFINITY,
        Some("-Infinity") => f64::NEG_INFINITY,
        Some("-0") => -0.0,
        _ => v.as_f64().unwrap(),
    }
}
fn num(v: f64) -> Value {
    if v.is_nan() {
        json!({"$number":"NaN"})
    } else if v == f64::INFINITY {
        json!({"$number":"Infinity"})
    } else if v == f64::NEG_INFINITY {
        json!({"$number":"-Infinity"})
    } else if v == 0.0 && v.is_sign_negative() {
        json!({"$number":"-0"})
    } else {
        json!(v)
    }
}
fn optional(v: &Value) -> Option<f64> {
    (!v.is_null()).then(|| number(v))
}
fn opt(v: Option<f64>) -> Value {
    v.map(num).unwrap_or(Value::Null)
}
fn reserves(v: &Value) -> Reserves {
    Reserves {
        ammo: number(&v["ammo"]),
        ammo_max: number(&v["ammoMax"]),
        ammo_price: number(&v["ammoPrice"]),
        ammo_credit: optional(&v["ammoCredit"]),
        provisions: number(&v["provisions"]),
        fuel: optional(&v["fuel"]),
        emergency: optional(&v["emergency"]),
        warning: optional(&v["warning"]),
        forced: v["forced"].as_bool().unwrap(),
        stranded: v["stranded"].as_bool().unwrap(),
        exhausted: v["exhausted"].as_bool().unwrap(),
        supply_rate: None,
        cut_for: None,
    }
}
fn reserve_json(r: Option<&Reserves>) -> Value {
    r.map(|r|json!({"ammo":num(r.ammo),"ammoMax":num(r.ammo_max),"ammoPrice":num(r.ammo_price),"ammoCredit":opt(r.ammo_credit),"provisions":num(r.provisions),"fuel":opt(r.fuel),"emergency":opt(r.emergency),"warning":opt(r.warning),"forced":r.forced,"stranded":r.stranded,"exhausted":r.exhausted})).unwrap_or(Value::Null)
}
fn stock(v: &Value) -> Stock {
    Stock {
        ammo: number(&v["ammo"]),
        provisions: number(&v["provisions"]),
        fuel: number(&v["fuel"]),
    }
}
fn stock_json(s: Stock) -> Value {
    json!({"ammo":num(s.ammo),"provisions":num(s.provisions),"fuel":num(s.fuel)})
}
fn context(v: &Value) -> Context {
    Context {
        enabled_override: v["override"].as_bool(),
        scenario: v["scenario"].as_bool().unwrap_or(false),
        mode: Mode::from_reference(v["mode"].as_str()),
    }
}
fn facts<'a>(
    id: u32,
    kind: &'a str,
    owner: Option<usize>,
    owner_exists: bool,
    guard_home: bool,
    riding: bool,
) -> UnitFacts<'a> {
    UnitFacts {
        id,
        kind,
        owner,
        owner_exists,
        guard_home,
        riding,
    }
}
fn compare(a: &Value, b: &Value, path: &str) {
    if b.is_number() || b.get("$number").is_some() {
        let a = number(a);
        let b = number(b);
        if b.is_nan() {
            assert!(a.is_nan(), "{path}");
        } else {
            assert_eq!(a.to_bits(), b.to_bits(), "{path}");
        }
        return;
    }
    match b {
        Value::Object(b) => {
            let a = a.as_object().unwrap();
            assert_eq!(a.len(), b.len(), "{path}");
            for (k, v) in b {
                compare(&a[k], v, &format!("{path}.{k}"));
            }
        }
        Value::Array(b) => {
            let a = a.as_array().unwrap();
            assert_eq!(a.len(), b.len(), "{path}");
            for (i, v) in b.iter().enumerate() {
                compare(&a[i], v, &format!("{path}[{i}]"));
            }
        }
        _ => assert_eq!(a, b, "{path}"),
    }
}
#[test]
fn modes_initialization_and_weapon_prices_match_javascript() {
    let f = fixture();
    let c = Catalog::compiled().unwrap();
    for row in f["contexts"].as_array().unwrap() {
        assert_eq!(
            logistics::enabled_for(context(row)),
            row["expected"].as_bool().unwrap()
        );
    }
    let definitions = f["definitions"].as_array().unwrap();
    let custom: Vec<_> = definitions
        .iter()
        .map(|d| Catalog::from_json(&json!({"tables":{"def":d["def"]}}).to_string()).unwrap())
        .collect();
    for (i, d) in definitions.iter().enumerate() {
        let value = if let Some(kind) = d["kind"].as_str() {
            Some(c.unit(kind).unwrap())
        } else if d["def"].is_null() {
            None
        } else {
            Some(custom[i].table("def").unwrap())
        };
        let def = value.map(Definition::from_value);
        compare(
            &num(logistics::ammo_price(def.as_ref())),
            &d["price"],
            "price",
        );
    }
    for row in f["initial"].as_array().unwrap() {
        let i = row["definition"].as_u64().unwrap() as usize;
        let d = &definitions[i];
        let value = if let Some(kind) = d["kind"].as_str() {
            Some(c.unit(kind).unwrap())
        } else if d["def"].is_null() {
            None
        } else {
            Some(custom[i].table("def").unwrap())
        };
        let def = value.map(Definition::from_value);
        let owner = row["facts"]["owner"].as_u64().map(|n| n as usize);
        let unit = facts(
            1,
            row["facts"]["type"]
                .as_str()
                .unwrap_or(d["kind"].as_str().unwrap_or("synthetic")),
            owner,
            row["facts"]["exists"].as_bool().unwrap(),
            row["facts"]["guard"].as_bool().unwrap(),
            false,
        );
        let mut state = LogisticsState::default();
        let result = state.initialize_unit(
            Context {
                enabled_override: Some(true),
                scenario: false,
                mode: Mode::Conquest,
            },
            unit,
            def.as_ref(),
        );
        compare(&reserve_json(result), &row["expected"], &format!("init{i}"));
    }
}
#[test]
fn reserve_transitions_and_transfers_match_original_javascript() {
    let f = fixture();
    for (i, trace) in f["traces"].as_array().unwrap().iter().enumerate() {
        let kind = trace["kind"].as_str().unwrap();
        let def = Definition::from_catalog(kind).ok();
        let mut state = LogisticsState::default();
        if !trace["initial"].is_null() {
            state.insert(17, reserves(&trace["initial"]));
        }
        let mut cargo = (!trace["store"].is_null()).then(|| stock(&trace["store"]));
        let mut enabled = true;
        for (j, step) in trace["steps"].as_array().unwrap().iter().enumerate() {
            let unit = facts(
                17,
                kind,
                Some(0),
                true,
                false,
                step["riding"].as_bool().unwrap_or(false),
            );
            let mut result = Value::Null;
            match step["op"].as_str().unwrap() {
                "consume" => result = num(state.consume_ammo(17, number(&step["count"]))),
                "tick" => {
                    enabled = step["enabled"].as_bool().unwrap_or(true);
                    state.tick_reserves(
                        Context {
                            enabled_override: Some(enabled),
                            scenario: false,
                            mode: Mode::Conquest,
                        },
                        unit,
                        number(&step["dt"]),
                        step.get("travel").map(number).unwrap_or(0.0),
                    );
                }
                "drive" => state.consume_driving_fuel(unit, number(&step["seconds"])),
                "transfer" => {
                    result = stock_json(state.transfer_stock(
                        17,
                        if step["absent"].as_bool().unwrap_or(false) {
                            None
                        } else {
                            cargo.as_mut()
                        },
                        number(&step["seconds"]),
                    ))
                }
                "initialize" => {
                    result = json!(
                        state
                            .initialize_unit(
                                Context {
                                    enabled_override: Some(enabled),
                                    scenario: false,
                                    mode: Mode::Conquest
                                },
                                unit,
                                def.as_ref()
                            )
                            .is_some()
                    )
                }
                _ => panic!("invalid fixture operation"),
            }
            let path = format!("trace{i}.step{j}");
            compare(&result, &step["result"], &format!("{path}.result"));
            compare(
                &reserve_json(state.get(17)),
                &step["reserves"],
                &format!("{path}.reserves"),
            );
            compare(
                &cargo.map(stock_json).unwrap_or(Value::Null),
                &step["stock"],
                &format!("{path}.stock"),
            );
            let s = state.shortage_state(17);
            let query = json!({"canFire":([0.0,0.99,1.0,1.5,10.0,1e6,-1.0,f64::NAN,f64::INFINITY].map(|n|state.can_fire(17,n))),"recovered":state.recovered(17),"shortage":{"lowAmmo":s.low_ammo,"lowProvisions":s.low_provisions,"exhausted":s.exhausted,"forced":s.forced,"stranded":s.stranded},"shortageRate":num(state.shortage_rate(17)),"recoveryRate":num(state.recovery_rate(17))});
            compare(&query, &step["query"], &format!("{path}.query"));
        }
    }
}
#[test]
fn reserve_constants_and_stable_identity_are_explicit() {
    let f = fixture();
    let c = &f["constants"];
    for (name, value) in [
        ("ammoSeconds", RESERVE_RULES.ammo_seconds),
        ("provisions", RESERVE_RULES.provisions),
        ("fuel", RESERVE_RULES.fuel),
        ("emergency", RESERVE_RULES.emergency),
        ("low", RESERVE_RULES.low),
        ("recovery", RESERVE_RULES.recovery),
        ("warning", RESERVE_RULES.warning),
        ("unloadSeconds", RESERVE_RULES.unload_seconds),
        ("smallAmmoPrice", RESERVE_RULES.small_ammo_price),
        ("shellAmmoPrice", RESERVE_RULES.shell_ammo_price),
        ("heavyAmmoPrice", RESERVE_RULES.heavy_ammo_price),
    ] {
        compare(&num(value), &c[name], name);
    }
    let d = Definition::from_catalog("rifle").unwrap();
    let mut s = LogisticsState::default();
    let ctx = Context {
        enabled_override: Some(true),
        scenario: false,
        mode: Mode::Conquest,
    };
    for id in [9, 2, 5] {
        s.initialize_unit(
            ctx,
            facts(id, "rifle", Some(0), true, false, false),
            Some(&d),
        );
    }
    s.consume_ammo(5, 3.0);
    let before = s.clone();
    s.initialize_unit(
        ctx,
        facts(5, "rifle", Some(0), true, false, false),
        Some(&d),
    );
    assert_eq!(before, s);
    assert_eq!(s.iter().map(|(id, _)| id).collect::<Vec<_>>(), [2, 5, 9]);
    assert!(s.remove(5).is_some());
    assert!(Definition::from_catalog("unknown").is_err());
}
