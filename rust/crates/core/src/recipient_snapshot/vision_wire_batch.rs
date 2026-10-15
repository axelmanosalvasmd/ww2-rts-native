//! CMB-132 exercises the production fog delivery state and run encoder.

use super::*;
use crate::grid::SpatialGrid;

fn fixture() -> Value {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/vision-wire-batch.json");
    let bytes =
        std::fs::read(path).expect("generate vision-wire-batch.json with a Root recorder slot");
    let value: Value = serde_json::from_slice(&bytes).expect("vision wire fixture JSON");
    assert_eq!(value["pin"], "e4e579a838aabae3b1d9dadbc17a0a87147bc52d");
    value
}

// Decode independently from the production encoder and apply differing runs.
fn apply_runs(mask: &mut [u8], packed: &str) {
    let digits = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let (mut cell, mut differing, mut value, mut shift) = (0, false, 0usize, 0);
    for byte in packed.bytes() {
        let digit = digits.iter().position(|&d| d == byte).unwrap();
        value |= (digit & 31) << shift;
        if digit & 32 != 0 {
            shift += 5;
            continue;
        }
        assert!(cell + value <= mask.len());
        if differing {
            for bit in &mut mask[cell..cell + value] {
                *bit ^= 1;
            }
        }
        cell += value;
        differing = !differing;
        value = 0;
        shift = 0;
    }
    assert_eq!(shift, 0, "unfinished varint");
}

#[test]
fn cmb132_fog_packets_and_run_roundtrips_match_original_source() {
    let fixture = fixture();
    let cases = fixture["packCases"].as_array().unwrap();
    assert_eq!(cases.len(), 64);
    for case in cases {
        let bytes = |v: &Value| {
            v.as_array()
                .unwrap()
                .iter()
                .map(|n| n.as_u64().unwrap() as u8)
                .collect::<Vec<_>>()
        };
        let mask = bytes(&case["mask"]);
        let base = (!case["base"].is_null()).then(|| bytes(&case["base"]));
        let packed = pack(&mask, base.as_deref());
        assert_eq!(
            packed,
            case["expected"].as_str().unwrap(),
            "{}",
            case["name"]
        );
        let mut decoded = base.unwrap_or_else(|| vec![0; mask.len()]);
        apply_runs(&mut decoded, &packed);
        assert_eq!(decoded, mask, "{}", case["name"]);
    }

    let mut state = crate::recipient_snapshot::tests::initialize(&fixture["initial"]);
    let mut delivery = DeliveryState::new(&state).unwrap();
    let actions = fixture["actions"].as_array().unwrap();
    assert_eq!(actions.len(), 8);
    let cells = state.world.game.w * state.world.game.h;
    let mut decoded = vec![vec![0; cells]; 3];
    for action in actions {
        if let Some(tick) = action["tick"].as_i64() {
            state.world.game.tick = tick;
        }
        if action["move"].is_object() {
            let movement = &action["move"];
            let u = state
                .world
                .game
                .units
                .get_mut(&(movement["id"].as_u64().unwrap() as u32))
                .unwrap();
            u.x = movement["x"].as_f64().unwrap();
            u.z = movement["z"].as_f64().unwrap();
        }
        if action["vision"] == true {
            let grid = SpatialGrid::new(&state.world.game.units, 16.0);
            vision::update_vision(&mut state.world.game, &state.rules, &grid);
        }
        let flags = |key: &str| {
            (0..3)
                .map(|slot| action[key][slot].as_bool().unwrap_or(false))
                .collect::<Vec<_>>()
        };
        let frame = delivery
            .capture(&mut state, &flags("full"), &flags("omit"))
            .unwrap();
        for (slot, mask) in decoded.iter_mut().enumerate() {
            let fields = frame.recipient(slot).unwrap().fields;
            let expected = &action["outputs"][slot];
            assert_eq!(
                fields.get("fog"),
                expected.get("fog"),
                "{} slot={slot}",
                action["name"]
            );
            if let Some(packet) = fields.get("fog") {
                if let Some(packed) = packet.as_str() {
                    apply_runs(mask, packed);
                } else {
                    mask.fill(0);
                    apply_runs(mask, packet["v"].as_str().unwrap());
                    let mut explored = vec![0; cells];
                    apply_runs(&mut explored, packet["e"].as_str().unwrap());
                    assert!(mask.iter().zip(explored).all(|(&v, e)| v <= e));
                }
            }
            assert_eq!(
                json!(mask),
                expected["base"],
                "{} decoded slot={slot}",
                action["name"]
            );
        }
        drop(frame);
        for (slot, expected) in action["outputs"].as_array().unwrap().iter().enumerate() {
            assert_eq!(json!(delivery.seats[slot].version), expected["version"]);
            assert_eq!(json!(delivery.seats[slot].base), expected["base"]);
        }
    }
}
