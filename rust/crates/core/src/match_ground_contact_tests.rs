use super::*;
use crate::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    map::AuthoritativeMap,
    match_runtime::{MatchOptions, PlayerConfig},
    weather::WeatherChoice,
};
use serde_json::Value;

fn number(v: &Value) -> f64 {
    v["value"].as_f64().unwrap()
}
fn bits(v: &Value) -> u64 {
    u64::from_str_radix(v["bits"].as_str().unwrap(), 16).unwrap()
}
// JSON has one numeric domain; compare its IEEE value, retaining type and shape.
pub(super) fn exact_json_numbers(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => {
            a.as_f64().unwrap().to_bits() == b.as_f64().unwrap().to_bits()
        }
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| exact_json_numbers(a, b))
        }
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len()
                && a.iter()
                    .all(|(key, a)| b.get(key).is_some_and(|b| exact_json_numbers(a, b)))
        }
        _ => a == b,
    }
}
pub(super) fn fixture() -> Value {
    serde_json::from_str(include_str!("../tests/fixtures/ground-contact.json")).unwrap()
}
pub(super) fn scene(case: &Value) -> (Match, u32, usize) {
    let map =
        AuthoritativeMap::from_json(&serde_json::to_vec(&case["setup"]["map"]).unwrap()).unwrap();
    let mut m = Match::new(
        &map,
        MatchOptions {
            seed: 701,
            mode: Mode::Conquest,
            players: (0..2)
                .map(|slot| PlayerConfig {
                    name: format!("p{slot}"),
                    team: slot,
                    faction: Faction::try_from(slot as u8).unwrap(),
                })
                .collect(),
            shuffle: false,
            army: ArmySize::Standard,
            weather: WeatherChoice::Map,
            weather_seed: Some(1),
            living_weather: false,
            map_key: None,
            tech: false,
            supply: false,
            logistics: false,
            sandbox: false,
        },
    )
    .unwrap();
    let id = m.next_unit_id;
    let staged = crate::unit_publication::StagedUnit::ordinary(
        &m.world.game,
        &m.rules,
        0,
        m.rules.kind("tank").unwrap(),
        id,
        m.world.game.units.len() as u32,
        m.start.players[0].spawn,
        Some(crate::game::Point { x: 51.0, z: 51.0 }),
    )
    .unwrap();
    crate::unit_publication::prepare_publication(
        crate::unit_publication::PublicationParts {
            units: &mut m.world.game.units,
            grid: &mut m.grid,
            controls: &mut m.commands.units,
            recruitment: &mut m.recruit_metadata,
            next_id: &mut m.next_unit_id,
        },
        &m.rules,
        2,
        vec![staged],
    )
    .unwrap()
    .publish_with(|u, _| {
        m.combat.initialize(u.id);
        m.firing.initialize(u.id);
        m.abilities.initialize(u.id);
        m.recovery.initialize(u.id);
    });
    assert_eq!(id, case["setup"]["spawn"]["id"].as_u64().unwrap() as u32);
    let c = case["setup"]["wearPrime"]["c"].as_u64().unwrap() as usize;
    let initial = &case["initial"]["cells"][0];
    m.world.game.wear.as_mut().unwrap()[c] = number(&initial["wear"]) as f32;
    m.world.cell_state[c] = initial["state"].as_u64().unwrap() as u8;
    m.world.surface_wear[c] = number(&initial["surfaceWear"]) as f32;
    m.world.take_journal();
    m.world.take_new_cells();
    (m, id, c)
}

#[test]
fn vehicle_wear_matches_actual_source_full_step_contacts() {
    let fixture = fixture();
    assert_eq!(fixture["cases"].as_array().unwrap().len(), 6);
    for case in fixture["cases"].as_array().unwrap() {
        let (mut m, id, c) = scene(case);
        let mut prior_dust = None;
        for expected in case["steps"].as_array().unwrap() {
            m.world.game.tick = expected["tick"].as_i64().unwrap();
            m.world.game.wet = expected["wx"].get("wet").and_then(Value::as_f64);
            let movement = &expected["movement"];
            let before = Pose {
                x: number(&movement["before"]["x"]),
                z: number(&movement["before"]["z"]),
                rot: 0.0,
            };
            let unit = m.world.game.units.get_mut(&id).unwrap();
            unit.x = number(&movement["after"]["x"]);
            unit.z = number(&movement["after"]["z"]);
            let terrain = m.world.game.terrain_version;
            let versions = m.world.versions;
            m.apply_moved_ground_contact(id, before, prior_dust)
                .unwrap();
            let row = &expected["cells"][0];
            assert_eq!(
                m.world.game.wear.as_ref().unwrap()[c].to_bits(),
                row["wearF32Bits"].as_u64().unwrap() as u32,
                "{} wear",
                case["name"]
            );
            assert_eq!(
                m.world.surface_wear[c].to_bits(),
                row["surfaceWearF32Bits"].as_u64().unwrap() as u32
            );
            assert_eq!(m.world.cell_state[c], row["state"].as_u64().unwrap() as u8);
            assert_eq!(char::from(m.world.game.chars[c]).to_string(), row["ch"]);
            assert_eq!(m.world.game.flags[c], row["flags"].as_u64().unwrap() as u16);
            assert_eq!(
                serde_json::json!(m.world.journal()),
                expected["journalDelta"]
            );
            let emitted = m.world.take_new_cells();
            assert_eq!(
                emitted.len(),
                expected["newCellsDelta"].as_array().unwrap().len()
            );
            for (actual, source) in emitted
                .iter()
                .zip(expected["newCellsDelta"].as_array().unwrap())
            {
                let data = &actual.data;
                assert!(
                    !data.mine
                        && data.section.is_none()
                        && data.rail.is_none()
                        && data.station.is_none()
                        && data.trench_front.is_none()
                );
                assert!(
                    exact_json_numbers(
                        &serde_json::json!([actual.cell, char::from(actual.ch).to_string(),
                    actual.height, actual.state, {"ground":char::from(data.ground).to_string(),
                    "object":char::from(data.object).to_string(),"material":data.material,
                    "groundMaterial":data.ground_material,"groundWear":f64::from(data.ground_wear)}]),
                        source
                    ),
                    "{}: complete numeric-semantic cell row",
                    case["name"]
                );
                assert_eq!(actual.cell, source[0].as_u64().unwrap() as usize);
                assert_eq!(char::from(actual.ch).to_string(), source[1]);
                assert_eq!(actual.state, source[3].as_u64().unwrap() as u8);
                assert_eq!(
                    f64::from(actual.data.ground_wear).to_bits(),
                    source[4]["groundWear"].as_f64().unwrap().to_bits()
                );
            }
            let mud = case["name"] == "dry-open-mud-transition" && expected["tick"] == 1;
            assert_eq!(
                m.world.game.terrain_version - terrain,
                if mud { 2 } else { 0 }
            );
            assert_eq!(m.world.versions, versions);
            let dust = expected["unit"]["dust"].as_f64();
            assert_eq!(m.world.game.units[&id].dust, dust);
            prior_dust = dust;
            m.world.take_journal();
            assert_eq!(
                f64::from(m.world.game.wear.as_ref().unwrap()[c]).to_bits(),
                bits(&row["wear"])
            );
        }
    }
}

#[test]
fn unavailable_contact_and_capacity_denials_preserve_world() {
    let fixture = fixture();
    for boundary in ["crush", "mine", "capacity", "shape", "kind"] {
        let (mut m, id, c) = scene(&fixture["cases"][0]);
        let expected = &fixture["cases"][0]["steps"][0];
        let movement = &expected["movement"];
        let before = Pose {
            x: number(&movement["before"]["x"]),
            z: number(&movement["before"]["z"]),
            rot: 0.0,
        };
        let unit = m.world.game.units.get_mut(&id).unwrap();
        unit.x = number(&movement["after"]["x"]);
        unit.z = number(&movement["after"]["z"]);
        match boundary {
            "crush" => {
                m.world.set_cell(c, b'H').unwrap();
            }
            "mine" => {
                m.world.set_cell(c, b'N').unwrap();
                m.world.mines.insert(c, 1);
            }
            "capacity" => {
                m.world.game.wear.as_mut().unwrap()[c] = 0.9999;
                m.world.game.terrain_version = u32::MAX;
            }
            "shape" => {
                m.world.game.flags.pop();
            }
            "kind" => {
                m.world.game.units.get_mut(&id).unwrap().kind = usize::MAX;
            }
            _ => unreachable!(),
        }
        let wear = m.world.game.wear.clone();
        let chars = m.world.game.chars.clone();
        let journal = m.world.journal().to_vec();
        let new_cells = m.world.new_cells().to_vec();
        let versions = m.world.versions;
        m.world.game.units.get_mut(&id).unwrap().dust = Some(1.0);
        let error = m
            .apply_moved_ground_contact(id, before, Some(-5.0))
            .unwrap_err();
        assert_eq!(
            m.world.game.units[&id].dust,
            Some(-5.0),
            "dust follows completed contact"
        );
        if matches!(boundary, "crush" | "mine") {
            assert!(error.contains("not connected"));
        }
        assert_eq!(m.world.game.wear, wear);
        assert_eq!(m.world.game.chars, chars);
        assert_eq!(m.world.journal(), journal);
        assert_eq!(m.world.new_cells(), new_cells);
        assert_eq!(m.world.versions, versions);
    }
}

#[test]
fn numeric_readback_preserves_signed_zero_types_and_absence() {
    use serde_json::json;
    assert!(exact_json_numbers(
        &json!([0, {"wear":1}]),
        &json!([0.0, {"wear":1.0}])
    ));
    assert!(!exact_json_numbers(&json!(-0.0), &json!(0.0)));
    assert!(!exact_json_numbers(&json!(0), &json!("0000000000000000")));
    assert!(!exact_json_numbers(&json!({}), &json!({"wear":null})));
    assert!(!exact_json_numbers(&json!([0, 1]), &json!([1, 0])));
}
