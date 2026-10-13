//! Exercises the public view ABI with caller-owned inputs and outputs.

use std::{ffi::c_char, ptr};
use ww2_ffi::*;

struct View(*mut ww2_view);
impl View {
    fn new() -> Self {
        let mut handle = ptr::null_mut();
        // SAFETY: writable pointer storage receives the handle owned by this wrapper.
        assert_eq!(unsafe { ww2_view_new(&mut handle) }, WW2_OK);
        assert!(!handle.is_null());
        Self(handle)
    }
    fn apply(&self, message: &str) -> i32 {
        // SAFETY: this wrapper exclusively owns a live handle, and message is readable for len bytes.
        unsafe { ww2_view_apply(self.0, message.as_ptr().cast(), message.len()) }
    }
    fn count(&self) -> usize {
        let mut count = usize::MAX;
        // SAFETY: handle is live and count is writable outside it.
        assert_eq!(unsafe { ww2_view_unit_count(self.0, &mut count) }, WW2_OK);
        count
    }
}
impl Drop for View {
    fn drop(&mut self) {
        // SAFETY: this wrapper owns the handle and frees it exactly once.
        assert_eq!(unsafe { ww2_view_free(self.0) }, WW2_OK);
    }
}

const START: &str = r#"{"t":"start","map":{"w":1,"h":1}}"#;
const SNAPSHOT: &str = r#"{"t":"s","units":[[7,"future_rifle",-1,1.25,2.5,0.5,-0.5,99,3,9,2,0.75,2147483648,4,0.25],[2,"tank",0,0,0,0,0,100,0,0,0,0,0,0,1]],"held":[2,5]}"#;

fn sentinel() -> ww2_view_unit {
    ww2_view_unit {
        id: 99,
        owner: 99,
        kind: [99; 64],
        x: 99.0,
        z: 99.0,
        rot: 99.0,
        aim: 99.0,
        hp: 99.0,
        supp: 99.0,
        cd: 99.0,
        built: 99.0,
        target: 99,
        cover: 99,
        flags: 99,
        stars: 99,
    }
}

#[test]
fn complete_rows_sorted_and_copied_only_with_capacity() {
    let view = View::new();
    assert_eq!(view.count(), 0);
    assert_eq!(view.apply(START), WW2_OK);
    assert_eq!(view.apply(SNAPSHOT), WW2_OK);
    let mut rows = [sentinel(); 2];
    let mut len = 99;
    let mut resync = 99;
    // SAFETY: all outputs are writable; the deliberately short capacity still lies within rows.
    unsafe {
        assert_eq!(
            ww2_view_units(view.0, ptr::null_mut(), 0, &mut len),
            WW2_ERR_CAPACITY
        );
        assert_eq!(len, 2);
        assert_eq!(
            ww2_view_units(view.0, rows.as_mut_ptr(), 1, &mut len),
            WW2_ERR_CAPACITY
        );
        assert_eq!(rows, [sentinel(); 2]);
        assert_eq!(
            ww2_view_units(view.0, rows.as_mut_ptr(), 2, &mut len),
            WW2_OK
        );
        assert_eq!(ww2_view_needs_resync(view.0, &mut resync), WW2_OK);
    }
    assert_eq!(resync, 0);
    assert_eq!(len, 2);
    assert_eq!(rows[0].id, 2);
    let unit = &rows[1];
    assert_eq!((unit.id, unit.owner), (7, -1));
    assert_eq!(
        &unit.kind[..12],
        b"future_rifle"
            .iter()
            .map(|&b| b as c_char)
            .collect::<Vec<_>>()
    );
    assert!(unit.kind[12..].iter().all(|&c| c == 0));
    assert_eq!((unit.x, unit.z, unit.rot, unit.aim), (1.25, 2.5, 0.5, -0.5));
    assert_eq!(
        (unit.hp, unit.supp, unit.cd, unit.built),
        (99.0, 3.0, 0.75, 0.25)
    );
    assert_eq!(
        (unit.target, unit.cover, unit.flags, unit.stars),
        (9, 2, 2147483648, 4)
    );
    assert_eq!(std::mem::size_of::<ww2_view_unit>(), 152);
    assert_eq!(std::mem::offset_of!(ww2_view_unit, kind), 8);
    assert_eq!(std::mem::offset_of!(ww2_view_unit, x), 72);
    assert_eq!(std::mem::offset_of!(ww2_view_unit, target), 136);
    assert_eq!(ww2_abi_version(), 2);
}

#[test]
fn invalid_inputs_leave_state_unchanged_and_report_errors() {
    let view = View::new();
    assert_eq!(view.apply(SNAPSHOT), WW2_ERR_WRONG_STATE);
    assert_eq!(view.apply(START), WW2_OK);
    assert_eq!(view.apply(SNAPSHOT), WW2_OK);
    assert_eq!(view.apply("{"), WW2_ERR_PARSE);
    assert_eq!(view.apply(r#"{"t":"s","units":[[8]]}"#), WW2_ERR_PARSE);
    let invalid_utf8 = [0xffu8];
    let mut count = 99;
    let mut resync = 99;
    // SAFETY: invalid byte content is intentional; valid pointers and lengths still obey the ABI.
    // Oversized lengths and null nonempty buffers must be rejected before any read.
    unsafe {
        assert_eq!(
            ww2_view_apply(view.0, invalid_utf8.as_ptr().cast(), 1),
            WW2_ERR_PARSE
        );
        assert_eq!(ww2_view_apply(view.0, ptr::null(), 0), WW2_ERR_PARSE);
        assert_eq!(ww2_view_apply(view.0, ptr::null(), 1), WW2_ERR_NULL_POINTER);
        assert_eq!(
            ww2_view_apply(view.0, invalid_utf8.as_ptr().cast(), usize::MAX),
            WW2_ERR_BAD_ARGUMENT
        );
        assert_eq!(ww2_view_new(ptr::null_mut()), WW2_ERR_NULL_POINTER);
        assert_eq!(ww2_view_free(ptr::null_mut()), WW2_OK);
        assert_eq!(
            ww2_view_apply(ptr::null_mut(), ptr::null(), 0),
            WW2_ERR_NULL_POINTER
        );
        assert_eq!(
            ww2_view_unit_count(ptr::null_mut(), &mut count),
            WW2_ERR_NULL_POINTER
        );
        assert_eq!(
            ww2_view_unit_count(view.0, ptr::null_mut()),
            WW2_ERR_NULL_POINTER
        );
        assert_eq!(
            ww2_view_needs_resync(ptr::null_mut(), &mut resync),
            WW2_ERR_NULL_POINTER
        );
        assert_eq!(
            ww2_view_needs_resync(view.0, ptr::null_mut()),
            WW2_ERR_NULL_POINTER
        );
        assert_eq!(
            ww2_view_units(view.0, ptr::null_mut(), 2, &mut count),
            WW2_ERR_NULL_POINTER
        );
        assert_eq!(
            ww2_view_units(view.0, ptr::null_mut(), 0, ptr::null_mut()),
            WW2_ERR_NULL_POINTER
        );
        assert_eq!(
            ww2_view_units(view.0, ptr::null_mut(), usize::MAX, &mut count),
            WW2_ERR_BAD_ARGUMENT
        );
    }
    assert_eq!(view.count(), 2);
}

#[test]
fn unrepresentable_kind_never_truncates_or_partially_writes() {
    for kind in ["x".repeat(64), "rifle\\u0000suffix".into()] {
        let view = View::new();
        assert_eq!(view.apply(START), WW2_OK);
        assert_eq!(view.apply(&format!(r#"{{"t":"s","units":[[1,"ok",0,0,0,0,0,1,0,0,0,0,0,0,1],[2,"{kind}",0,0,0,0,0,1,0,0,0,0,0,0,1]],"held":[2,3]}}"#)), WW2_OK);
        let mut rows = [sentinel(); 2];
        let mut len = 0;
        // SAFETY: rows has capacity for the two units, and len is writable.
        assert_eq!(
            unsafe { ww2_view_units(view.0, rows.as_mut_ptr(), 2, &mut len) },
            WW2_ERR_BAD_ARGUMENT
        );
        assert_eq!(rows, [sentinel(); 2]);
        assert_eq!(len, 2);
    }
}

#[test]
fn held_set_mismatch_and_full_resync_cross_the_abi() {
    let view = View::new();
    assert_eq!(view.apply(START), WW2_OK);
    assert_eq!(view.apply(r#"{"t":"s","units":[],"held":[1,7]}"#), WW2_OK);
    let mut resync = 0;
    // SAFETY: live exclusive handle and writable byte outside it.
    unsafe {
        assert_eq!(ww2_view_needs_resync(view.0, &mut resync), WW2_OK);
        assert_eq!(resync, 1);
    }
    assert_eq!(
        view.apply(r#"{"t":"s","all":true,"units":[],"held":[0,0]}"#),
        WW2_OK
    );
    let mut len = 99;
    // SAFETY: zero-capacity null output is valid for the empty view.
    unsafe {
        assert_eq!(ww2_view_needs_resync(view.0, &mut resync), WW2_OK);
        assert_eq!(resync, 0);
        assert_eq!(ww2_view_units(view.0, ptr::null_mut(), 0, &mut len), WW2_OK);
        assert_eq!(len, 0);
    }
}
