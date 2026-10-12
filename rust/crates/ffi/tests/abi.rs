//! Calls the C entry points as Unreal would: results match the core, and bad arguments come back as
//! error codes with a message, never as undefined behavior.

use std::ptr;

use ww2_core::formation::{self, Item, Shape};
use ww2_ffi::*;

fn last_error() -> String {
    let mut buf = [0; 256];
    let mut len = 0;
    // SAFETY: buf has 256 writable bytes and len is a writable usize.
    let code = unsafe { ww2_last_error(buf.as_mut_ptr(), buf.len(), &mut len) };
    assert_eq!(code, WW2_OK);
    buf[..len].iter().map(|&c| c as u8 as char).collect()
}

fn items() -> Vec<ww2_formation_item> {
    let item = |id, back, x, z, size| ww2_formation_item {
        id,
        back,
        x,
        z,
        size,
    };
    vec![
        item(11, 0, 100.0, 100.0, 3.0),
        item(12, 0, 104.0, 101.0, 5.9),
        item(13, 1, 98.0, 96.0, 3.0),
        item(4_000_000_000, 0, 103.0, 92.0, 7.5),
        item(15, 2, 96.0, 104.0, 3.0),
    ]
}

/// Every field crosses the boundary: ids, back flags, positions and sizes give the core's answer.
#[test]
fn spots_match_the_core() {
    let items = items();
    let core_items: Vec<Item> = items
        .iter()
        .map(|u| Item {
            id: u.id,
            x: u.x,
            z: u.z,
            size: u.size,
            back: u.back != 0,
        })
        .collect();
    let expected = formation::facing_spots(&core_items, 120.0, 80.0, 0.7, 30.0, Shape::Block, 1.25);
    let mut out = vec![
        ww2_formation_spot {
            id: 0,
            x: 0.0,
            z: 0.0
        };
        items.len()
    ];
    let mut len = usize::MAX;
    // SAFETY: items and out hold items.len() elements each; len is writable.
    let code = unsafe {
        ww2_formation_spots(
            items.as_ptr(),
            items.len(),
            120.0,
            80.0,
            0.7,
            30.0,
            WW2_SHAPE_BLOCK,
            1.25,
            out.as_mut_ptr(),
            out.len(),
            &mut len,
        )
    };
    assert_eq!(code, WW2_OK);
    assert_eq!(len, expected.len());
    for (spot, want) in out.iter().zip(&expected) {
        assert_eq!(
            (spot.id, spot.x.to_bits(), spot.z.to_bits()),
            (want.id, want.x.to_bits(), want.z.to_bits())
        );
    }
}

/// Null pointers, a short output array and an unknown shape return codes and set the message.
#[test]
fn bad_arguments_return_errors() {
    let items = items();
    let mut out = vec![
        ww2_formation_spot {
            id: 0,
            x: 0.0,
            z: 0.0
        };
        items.len()
    ];
    let mut len = 7;
    let call = |items: *const ww2_formation_item,
                count,
                shape,
                out: *mut ww2_formation_spot,
                capacity,
                len: *mut usize| {
        // SAFETY: every non-null pointer below points to at least as many elements as its count says.
        unsafe {
            ww2_formation_spots(
                items, count, 0.0, 0.0, 0.0, 0.0, shape, 1.0, out, capacity, len,
            )
        }
    };
    let n = items.len();
    assert_eq!(
        call(
            items.as_ptr(),
            n,
            WW2_SHAPE_LINE,
            out.as_mut_ptr(),
            n,
            ptr::null_mut()
        ),
        WW2_ERR_NULL_POINTER
    );
    assert_eq!(last_error(), "out_len is null");
    assert_eq!(
        call(
            ptr::null(),
            n,
            WW2_SHAPE_LINE,
            out.as_mut_ptr(),
            n,
            &mut len
        ),
        WW2_ERR_NULL_POINTER
    );
    assert_eq!(len, 0);
    assert_eq!(last_error(), "items is null");
    assert_eq!(
        call(
            items.as_ptr(),
            n,
            WW2_SHAPE_LINE,
            ptr::null_mut(),
            n,
            &mut len
        ),
        WW2_ERR_NULL_POINTER
    );
    assert_eq!(last_error(), "out is null");
    assert_eq!(
        call(
            items.as_ptr(),
            n,
            WW2_SHAPE_LINE,
            out.as_mut_ptr(),
            n - 1,
            &mut len
        ),
        WW2_ERR_CAPACITY
    );
    assert_eq!(
        last_error(),
        "out has room for 4 spots but there are 5 items"
    );
    assert_eq!(
        call(items.as_ptr(), n, 4, out.as_mut_ptr(), n, &mut len),
        WW2_ERR_BAD_ARGUMENT
    );
    assert_eq!(
        last_error(),
        "unknown shape 4: expected a WW2_SHAPE_* from 0 to 3"
    );
    // Nothing to read or write: null pointers with zero lengths are fine.
    len = 7;
    assert_eq!(
        call(
            ptr::null(),
            0,
            WW2_SHAPE_WEDGE,
            ptr::null_mut(),
            0,
            &mut len
        ),
        WW2_OK
    );
    assert_eq!(len, 0);
    // SAFETY: a null out is the case under test; nothing is written through it.
    assert_eq!(
        unsafe { ww2_formation_slot_size(1, 0.5, ptr::null_mut()) },
        WW2_ERR_NULL_POINTER
    );
}

/// The message is copied whole or not at all, and its length is always reported.
#[test]
fn last_error_reports_its_length_first() {
    // SAFETY: a null out is the case under test; it sets the message "out is null".
    assert_eq!(
        unsafe { ww2_formation_slot_size(0, 1.0, ptr::null_mut()) },
        WW2_ERR_NULL_POINTER
    );
    let mut len = 0;
    // SAFETY: capacity 0 with a null buffer only asks for the length; len is writable.
    assert_eq!(
        unsafe { ww2_last_error(ptr::null_mut(), 0, &mut len) },
        WW2_ERR_CAPACITY
    );
    assert_eq!(len, "out is null".len());
    let mut buf = [0; 4];
    // SAFETY: buf has 4 writable bytes; len is writable.
    assert_eq!(
        unsafe { ww2_last_error(buf.as_mut_ptr(), buf.len(), &mut len) },
        WW2_ERR_CAPACITY
    );
    assert_eq!(buf, [0; 4]);
    assert_eq!(last_error(), "out is null");
}
