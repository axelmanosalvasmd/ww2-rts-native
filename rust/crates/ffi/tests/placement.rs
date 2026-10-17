//! Exact placement preview shapes from e4e579a client/availability.js:169-198 and
//! shared/sim.js:3261-3305, exercised through the public recipient ABI.
use serde_json::{Value, json};
use std::{ffi::c_char, ptr};
use ww2_ffi::*;

struct View(*mut ww2_view);
impl View {
    fn new(map: Value, units: Value) -> Self {
        let mut handle = ptr::null_mut();
        // SAFETY: writable output receives a handle owned by this wrapper.
        assert_eq!(unsafe { ww2_view_new(&mut handle) }, WW2_OK);
        let view = Self(handle);
        view.apply(json!({"t":"start","you":0,"teams":[0,1,0],"map":map}));
        view.apply(json!({"t":"s","all":true,"units":units}));
        view
    }
    fn ordinary() -> Self {
        Self::new(map(), json!([observer(1, 0, 1.0, 1.0)]))
    }
    fn apply(&self, value: Value) {
        let bytes = serde_json::to_vec(&value).unwrap();
        // SAFETY: live exclusively owned view and a readable byte slice.
        assert_eq!(
            unsafe { ww2_view_apply(self.0, bytes.as_ptr().cast(), bytes.len()) },
            WW2_OK
        );
    }
    fn check(&self, request: Value) -> Value {
        let bytes = serde_json::to_vec(&request).unwrap();
        let mut required = usize::MAX;
        // SAFETY: live view, readable request and writable size, with an empty output.
        assert_eq!(
            unsafe {
                ww2_view_placement_check(
                    self.0,
                    bytes.as_ptr().cast(),
                    bytes.len(),
                    ptr::null_mut(),
                    0,
                    &mut required,
                )
            },
            WW2_ERR_CAPACITY
        );
        let mut out = vec![0_u8; required];
        // SAFETY: all buffers and size storage are valid and disjoint.
        assert_eq!(
            unsafe {
                ww2_view_placement_check(
                    self.0,
                    bytes.as_ptr().cast(),
                    bytes.len(),
                    out.as_mut_ptr().cast(),
                    out.len(),
                    &mut required,
                )
            },
            WW2_OK
        );
        serde_json::from_slice(&out[..required]).unwrap()
    }
}
impl Drop for View {
    fn drop(&mut self) {
        // SAFETY: the wrapper frees its owned handle once.
        assert_eq!(unsafe { ww2_view_free(self.0) }, WW2_OK);
    }
}
fn map() -> Value {
    json!({"w":16,"h":16,"rows":vec!["................";16]})
}
fn observer(id: u32, owner: i32, x: f64, z: f64) -> Value {
    json!([id, "rifle", owner, x, z, 0, 0, 100, 0, 0, 0, 0, 0, 0, 1])
}
fn building() -> Value {
    json!({"kind":"barracks","x":9,"z":9})
}
fn footprint() -> Value {
    json!([51, 52, 53, 67, 68, 69, 83, 84, 85])
}

#[test]
fn building_shape_snaps_and_omits_undefined_properties() {
    let view = View::ordinary();
    assert_eq!(
        view.check(building()),
        json!({"ok":true,"x":9.0,"z":9.0,"c":51,"cells":footprint()})
    );
    assert_eq!(
        view.check(json!({"kind":"barracks","x":8.0,"z":8.0})),
        view.check(building())
    );
    assert_eq!(
        view.check(json!({"kind":"barracks","x":31,"z":31})),
        json!({"ok":false,"reason":"blocked","x":31.0,"z":31.0,"c":238,"cells":null})
    );
    assert_eq!(
        view.check(json!({"kind":"barracks","x":-1,"z":9})),
        json!({"ok":false,"reason":"blocked","x":-1.0,"z":9.0})
    );
}

#[test]
fn blocked_kind_logistics_and_nonfinite_shape_have_no_footprint() {
    let view = View::ordinary();
    for kind in ["rifle", "constructor", "supplycache"] {
        assert_eq!(
            view.check(json!({"kind":kind,"x":9,"z":9})),
            json!({"ok":false,"reason":"blocked","x":9.0,"z":9.0})
        );
    }
    assert_eq!(
        view.check(json!({"kind":"barracks","x":null,"z":9})),
        json!({"ok":false,"reason":"blocked","x":0.0,"z":9.0})
    );
    view.apply(json!({"t":"s","units":[],"held":[1,1],"logistics":{"enabled":true}}));
    assert_eq!(
        view.check(json!({"kind":"supplycache","x":9,"z":9}))["ok"],
        true
    );
}

#[test]
fn forts_preserve_plan_order_nest_letters_and_concrete_nulls() {
    let view = View::ordinary();
    for (kind, cells) in [
        (
            "trench",
            json!([[67, "T"], [68, "T"], [69, "T"], [70, "T"]]),
        ),
        (
            "sandbags",
            json!([[67, "#"], [68, "#"], [69, "#"], [70, "#"]]),
        ),
        (
            "wire",
            json!([[66, "X"], [67, "X"], [68, "X"], [69, "X"], [70, "X"]]),
        ),
        ("traps", json!([[67, "Y"], [68, "Y"], [69, "Y"], [70, "Y"]])),
        ("mines", json!([[67, "N"], [68, "N"], [69, "N"], [70, "N"]])),
        ("aid", json!([[68, "A"]])),
        (
            "wall",
            json!([[67, null], [68, null], [69, null], [70, null]]),
        ),
        (
            "nest",
            json!([
                [68, "T"],
                [51, "#"],
                [52, "#"],
                [53, "#"],
                [67, "#"],
                [69, "#"]
            ]),
        ),
    ] {
        assert_eq!(
            view.check(json!({"kind":kind,"x":9,"z":9})),
            json!({"ok":true,"x":9.0,"z":9.0,"cells":cells}),
            "{kind}"
        );
    }
    assert_eq!(
        view.check(json!({"kind":"gate","x":9,"z":9,"dir":std::f64::consts::FRAC_PI_4})),
        json!({"ok":true,"x":9.0,"z":9.0,"cells":[[52,null],[68,null],[84,null]]})
    );
}

#[test]
fn fort_visibility_precedes_blockage_and_partial_plans_keep_order() {
    let mut lookout = observer(1, 0, 1.0, 1.0);
    lookout[1] = json!("hq");
    let view = View::new(map(), json!([lookout]));
    view.apply(json!({"t":"s","units":[],"held":[1,1],"cells":[[68,"H",0,0]]}));
    assert_eq!(
        view.check(json!({"kind":"trench","x":9,"z":9})),
        json!({"ok":true,"x":9.0,"z":9.0,"cells":[[67,"T"],[69,"T"],[70,"T"]]})
    );
    view.apply(json!({"t":"s","units":[],"gone":[1],"held":[0,0]}));
    assert_eq!(
        view.check(json!({"kind":"trench","x":9,"z":9})),
        json!({"ok":false,"reason":"notVisible","x":9.0,"z":9.0,"cells":[]})
    );
}

#[test]
fn only_delivered_allied_observers_supply_visibility() {
    let view = View::new(map(), json!([observer(1, 1, 1.0, 1.0)]));
    let denied =
        json!({"ok":false,"reason":"notVisible","x":9.0,"z":9.0,"c":51,"cells":footprint()});
    assert_eq!(view.check(building()), denied);
    view.apply(json!({"t":"s","units":[observer(2,2,1.0,1.0)],"held":[2,3]}));
    assert_eq!(view.check(building())["ok"], true);
    view.apply(json!({"t":"s","units":[],"gone":[2],"held":[1,1]}));
    assert_eq!(view.check(building()), denied);
}

#[test]
fn unit_and_terrain_deltas_change_placement_without_hidden_state() {
    let view = View::ordinary();
    view.apply(json!({"t":"s","units":[observer(2,1,9.0,9.0)],"held":[2,3]}));
    assert_eq!(view.check(building())["reason"], "blocked");
    view.apply(json!({"t":"s","units":[],"gone":[2],"held":[1,1],"cells":[[85,".",3,0]]}));
    assert_eq!(view.check(building())["reason"], "blocked");
    view.apply(json!({"t":"s","units":[],"held":[1,1]}));
    assert_eq!(view.check(building())["reason"], "blocked");
    view.apply(json!({"t":"s","units":[],"held":[1,1],"cells":[[85,".",1,0]]}));
    assert_eq!(view.check(building())["ok"], true);
}

#[test]
fn depot_returns_the_selected_node_object_and_reserves_nodes_for_other_buildings() {
    let view = View::ordinary();
    view.apply(json!({"t":"s","units":[],"held":[1,1],"nodes":[[8,8,0],[12,8,0]]}));
    assert_eq!(
        view.check(json!({"kind":"depot","x":10,"z":8})),
        json!({"ok":true,"x":8.0,"z":8.0,"c":51,"cells":[51,52,67,68],"node":{"x":8.0,"z":8.0,"c":51,"depot":0}})
    );
    assert_eq!(view.check(building())["reason"], "blocked");
    view.apply(json!({"t":"s","units":[],"held":[1,1]}));
    assert!(
        view.check(json!({"kind":"depot","x":8,"z":8}))
            .get("node")
            .is_some()
    );
}

#[test]
fn coastal_and_logistics_refusals_keep_prototype_precedence() {
    let inland = View::ordinary();
    assert_eq!(
        inland.check(json!({"kind":"shipyard","x":9,"z":9}))["reason"],
        "blocked"
    );
    let mut naval = map();
    naval["naval"] = json!(true);
    let view = View::new(naval, json!([observer(1, 0, 1.0, 1.0)]));
    assert_eq!(
        view.check(json!({"kind":"shipyard","x":9,"z":9}))["reason"],
        "coast"
    );
    view.apply(json!({"t":"s","units":[],"held":[1,1],"cells":[[49,"W",0,0]]}));
    assert_eq!(
        view.check(json!({"kind":"shipyard","x":9,"z":9}))["ok"],
        true
    );
}

#[test]
fn fill_demine_and_bank_supported_bridges_use_received_terrain() {
    let mut lookout = observer(1, 0, 1.0, 1.0);
    lookout[1] = json!("hq");
    let view = View::new(map(), json!([lookout]));
    view.apply(json!({"t":"s","units":[],"held":[1,1],"cells":[[66,"+",0,0],[67,"R",0,0],[68,"F",0,0],[69,".",-1,0],[70,"N",0,0]]}));
    assert_eq!(
        view.check(json!({"kind":"fill","x":9,"z":9}))["cells"],
        json!([[66, "."], [67, "."], [68, "."], [69, "."]])
    );
    assert_eq!(
        view.check(json!({"kind":"demine","x":9,"z":9}))["cells"],
        json!([[70, "."]])
    );
    view.apply(json!({"t":"s","units":[],"held":[1,1],"cells":[[66,"W",0,0],[67,"W",0,0],[68,"W",0,0],[69,"W",0,0],[70,"W",0,0]]}));
    assert_eq!(
        view.check(json!({"kind":"bridge","x":9,"z":9}))["cells"],
        json!([[66, "="], [67, "="], [68, "="], [69, "="], [70, "="]])
    );
}

#[test]
fn world_refusal_is_explicit_and_does_not_poison_the_recipient() {
    let mut world = map();
    world["world"] = json!({"size":"huge"});
    let view = View::new(world, json!([observer(1, 0, 1.0, 1.0)]));
    let bytes = serde_json::to_vec(&building()).unwrap();
    let mut output = [91 as c_char; 16];
    let mut length = 77;
    // SAFETY: the view and all independent buffers are valid for their supplied lengths.
    assert_eq!(
        unsafe {
            ww2_view_placement_check(
                view.0,
                bytes.as_ptr().cast(),
                bytes.len(),
                output.as_mut_ptr(),
                output.len(),
                &mut length,
            )
        },
        WW2_ERR_BAD_ARGUMENT
    );
    assert_eq!(output, [91 as c_char; 16]);
    let mut count = 0;
    // SAFETY: live view and writable count outside it.
    assert_eq!(unsafe { ww2_view_unit_count(view.0, &mut count) }, WW2_OK);
    assert_eq!(count, 1);
}

#[test]
fn recipient_preview_preserves_browser_weather_plane_and_mine_mismatches() {
    let view = View::ordinary();
    view.apply(json!({"t":"s","units":[],"held":[1,1],"weather":["fog"],"wx":[1,1]}));
    // placementState omits weather and rain, so the rifle retains its 36 metre sight.
    assert_eq!(view.check(json!({"kind":"tower","x":31,"z":1}))["ok"], true);
    let mut plane = observer(2, 0, 1.0, 1.0);
    plane[1] = json!("fighter");
    plane[12] = json!(512);
    view.apply(json!({"t":"s","all":true,"units":[plane],"cells":[[2,"B",0,0]]}));
    // snapshotUnits drops air state. The plane uses the same ground LOS as the browser.
    assert_eq!(
        view.check(json!({"kind":"tower","x":9,"z":1}))["reason"],
        "notVisible"
    );
    view.apply(json!({"t":"s","all":true,"units":[observer(1,0,1.0,1.0)],"cells":[[2,".",0,0],[68,"N",0,0,{"ground":".","object":"."}]]}));
    assert_eq!(view.check(building())["ok"], true);
}

#[test]
fn delivered_smoke_and_unfinished_lookouts_bound_visibility() {
    let view = View::ordinary();
    view.apply(json!({"t":"s","units":[],"held":[1,1],"smokes":[[9,9,4]]}));
    assert_eq!(
        view.check(json!({"kind":"barracks","x":17,"z":17}))["reason"],
        "notVisible"
    );
    let mut tower = observer(2, 0, 1.0, 1.0);
    tower[1] = json!("tower");
    tower[14] = json!(0.5);
    view.apply(json!({"t":"s","all":true,"units":[tower],"smokes":[]}));
    assert_eq!(
        view.check(json!({"kind":"tower","x":15,"z":1}))["reason"],
        "notVisible"
    );
    tower[14] = json!(1.0);
    view.apply(json!({"t":"s","units":[tower],"held":[1,2]}));
    assert_eq!(view.check(json!({"kind":"tower","x":15,"z":1}))["ok"], true);
}
