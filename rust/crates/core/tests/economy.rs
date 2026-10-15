//! Expected results are recorded by calling the reference JavaScript functions, not Rust formulas.
use serde_json::Value as Json;
use ww2_core::{
    catalog::{Catalog, Value},
    economy::{self, AbilityCost, Currency, Faction, Mode, TechState, Technology},
};

fn fixture() -> Json {
    serde_json::from_str(include_str!("fixtures/economy.json")).unwrap()
}
fn mode(row: &Json) -> Mode {
    Mode::from_kind(row["kind"].as_str())
}
fn player(row: &Json) -> Option<TechState> {
    (!row.is_null()).then(|| {
        TechState::new(
            row["tier"].as_u64().unwrap() as u8,
            row["armory"]["inf"].as_u64().unwrap() as u8,
            row["armory"]["guns"].as_u64().unwrap() as u8,
            row["armory"]["armor"].as_u64().unwrap() as u8,
        )
        .unwrap()
    })
}
fn strings(json: &Json) -> Vec<&str> {
    json.as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap())
        .collect()
}
fn tagged_number(json: &Json) -> Option<f64> {
    if json.is_null() {
        return None;
    }
    Some(match json["$number"].as_str() {
        Some("NaN") => f64::NAN,
        Some("Infinity") => f64::INFINITY,
        Some("-Infinity") => f64::NEG_INFINITY,
        Some("-0") => -0.0,
        Some(other) => panic!("unexpected number tag: {other}"),
        None => json.as_f64().unwrap(),
    })
}
fn assert_bits(actual: f64, expected: &Json, context: impl std::fmt::Debug) {
    let expected = u64::from_str_radix(expected.as_str().unwrap(), 16).unwrap();
    if f64::from_bits(expected).is_nan() {
        assert!(actual.is_nan(), "{context:?}");
    } else {
        assert_eq!(actual.to_bits(), expected, "{context:?}");
    }
}

#[test]
fn compiled_catalog_keeps_complete_post_initialization_tables() {
    let c = Catalog::compiled().unwrap();
    // The fixture fingerprint covers every static field, not only those queried by helpers.
    // It is a test fingerprint, independent of the public FFI rules hash.
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in include_bytes!("../data/rules.json") {
        hash = (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3);
    }
    assert_eq!(
        format!("{hash:016x}"),
        fixture()["catalogFnv"].as_str().unwrap()
    );
    assert!(std::ptr::eq(c, Catalog::compiled().unwrap()));
    assert_eq!(c.reference(), "e4e579a838aabae3b1d9dadbc17a0a87147bc52d");
    assert_eq!(c.table("UNITS").unwrap().object().unwrap().len(), 47);
    let types = c.table("UNIT_TYPES").unwrap().array().unwrap();
    assert_eq!(types.len(), 47);
    for unit in types {
        assert!(c.unit(unit.string().unwrap()).is_ok());
    }
    for table in [
        "CFG",
        "SUPPORT",
        "FORTS",
        "TERRAIN",
        "ENTRENCH",
        "BUILDABLE",
        "WORLD_BUILDS",
        "FIELD_BUILDS",
        "SKIRMISH_BUILDS",
        "AB_MUN",
        "TIER",
        "TECH",
        "TIER_NAMES",
        "WEATHER",
        "SHAPES",
        "SLOTS",
    ] {
        assert!(c.table(table).is_ok(), "{table}");
    }
    // These are added after the initial UNITS/SUPPORT literals in sim.js.
    let tank = c.unit("tank").unwrap();
    assert_eq!(tank.get("train").unwrap().number(), Some(35.0));
    assert_eq!(tank.get("classicCost").unwrap().number(), Some(200.0));
    assert_eq!(tank.get("fuel").unwrap().number(), Some(60.0));
    assert_eq!(
        c.support("recon").unwrap().get("mun").unwrap().number(),
        Some(25.0)
    );
    assert_eq!(
        c.table("SLOTS")
            .unwrap()
            .get("conscript")
            .unwrap()
            .array()
            .unwrap()
            .len(),
        21
    );
}

#[test]
fn special_number_tags_are_lossless_and_invalid_tags_fail() {
    let c = Catalog::from_json(r#"{"tables":{"test":[{"$number":"Infinity"},{"$number":"-Infinity"},{"$number":"NaN"},{"$number":"-0"},null]}}"#).unwrap();
    let values = c.table("test").unwrap().array().unwrap();
    assert_eq!(values[0].number(), Some(f64::INFINITY));
    assert_eq!(values[1].number(), Some(f64::NEG_INFINITY));
    assert!(values[2].number().unwrap().is_nan());
    assert_eq!(values[3].number().unwrap().to_bits(), (-0.0_f64).to_bits());
    assert_eq!(values[4], Value::Null);
    for text in [
        r#"{"tables":{"bad":{"$number":"typo"}}}"#,
        r#"{"tables":{"bad":{"$number":"NaN","extra":1}}}"#,
        "{}",
        "{",
    ] {
        assert!(Catalog::from_json(text).is_err());
    }
    let mut special_count = 0;
    fn count(v: &Value, n: &mut usize) {
        match v {
            Value::Number(x) if !x.is_finite() => *n += 1,
            Value::Array(v) => v.iter().for_each(|v| count(v, n)),
            Value::Object(v) => v.values().for_each(|v| count(v, n)),
            _ => {}
        }
    }
    count(
        Catalog::compiled().unwrap().table("CFG").unwrap(),
        &mut special_count,
    );
    assert_eq!(
        special_count, 3,
        "three house categories have an infinite minimum"
    );
}

#[test]
fn prices_factions_population_builders_and_support_match_javascript() {
    let f = fixture();
    assert_eq!(f["modes"].as_array().unwrap().len(), 10);
    for row in f["modes"].as_array().unwrap() {
        let mode = mode(row);
        assert_eq!(
            economy::build_kinds(mode).unwrap(),
            strings(&row["builds"]),
            "{mode:?}"
        );
        assert_eq!(
            economy::builder_types(mode).unwrap(),
            strings(&row["builders"]),
            "{mode:?}"
        );
        assert_eq!(row["units"].as_array().unwrap().len(), 47);
        for unit in row["units"].as_array().unwrap() {
            let name = unit[0].as_str().unwrap();
            let price = economy::price_of(mode, name).unwrap();
            assert_eq!(price.mp, unit[1]["mp"].as_f64().unwrap(), "{mode:?} {name}");
            assert_eq!(
                price.fuel,
                unit[1]["fuel"].as_f64().unwrap(),
                "{mode:?} {name}"
            );
            assert_eq!(
                economy::pop_use(name).unwrap(),
                unit[2].as_f64().unwrap(),
                "{name}"
            );
            for faction in 0..4 {
                assert_eq!(
                    economy::can_build(name, Faction::try_from(faction).unwrap()).unwrap(),
                    unit[3][faction as usize].as_bool().unwrap(),
                    "{name} {faction}"
                );
            }
            assert_eq!(
                economy::tier_of(name).unwrap(),
                unit[4].as_u64().unwrap() as u8,
                "{name}"
            );
        }
        assert_eq!(row["supports"].as_array().unwrap().len(), 8);
        for support in row["supports"].as_array().unwrap() {
            let name = support[0].as_str().unwrap();
            let cost = economy::sup_cost(mode, name).unwrap();
            assert_eq!(
                cost.currency,
                if support[1]["cur"] == "mp" {
                    Currency::Manpower
                } else {
                    Currency::Munitions
                }
            );
            assert_eq!(cost.cost, support[1]["cost"].as_f64().unwrap());
            assert_eq!(
                economy::drop_pop(name).unwrap(),
                support[2].as_f64().unwrap()
            );
        }
    }
}

#[test]
fn ability_costs_match_reference_objects_and_nullish_overrides() {
    let f = fixture();
    for row in f["abilities"].as_array().unwrap() {
        let ability = &row["ability"];
        let actual = economy::ab_cost(
            mode(row),
            AbilityCost {
                id: ability["id"].as_str().unwrap_or(""),
                munitions: tagged_number(&ability["mun"]),
            },
        )
        .unwrap();
        assert_bits(actual, &row["expected"], row);
    }
}

#[test]
fn research_prices_include_maxed_levels_and_mode_currency() {
    let f = fixture();
    for row in f["research"].as_array().unwrap() {
        let technology = match row["technology"].as_str().unwrap() {
            "tier" => Technology::Tier,
            "inf" => Technology::Infantry,
            "guns" => Technology::Guns,
            "armor" => Technology::Armor,
            _ => unreachable!(),
        };
        let actual =
            economy::tech_cost(player(&row["player"]).unwrap(), technology, mode(row)).unwrap();
        let expected = &row["expected"];
        if expected.is_null() {
            assert_eq!(actual, None, "{row}");
        } else {
            let actual = actual.unwrap();
            assert_eq!(actual.mp, expected["mp"].as_f64().unwrap(), "{row}");
            assert_eq!(actual.mun, expected["mun"].as_f64().unwrap(), "{row}");
            assert_eq!(actual.time, expected["time"].as_f64().unwrap(), "{row}");
        }
    }
}

#[test]
fn damage_multipliers_match_javascript_for_every_unit_pair_and_level() {
    // Exact selected Source literals, test-tech.js:38-44 (BASE-51).
    let attacker = Some(TechState::new(1, 2, 1, 0).unwrap());
    let defender = Some(TechState::new(1, 0, 0, 2).unwrap());
    let infantry = economy::tech_mul(attacker, Some("rifle"), defender, "rifle").unwrap();
    assert!((infantry - 1.2).abs() < 1e-9);
    let vehicle = economy::tech_mul(attacker, Some("tank"), defender, "tank").unwrap();
    assert!((vehicle - 1.1 * 0.81).abs() < 1e-9);
    assert_eq!(economy::tech_mul(None, Some("rifle"), None, "tank").unwrap(), 1.0);
    let f = fixture();
    let players: Vec<_> = f["players"]
        .as_array()
        .unwrap()
        .iter()
        .map(player)
        .collect();
    let types: Vec<_> = f["types"]
        .as_array()
        .unwrap()
        .iter()
        .map(Json::as_str)
        .collect();
    assert_eq!(f["multiplierCases"].as_u64(), Some(56_400));
    let rows = f["multipliers"].as_array().unwrap();
    assert_eq!(rows.len(), 1_200);
    for row in rows {
        let a = row[0].as_u64().unwrap() as usize;
        let by = row[1].as_u64().unwrap() as usize;
        let d = row[2].as_u64().unwrap() as usize;
        let mut hash = 0xcbf29ce484222325_u64;
        for to in types.iter().skip(1) {
            let actual = economy::tech_mul(players[a], types[by], players[d], to.unwrap()).unwrap();
            for byte in actual.to_bits().to_be_bytes() {
                hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
            }
        }
        assert_eq!(
            format!("{hash:016x}"),
            row[3].as_str().unwrap(),
            "{row}: {:?}",
            types[by]
        );
    }
    for row in f["multiplierSamples"].as_array().unwrap() {
        let a = row[0].as_u64().unwrap() as usize;
        let by = row[1].as_u64().unwrap() as usize;
        let d = row[2].as_u64().unwrap() as usize;
        let to = row[3].as_u64().unwrap() as usize;
        let actual =
            economy::tech_mul(players[a], types[by], players[d], types[to].unwrap()).unwrap();
        assert_bits(actual, &row[4], (row, types[by], types[to]));
    }
}

#[test]
fn invalid_type_and_research_inputs_are_errors() {
    for name in ["", "unknown", "__proto__", "constructor", "Tank", "tank\0"] {
        assert!(economy::price_of(Mode::Classic, name).is_err());
        assert!(economy::can_build(name, Faction::Usa).is_err());
        assert!(economy::pop_use(name).is_err());
        assert!(economy::tier_of(name).is_err());
        assert!(economy::drop_pop(name).is_err());
        assert!(economy::sup_cost(Mode::Classic, name).is_err());
        assert!(economy::tech_mul(None, Some(name), None, "tank").is_err());
        assert!(economy::tech_mul(None, None, None, name).is_err());
    }
    for state in [
        (0, 0, 0, 0),
        (4, 0, 0, 0),
        (1, 4, 0, 0),
        (1, 0, 4, 0),
        (1, 0, 0, 4),
    ] {
        assert!(TechState::new(state.0, state.1, state.2, state.3).is_err());
    }
    assert!(Faction::try_from(4).is_err());
    assert!(Faction::try_from(255).is_err());
}
