use serde_json::{Value, json};
use ww2_core::client_view::{CellState, ClientView, start_state};

fn apply(view: &mut ClientView, message: Value) {
    view.apply(&message.to_string()).unwrap();
}

#[test]
fn recipient_cell_state_composes_defaults_overrides_and_reset() {
    // Pinned sim.js:2347-2356, evaluated as pure functions without a Game.
    for (terrain, expected) in [
        ('M', [3, 3, 2, 2, 2, 2, 1, 2, 2, 0, 1, 0, 2, 1, 2, 1]),
        ('F', [3, 3, 2, 2, 2, 2, 1, 2, 2, 0, 1, 0, 2, 1, 2, 1]),
        ('+', [2, 2, 2, 1, 2, 1, 1, 1, 2, 1, 1, 1, 1, 1, 1, 1]),
    ] {
        for (index, packed) in expected.into_iter().enumerate() {
            assert_eq!(start_state(terrain, index as u32).packed(), packed);
        }
    }
    for terrain in ".DBH#TXY=RNOQW".chars() {
        assert_eq!(start_state(terrain, 0).packed(), 0);
    }
    // Original wear is 0.7499999976716936. Rounding it to f32 first gives the wrong stage 3.
    assert_eq!(start_state('M', 3_037_959).packed(), 2);
    for (wear, burnt, hp, max, scar, packed) in [
        (0.0, false, 100.0, 100.0, 0.0, 0),
        (0.25, false, 67.0, 100.0, 0.02, 1),
        (0.5, true, 66.0, 100.0, 0.020000000000000004, 46),
        (0.75, false, 34.0, 100.0, 0.3, 43),
        (1.0, true, 33.0, 100.0, 0.30000000000000004, 87),
        (2.0, true, 0.0, 100.0, 0.6, 87),
        (0.0, false, 0.0, 1.0, 0.6000000000000001, 96),
    ] {
        assert_eq!(CellState::pack(wear, burnt, hp, max, scar).packed(), packed);
    }
    for packed in 0..=255 {
        let state = CellState::from_packed(packed);
        assert_eq!(u32::from(state.packed()), packed);
        assert_eq!(u32::from(state.wear()), packed & 3);
        assert_eq!(state.burnt(), packed & 4 != 0);
        assert_eq!(u32::from(state.damage()), packed >> 3 & 3);
        assert_eq!(u32::from(state.scar()), packed >> 5 & 3);
    }

    let mut view = ClientView::new();
    assert!(view.cell(0).is_none());
    let start = json!({
        "t":"start", "matchId":7,
        "map":{"w":8,"h":2,"rows":["MF+N=BR.",".D+MWF#T"],"heights":["01234ab0","00000000"]},
        "cells":[[8,"N",-2,119,{"ground":"D","object":".","material":"road"},{"future":true}]]
    });
    apply(&mut view, start.clone());
    assert_eq!(view.start(), Some(&start));
    for (index, terrain, ground, object, mine, physical, level, packed) in [
        (0, 'M', 'M', '.', false, 'M', 0.0, 3),
        (1, 'F', 'F', '.', false, 'F', 1.0, 3),
        (2, '+', '+', '.', false, '+', 2.0, 2),
        (3, 'N', '.', '.', true, '.', 3.0, 0),
        (4, '=', 'W', '=', false, '=', 4.0, 0),
        (5, 'B', '.', 'B', false, 'B', -1.0, 0),
        (6, 'R', '.', 'R', false, 'R', -2.0, 0),
        (8, 'N', 'D', '.', true, 'D', -2.0, 119),
    ] {
        let cell = view.cell(index).unwrap();
        assert_eq!(
            (
                cell.terrain,
                cell.ground,
                cell.object,
                cell.mine,
                cell.physical,
                cell.level,
                cell.state.packed()
            ),
            (
                terrain,
                Some(ground),
                Some(object),
                mine,
                Some(physical),
                level,
                packed
            ),
        );
    }
    assert!(view.cell(16).is_none());
    assert_eq!(view.cells()[&8], start["cells"][0]);
    assert_eq!(view.cell(8).unwrap().data.unwrap()["material"], "road");

    // main.js:523,687-701: short rows reset state, but retain earlier height and layers.
    apply(&mut view, json!({"t":"s","units":[],"cells":[[8,"N"]]}));
    let cell = view.cell(8).unwrap();
    assert_eq!(
        (cell.state.packed(), cell.level, cell.physical),
        (0, -2.0, Some('D'))
    );
    assert_eq!(cell.data.unwrap()["material"], "road");
    assert_eq!(view.cells()[&8], json!([8, "N"]));
    apply(
        &mut view,
        json!({
            "t":"s","units":[],"cells":[
                [9,"B",-1,64,{"ground":".","object":"B","material":"stone"}],
                [9,"R"]
            ]
        }),
    );
    let cell = view.cell(9).unwrap();
    assert_eq!(
        (
            cell.terrain,
            cell.level,
            cell.state.packed(),
            cell.object,
            cell.physical
        ),
        ('R', -1.0, 0, Some('B'), Some('R'))
    );
    assert_eq!(view.cells()[&9], json!([9, "R"]));
    apply(
        &mut view,
        json!({
            "t":"s","units":[],"cells":[[9,"N",-1,u32::MAX,null,{"future":2}]]
        }),
    );
    let cell = view.cell(9).unwrap();
    assert_eq!(
        (cell.mine, cell.physical, cell.state.packed()),
        (true, Some('B'), 255)
    );
    assert_eq!(cell.data.unwrap()["material"], "stone");
    assert_eq!(
        view.cells()[&9],
        json!([9,"N",-1,u32::MAX,null,{"future":2}])
    );
    apply(
        &mut view,
        json!({"t":"s","units":[],"all":true,"held":[0,0]}),
    );
    assert_eq!(view.cell(8).unwrap().physical, Some('D'));
    assert_eq!(view.cell(9).unwrap().level, -1.0);
    assert_eq!(view.take_changed_cells(), [8, 9]);
    assert!(view.changed_cells().is_empty());
    assert_eq!(view.cell(9).unwrap().state.packed(), 255);
    let before = view.clone();
    assert!(
        view.apply(
            &json!({"t":"s","units":[],"cells":[[9,"T",2,0,{"ground":"M","object":"T"}],[16,"."]]})
                .to_string()
        )
        .is_err()
    );
    assert_eq!(view, before);

    // world-layers.js:19-27,53-65: both authored-object mines and separate mine markers.
    apply(
        &mut view,
        json!({
            "t":"start","matchId":7,
            "map":{"w":4,"h":1,"worldVersion":2,"rows":["NTNR"],
                "layers":{"ground":["DM+."],"objects":["NT.R"],"mines":["..N."]}}
        }),
    );
    assert!(view.cells().is_empty());
    assert!(view.changed_cells().is_empty());
    assert!(view.cell(8).is_none());
    for (index, terrain, ground, object, mine, physical) in [
        (0, 'N', 'D', '.', true, 'D'),
        (1, 'T', 'M', 'T', false, 'T'),
        (2, 'N', '+', '.', true, '+'),
        (3, 'R', '.', 'R', false, 'R'),
    ] {
        let cell = view.cell(index).unwrap();
        assert_eq!(
            (
                cell.terrain,
                cell.ground,
                cell.object,
                cell.mine,
                cell.physical,
                cell.level,
                cell.state.packed(),
                cell.data
            ),
            (
                terrain,
                Some(ground),
                Some(object),
                mine,
                Some(physical),
                0.0,
                0,
                None
            ),
        );
    }

    // The existing decoder also accepts starts without rows. Do not require an authoritative map.
    apply(
        &mut view,
        json!({"t":"start","matchId":7,"map":{"w":1,"h":1}}),
    );
    assert!(view.cell(0).is_none());
    apply(&mut view, json!({"t":"s","units":[],"cells":[[0,"F"]]}));
    let cell = view.cell(0).unwrap();
    assert_eq!(
        (
            cell.terrain,
            cell.state.packed(),
            cell.level,
            cell.ground,
            cell.object,
            cell.physical
        ),
        ('F', 0, 0.0, None, None, Some('F'))
    );
}
