use serde_json::{Value, json};
use ww2_core::client_view::{Applied, ClientView};

fn row(id: u32) -> Value {
    json!([id, "rifle", 0, 10.1, 20.2, 0, 0, 100, 0, 0, 1, 0, 2048, 2, 1, {"future":true}])
}
fn apply(view: &mut ClientView, value: Value) {
    view.apply(&value.to_string()).unwrap();
}
fn start(view: &mut ClientView) {
    apply(
        view,
        json!({"t":"start", "matchId":1, "map":{"w":4,"h":2}, "fog":{"v":"AC", "e":"AD"}, "cells":[[1,"g",0,0,{"ground":"g"}]]}),
    );
}
#[test]
fn deltas_remove_and_replace_rows_and_recover_from_held_mismatch() {
    let mut view = ClientView::new();
    start(&mut view);
    apply(
        &mut view,
        json!({"t":"s", "units":[row(1),row(2)],"held":[2,3],"wrecks":[[7]],"nodes":[[8]],"mp":[50]}),
    );
    assert_eq!(view.units().map(|u| u.id).collect::<Vec<_>>(), [1, 2]);
    assert_eq!(view.unit(1).unwrap().kind, "rifle");
    assert_eq!(view.unit(1).unwrap().raw()[15]["future"], true);
    apply(
        &mut view,
        json!({"t":"s","units":[],"gone":[1],"held":[1,2]}),
    );
    assert!(view.unit(1).is_none());
    assert!(!view.needs_resync());
    assert_eq!(view.state()["wrecks"], json!([[7]]));
    assert_eq!(view.state()["nodes"], json!([[8]]));
    apply(&mut view, json!({"t":"s","units":[],"held":[1,99]}));
    assert!(view.needs_resync());
    apply(
        &mut view,
        json!({"t":"s","all":true,"units":[row(3)],"held":[1,3]}),
    );
    assert!(!view.needs_resync());
    assert_eq!(view.unit_count(), 1);
    assert!(view.unit(3).is_some());
    // Servers predating deltas send no held set and replace the whole unit set.
    apply(&mut view, json!({"t":"s","units":[]}));
    assert_eq!(view.unit_count(), 0);
}
#[test]
fn start_resets_even_the_same_match_and_does_not_decode_start_units() {
    let mut view = ClientView::new();
    start(&mut view);
    apply(&mut view, json!({"t":"s","units":[row(8)],"held":[1,8]}));
    apply(
        &mut view,
        json!({"t":"start","matchId":1,"map":{"w":1,"h":1},"units":[row(9)]}),
    );
    assert_eq!(view.unit_count(), 0);
    assert_eq!(view.cells().len(), 0);
    assert_eq!(view.seen(), [0]);
    assert_eq!(view.explored(), [0]);
    assert!(view.state().is_null());
}
#[test]
fn fog_xor_and_remembered_cells_survive_full_resync_and_accumulate_until_read() {
    let mut view = ClientView::new();
    start(&mut view);
    assert_eq!(view.seen(), [1, 1, 0, 0, 0, 0, 0, 0]);
    assert_eq!(view.explored(), [1, 1, 1, 0, 0, 0, 0, 0]);
    apply(
        &mut view,
        json!({"t":"s","units":[],"held":[0,0],"fog":"BB","cells":[[3,"w",1,2,{"new":7}]]}),
    );
    apply(
        &mut view,
        json!({"t":"s","units":[],"all":true,"held":[0,0],"fog":"DB","cells":[[1,"r",2,3]]}),
    );
    assert_eq!(view.seen(), [1, 0, 0, 1, 0, 0, 0, 0]);
    assert_eq!(view.explored(), [1, 1, 1, 1, 0, 0, 0, 0]);
    assert_eq!(view.cells()[&1], json!([1, "r", 2, 3]));
    assert_eq!(view.cells()[&3][4]["new"], 7);
    assert_eq!(view.changed_cells(), [1, 3]);
    assert_eq!(view.take_changed_cells(), [1, 3]);
    assert!(view.changed_cells().is_empty());
}
#[test]
fn invalid_updates_are_atomic_and_unknown_messages_leave_pending_changes() {
    let mut view = ClientView::new();
    assert!(view.apply(r#"{"t":"s","units":[]}"#).is_err());
    start(&mut view);
    let before = view.clone();
    for bad in [
        json!({"t":"s","units":[row(1)],"held":[1,1],"fog":"_"}),
        json!({"t":"s","units":[row(1)],"cells":[[8,"g"]]}),
        json!({"t":"s","units":[[1,"rifle"]]}),
        json!({"t":"start","map":{"w":u32::MAX,"h":u32::MAX}}),
        json!({"t":"start","map":{"w":2,"h":2},"fog":{"v":"AF","e":""}}),
    ] {
        assert!(view.apply(&bad.to_string()).is_err(), "{bad}");
        assert_eq!(view, before);
    }
    assert_eq!(
        view.apply(r#"{"t":"future","future":true}"#).unwrap(),
        Applied::Ignored
    );
    assert_eq!(view, before);
}

#[test]
fn lobby_pause_and_denial_are_retained_without_resetting_match() {
    let mut view = ClientView::new();
    start(&mut view);
    apply(&mut view, json!({"t":"s","units":[row(1)],"held":[1,1]}));
    apply(&mut view, json!({"t":"lobby","state":"play","you":1}));
    apply(&mut view, json!({"t":"pause","paused":true}));
    apply(
        &mut view,
        json!({"t":"deny","cmd":"move","reason":"blocked"}),
    );
    assert_eq!(view.unit_count(), 1);
    assert_eq!(view.changed_cells(), [1]);
    assert_eq!(view.lobby().unwrap()["you"], 1);
    assert_eq!(view.pause().unwrap()["paused"], true);
    assert_eq!(view.last_denial().unwrap()["reason"], "blocked");
    assert!(view.has_fog_mask());
    assert!(!view.fog_lifted());
    apply(&mut view, json!({"t":"start","map":{"w":1,"h":1}}));
    assert!(!view.has_fog_mask());
    assert!(view.fog_lifted());
    apply(
        &mut view,
        json!({"t":"start","map":{"w":1,"h":1,"world":true}}),
    );
    assert!(!view.has_fog_mask());
    assert!(!view.fog_lifted());
}
