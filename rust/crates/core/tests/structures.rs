use serde_json::{Value, json};
use ww2_core::{
    map::{AuthoritativeMap, MapError},
    structures::{SectionState, StructuralSection, Structure, StructureKind, supported_sections},
};

fn packed(s: &Structure) -> Value {
    json!([
        s.id,
        s.kind.as_str(),
        s.sections
            .iter()
            .map(|x| json!([
                x.id,
                x.structure_id,
                x.cell,
                x.hp,
                x.max_hp,
                x.material,
                x.anchor,
                x.supports,
                x.state.as_str(),
                x.rubble
            ]))
            .collect::<Vec<_>>()
    ])
}
fn numbers_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => {
            a.as_f64().unwrap().to_bits() == b.as_f64().unwrap().to_bits()
        }
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| numbers_equal(a, b))
        }
        _ => a == b,
    }
}
fn hp_runs(values: &[f32]) -> Value {
    let mut runs: Vec<[u32; 2]> = Vec::new();
    for value in values {
        if let Some(last) = runs.last_mut().filter(|r| r[0] == value.to_bits()) {
            last[1] += 1;
        } else {
            runs.push([value.to_bits(), 1]);
        }
    }
    json!(runs)
}
fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/structures.json")).unwrap()
}

#[test]
fn initial_structures_match_original_validation_graphs_materials_and_health() {
    let fixture = fixture();
    assert_eq!(
        fixture["reference"],
        "e4e579a838aabae3b1d9dadbc17a0a87147bc52d"
    );
    assert_eq!(fixture["node"], "v24.21.0");
    assert_eq!(fixture["arch"], "x64");
    let (mut valid, mut invalid, mut authored) = (0, 0, 0);
    for case in fixture["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let parsed = AuthoritativeMap::from_json(&serde_json::to_vec(&case["map"]).unwrap());
        if !case["error"].is_null() {
            assert!(
                matches!(parsed, Err(MapError::Invalid { .. })),
                "{name}: {parsed:?}"
            );
            invalid += 1;
            continue;
        }
        let map = parsed.unwrap_or_else(|error| panic!("{name}: {error}"));
        assert_eq!(map.source(), &case["map"], "{name}");
        assert_eq!(
            map.structures().len(),
            case["map"]["structures"].as_array().map_or(0, Vec::len)
        );
        let initial = map.initialize_terrain().unwrap();
        assert!(initial.query_game().sections.is_empty());
        let result = map.initialize_structures().unwrap();
        assert_eq!(result.terrain.chars, initial.chars);
        assert_eq!(result.terrain.flags, initial.flags);
        assert_eq!(result.terrain.layers, initial.layers);
        let structures: Vec<_> = result.structures.structures().iter().map(packed).collect();
        assert!(
            numbers_equal(&json!(structures), &case["expected"]["structures"]),
            "{name}: structural graph differs"
        );
        let cells: Vec<_> = result
            .structures
            .structures()
            .iter()
            .flat_map(|s| s.sections.iter().map(|s| s.cell))
            .collect();
        assert_eq!(
            json!(cells),
            case["expected"]["cells"],
            "{name}: cell insertion order"
        );
        assert_eq!(
            hp_runs(&result.terrain.cell_hp),
            case["expected"]["cellHp"],
            "{name}: cell health bits"
        );
        let game = result.query_game();
        assert_eq!(game.sections.len(), cells.len());
        assert!(game.players.is_empty() && game.units.is_empty());
        let supported: Vec<_> = result
            .structures
            .structures()
            .iter()
            .map(supported_sections)
            .collect();
        assert_eq!(
            json!(supported),
            case["expected"]["supported"],
            "{name}: support traversal"
        );
        for structure in result.structures.structures() {
            assert_eq!(result.structures.structure(&structure.id), Some(structure));
            for section in &structure.sections {
                assert_eq!(result.structures.section_at(section.cell), Some(section));
                let query = &game.sections[&(section.cell as u32)];
                assert!(!query.failed);
                assert_eq!(query.hp.to_bits(), section.hp.to_bits());
                assert_eq!(query.max_hp.unwrap().to_bits(), section.max_hp.to_bits());
                assert_eq!(
                    game.cell_hp[section.cell].to_bits(),
                    (section.hp as f32).to_bits()
                );
            }
        }
        let materials: Vec<_> = cells
            .iter()
            .map(|&c| {
                json!([
                    c,
                    result.material_name(c, false),
                    result.material_name(c, true)
                ])
            })
            .collect();
        assert_eq!(
            json!(materials),
            case["expected"]["materials"],
            "{name}: effective materials"
        );
        for probe in case["expected"]["materialProbes"].as_array().unwrap() {
            let cell = probe[0].as_u64().unwrap() as usize;
            assert_eq!(
                result.material_name(cell, false),
                probe[1].as_str(),
                "{name}"
            );
            assert_eq!(
                result.material_name(cell, true),
                probe[2].as_str(),
                "{name}"
            );
        }
        assert_eq!(
            result.material_name(map.dimensions().0 * map.dimensions().1, false),
            None
        );
        valid += 1;
        authored += usize::from(name.starts_with("authored:"));
    }
    assert_eq!((valid, invalid, authored), (59, 36, 33));
}

fn decoded(value: &Value) -> Structure {
    Structure {
        id: value[0].as_str().unwrap().into(),
        kind: match value[1].as_str().unwrap() {
            "house" => StructureKind::House,
            "bridge" => StructureKind::Bridge,
            _ => panic!(),
        },
        sections: value[2]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| StructuralSection {
                id: v[0].as_str().unwrap().into(),
                structure_id: v[1].as_str().unwrap().into(),
                cell: v[2].as_u64().unwrap() as usize,
                hp: v[3].as_f64().unwrap(),
                max_hp: v[4].as_f64().unwrap(),
                material: v[5].as_str().unwrap().into(),
                anchor: v[6].as_bool().unwrap(),
                supports: v[7]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_str().unwrap().into())
                    .collect(),
                state: match v[8].as_str().unwrap() {
                    "intact" => SectionState::Intact,
                    "damaged" => SectionState::Damaged,
                    "burning" => SectionState::Burning,
                    "failed" => SectionState::Failed,
                    _ => panic!(),
                },
                rubble: v[9]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_u64().unwrap() as usize)
                    .collect(),
            })
            .collect(),
    }
}
#[test]
fn support_is_undirected_and_excludes_failed_or_dead_sections() {
    let fixture = fixture();
    let cases = fixture["supportCases"].as_array().unwrap();
    assert_eq!(cases.len(), 8);
    for case in cases {
        let structure = decoded(&case["structure"]);
        assert_eq!(
            json!(supported_sections(&structure)),
            case["expected"],
            "{}",
            case["name"]
        );
    }
}
