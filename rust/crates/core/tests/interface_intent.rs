//! Static recipient tables and the pure providers must describe the same geometry.
use ww2_core::{
    catalog::Value,
    field_works::{self, FortKind, Pattern},
    formation::{self, Item, Shape},
    game::Point,
    squad_men, startup_tables,
};

fn tables() -> Value {
    startup_tables::parse_json(startup_tables::compiled_json().unwrap()).unwrap()
}

fn number(value: &Value, field: &str) -> f64 {
    value.get(field).unwrap().number().unwrap()
}

#[test]
fn exported_squad_slots_match_every_health_transition_and_marching_position() {
    let root = tables();
    let tables = root.get("tables").unwrap();
    let scale = number(tables, "SLOT_SCALE");
    assert_eq!(scale.to_bits(), squad_men::SLOT_SCALE.to_bits());
    let slots = tables.get("SLOTS").unwrap().object().unwrap();
    for (kind, exported) in slots {
        let exported = exported.array().unwrap();
        let slot_count = exported.len();
        let max_hp = slot_count as f64;
        // These sine/cosine bits are recorded in fixtures/math.txt.
        for (rotation, cosine, sine) in [
            (0.0, 1.0, 0.0),
            (
                std::f64::consts::FRAC_PI_2,
                f64::from_bits(0x3c91_a626_3314_5c07),
                1.0,
            ),
            (
                std::f64::consts::PI,
                -1.0,
                f64::from_bits(0x3ca1_a626_3314_5c07),
            ),
        ] {
            // Half a hit point either side of each boundary gives exact binary inputs.
            for half_hp in 0..=2 * exported.len() + 1 {
                let hp = half_hp as f64 / 2.0;
                let expected_count = half_hp.div_ceil(2);
                let drawn = squad_men::drawn_men(
                    kind,
                    hp,
                    max_hp,
                    Point { x: 17.0, z: 23.0 },
                    rotation,
                    Point { x: 9.0, z: -18.0 },
                )
                .unwrap();
                assert_eq!(drawn.count, expected_count as f64, "{kind}, hp {hp}");
                assert_eq!(drawn.positions.len(), expected_count.min(exported.len()));
                for (actual, expected) in drawn.positions.iter().zip(exported) {
                    let expected = expected.array().unwrap();
                    let sx = expected[0].number().unwrap();
                    let sz = expected[1].number().unwrap();
                    let x = 17.0 + (sx * cosine - sz * sine) * scale - 1.0;
                    let z = 23.0 + (sx * sine + sz * cosine) * scale + 2.0;
                    assert_eq!(actual.x.to_bits(), x.to_bits(), "{kind}, hp {hp}");
                    assert_eq!(actual.z.to_bits(), z.to_bits(), "{kind}, hp {hp}");
                }
            }
        }

        // Catalog health is independent of the number of rendered slots.
        let unit = tables.get("UNITS").unwrap().get(kind).unwrap();
        let catalog_max_hp = number(unit, "hpPer") * number(unit, "models");
        for (hp, expected_count) in [
            (0.0, 0),
            (catalog_max_hp, slot_count),
            (2.0 * catalog_max_hp, 2 * slot_count),
        ] {
            let drawn = squad_men::drawn_men(
                kind,
                hp,
                catalog_max_hp,
                Point { x: 17.0, z: 23.0 },
                0.0,
                Point { x: 0.0, z: 0.0 },
            )
            .unwrap();
            assert_eq!(drawn.count, expected_count as f64, "{kind}, hp {hp}");
            assert_eq!(drawn.positions.len(), expected_count.min(slot_count));
        }

        // With this low maximum, each half hit point crosses one slot threshold.
        let low_max_hp = slot_count as f64 / 2.0;
        for expected_count in 1..=slot_count {
            let boundary_hp = expected_count as f64 / 2.0;
            for (hp, expected) in [
                (boundary_hp - 0.25, expected_count),
                (boundary_hp, expected_count),
                (boundary_hp + 0.25, expected_count + 1),
            ] {
                let drawn = squad_men::drawn_men(
                    kind,
                    hp,
                    low_max_hp,
                    Point { x: 17.0, z: 23.0 },
                    0.0,
                    Point { x: 0.0, z: 0.0 },
                )
                .unwrap();
                assert_eq!(drawn.count, expected as f64, "{kind}, hp {hp}");
                assert_eq!(drawn.positions.len(), expected.min(slot_count));
            }
        }
    }
    for (kind, gun_slots) in tables.get("GUN_SLOTS").unwrap().object().unwrap() {
        assert_eq!(slots.get(kind), Some(gun_slots), "{kind} gun crew");
    }
}

#[test]
fn every_exported_unit_has_a_formation_slot_and_every_exported_shape_places_it() {
    let root = tables();
    let tables = root.get("tables").unwrap();
    let units = tables.get("UNITS").unwrap().object().unwrap();
    let items: Vec<_> = units
        .iter()
        .enumerate()
        .map(|(i, (_, unit))| {
            let infantry = unit.get("infantry").and_then(Value::boolean) == Some(true);
            let radius = number(unit, "radius");
            let size = formation::slot_size(infantry, radius);
            assert_eq!(
                size,
                (if infantry { 3.0_f64 } else { 5.0 }).max(2.0 * radius + 1.5)
            );
            Item {
                id: i as u32 + 1,
                x: 0.0,
                z: i as f64 * 10.0,
                size,
                back: false,
            }
        })
        .collect();
    assert_eq!(items.len(), 47);
    let shapes = tables.get("SHAPES").unwrap().array().unwrap();
    assert_eq!(shapes.len(), formation::SHAPES.len());
    for (shape, name) in shapes.iter().zip(formation::SHAPES) {
        assert_eq!(shape.string(), Some(name));
        let shape = Shape::from_name(name).unwrap();
        let spots = formation::facing_spots(&items, 100.0, 200.0, 0.0, 90.0, shape, 1.0);
        let ids: std::collections::BTreeSet<_> = spots.iter().map(|spot| spot.id).collect();
        assert_eq!(ids, items.iter().map(|item| item.id).collect());
        assert_eq!(spots.len(), items.len());
        assert!(
            spots
                .iter()
                .all(|spot| spot.x.is_finite() && spot.z.is_finite())
        );
    }
}

#[test]
fn exported_line_fort_prices_and_lengths_match_the_pure_preview_provider() {
    let root = tables();
    let tables = root.get("tables").unwrap();
    let cell = number(tables, "CELL");
    for (name, definition) in tables.get("FORTS").unwrap().object().unwrap() {
        let Ok(kind) = FortKind::parse(name) else {
            continue;
        };
        if !field_works::line_fort(kind) {
            continue;
        }
        let cells = number(definition, "n") as usize;
        let cost = number(definition, "cost");
        assert_eq!(field_works::segment_cost(kind, cells).unwrap(), cost);
        assert_eq!(field_works::segment_cost(kind, 0).unwrap(), 1.0);
        let at = Point { x: 100.0, z: 100.0 };
        let one = field_works::entrench_plan(Pattern::Line, at, at, None, kind).unwrap();
        assert_eq!(one.len(), 1, "{name}");
        assert_eq!(one[0].kind, kind);
        let end = Point {
            x: at.x + 2.0 * cells as f64 * cell,
            z: at.z,
        };
        let two = field_works::entrench_plan(Pattern::Line, at, end, None, kind).unwrap();
        assert_eq!(two.len(), 2, "{name}");
        assert!(two.iter().all(|segment| segment.kind == kind));
        assert_eq!((two[1].at.x - two[0].at.x).abs(), cells as f64 * cell);
    }
}
