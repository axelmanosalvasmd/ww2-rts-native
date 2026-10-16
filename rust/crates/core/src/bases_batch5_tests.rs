//! Partial BASE-48 evidence. Gate admission and snapshots keep the full row open.
use super::*;
use crate::recipient_snapshot::DeliveryState;
use std::collections::BTreeMap;

#[test]
fn base48_exact_tier_table_defaults_and_fort_requirements() {
    // Literal entries from pinned shared/tech.js:4-10, not a native table copy.
    let expected: BTreeMap<_, _> = [
        ("motorpool", 2),
        ("shipyard", 2),
        ("armory", 2),
        ("pillbox", 2),
        ("halftrack", 2),
        ("armoredcar", 2),
        ("flaktrack", 2),
        ("tank", 2),
        ("at", 2),
        ("mortar", 2),
        ("flak", 2),
        ("lcvp", 2),
        ("gunboat", 2),
        ("airfield", 3),
        ("medium", 3),
        ("tankdestroyer", 3),
        ("rocket", 3),
        ("howitzer", 3),
        ("tiger", 3),
        ("churchill", 3),
        ("ranger", 3),
        ("commando", 3),
        ("destroyer", 3),
        ("fighter", 3),
        ("attacker", 3),
        ("bomber", 3),
    ]
    .into_iter()
    .collect();
    let catalog = Catalog::compiled().unwrap();
    let actual = catalog
        .table("TIER")
        .unwrap()
        .object()
        .unwrap()
        .iter()
        .map(|(kind, tier)| (kind.as_str(), tier.number().unwrap()))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(
        actual,
        expected.iter().map(|(&k, &v)| (k, f64::from(v))).collect()
    );
    // Also check the default branch for every known type omitted from TIER.
    for kind in catalog.table("UNITS").unwrap().object().unwrap().keys() {
        assert_eq!(
            economy::tier_of(kind).unwrap(),
            expected.get(kind.as_str()).copied().unwrap_or(1),
            "{kind}"
        );
    }
    for kind in ["wall", "gate"] {
        assert_eq!(
            catalog
                .table("FORTS")
                .unwrap()
                .get(kind)
                .unwrap()
                .get("tier")
                .and_then(Value::number),
            Some(2.)
        );
    }
}

#[test]
fn base48_admitted_command_families_reject_low_tier_without_side_effects() {
    let mut m = ordinary(true);
    // Build checks MP before tier, so earn enough for the Pillbox first.
    for _ in 0..400 {
        m.advance_tick().unwrap();
    }
    assert!(m.start.economy.players[0].mp >= 200.);
    let worker = own(&m, 0, "rifle");
    let barracks = own(&m, 0, "barracks");
    for command in [
        json!({"t":"buy","unit":"mortar","from":barracks}),
        json!({"t":"build","kind":"pillbox","ids":[worker],"x":30,"z":25}),
        json!({"t":"dig","kind":"wall","ids":[worker],"x":30,"z":25}),
        json!({"t":"entrench","fort":"wall","pattern":"line","ids":[worker],"x":30,"z":25,"x2":40,"z2":25}),
    ] {
        let before = format!(
            "{:?}",
            (
                &m.world,
                &m.start.economy,
                &m.commands.units,
                &m.production,
                &m.field_works,
                &m.story,
                m.next_unit_id
            )
        );
        denied(&mut m, 0, command.clone(), "tier");
        assert_eq!(
            format!(
                "{:?}",
                (
                    &m.world,
                    &m.start.economy,
                    &m.commands.units,
                    &m.production,
                    &m.field_works,
                    &m.story,
                    m.next_unit_id
                )
            ),
            before,
            "{command}"
        );
    }
    let mut delivery = DeliveryState::new(&m).unwrap();
    for _ in 0..3 {
        m.advance_tick().unwrap();
        let frame = delivery.capture(&mut m, &[false; 2], &[false; 2]).unwrap();
        for slot in 0..2 {
            frame.recipient(slot).unwrap();
        }
    }
    assert!(m.tick_state.fault().is_none());
}

#[test]
fn base48_gate_request_stays_out_of_ordinary_match_state() {
    let mut m = ordinary(true);
    let worker = own(&m, 0, "rifle");
    let before = format!(
        "{:?}",
        (
            &m.world,
            &m.start.economy,
            &m.commands.units,
            &m.production,
            &m.field_works,
            &m.story,
            m.next_unit_id
        )
    );
    denied(
        &mut m,
        0,
        json!({"t":"dig","kind":"gate","ids":[worker],"x":30,"z":25}),
        "unsupported",
    );
    assert_eq!(
        format!(
            "{:?}",
            (
                &m.world,
                &m.start.economy,
                &m.commands.units,
                &m.production,
                &m.field_works,
                &m.story,
                m.next_unit_id
            )
        ),
        before
    );
    // Do not turn this guard into a full BASE-48 claim: no Gate was admitted.
    assert!(m.field_works.jobs().is_empty());
    let mut delivery = DeliveryState::new(&m).unwrap();
    for _ in 0..3 {
        m.advance_tick().unwrap();
        let frame = delivery.capture(&mut m, &[false; 2], &[false; 2]).unwrap();
        for slot in 0..2 {
            frame.recipient(slot).unwrap();
        }
    }
    assert!(m.tick_state.fault().is_none());
}
