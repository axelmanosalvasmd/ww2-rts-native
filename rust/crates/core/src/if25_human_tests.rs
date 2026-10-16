use super::*;
use crate::human_commander_inputs::Unit;

fn unit(id: u32, owner: u32, x: f64) -> HumanUnit {
    HumanUnit {
        motion: Unit {
            id,
            at: Point { x, z: 20.0 },
            world_goal: None,
            amove: None,
            target_id: 0,
            retreating: false,
        },
        owner,
        kind: "rifle".into(),
        hp: 60.0,
        air: false,
        build: false,
        dig: false,
        entrench: false,
        garrison: false,
    }
}
fn batch() -> OwnerHurtBatch {
    OwnerHurtBatch {
        generation: 0,
        through_ordinal: 0,
        events: vec![],
    }
}
fn control() -> TickControl<'static> {
    TickControl {
        now: 1,
        live_owner_out: false,
        winner: false,
        level: "normal",
    }
}

#[test]
fn if25_ai_scan_uses_visible_insertion_order_for_equal_numeric_keys() {
    let mut human = HumanState::new(1, 0, Point { x: 20.0, z: 20.0 });
    human
        .advance::<(), _>(control(), None, &batch(), |_| {
            panic!("initial callback cannot plan")
        })
        .unwrap();
    let view = HumanView {
        tick: 1,
        spawn: Some(Point { x: 20.0, z: 20.0 }),
        units: vec![unit(1, 0, 20.0), unit(2, 1, 10.0), unit(9, 1, 30.0)],
        visible: vec![9, 2],
        sightings: vec![],
        packet: (),
    };
    let state = human.runtime.as_mut().unwrap();
    let look = state.scan_look(0, 1, &view).unwrap();
    assert_eq!(look.target, Some(Point { x: 30.0, z: 20.0 }));
    let reversed = HumanView {
        visible: vec![2, 9],
        ..view.clone()
    };
    assert_eq!(
        state.scan_look(0, 1, &reversed).unwrap().target,
        Some(Point { x: 10.0, z: 20.0 })
    );
    let zero_view = HumanView {
        units: vec![unit(1, 0, 0.0), unit(2, 1, -0.0), unit(9, 1, 0.0)],
        ..view.clone()
    };
    assert_eq!(validate(1, Some(&zero_view), &batch()), Ok(()));
    let zero = state.scan_look(0, 1, &zero_view).unwrap().target.unwrap();
    assert_eq!(zero.x.to_bits(), 0.0_f64.to_bits());
    let reversed_zero = HumanView {
        visible: vec![2, 9],
        ..zero_view
    };
    assert_eq!(
        state
            .scan_look(0, 1, &reversed_zero)
            .unwrap()
            .target
            .unwrap()
            .x
            .to_bits(),
        (-0.0_f64).to_bits()
    );
    // With no own ground units, every key is Infinity and Infinity-Infinity is NaN.
    let empty_own = HumanView {
        units: view.units.into_iter().filter(|u| u.owner != 0).collect(),
        ..view
    };
    assert_eq!(
        state.scan_look(0, 1, &empty_own).unwrap().target,
        Some(Point { x: 30.0, z: 20.0 })
    );
}

#[test]
fn if25_ai_rejects_nonfinite_positions_before_state_changes() {
    for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut human = HumanState::new(1, 0, Point { x: 20.0, z: 20.0 });
        let view = HumanView {
            tick: 1,
            spawn: None,
            units: vec![unit(1, 0, invalid)],
            visible: vec![1],
            sightings: vec![],
            packet: (),
        };
        assert_eq!(
            human.advance(control(), Some(&view), &batch(), |_| panic!(
                "invalid view reached planner"
            )),
            Err(Error::InvalidView)
        );
        assert!(human.snapshot().is_none());
    }
}
