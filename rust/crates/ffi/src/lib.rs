//! The C interface to the ww2-rts rules core (`docs/contracts/rust-unreal-abi.md`). Unreal links it as
//! a static library and includes `include/ww2_core.h`, which cbindgen writes from this file.
//!
//! Every function here follows the same rules:
//! - It returns `int32_t`: `WW2_OK` (0) on success, or a negative `WW2_ERR_*` code. Results go through
//!   out-parameters. `ww2_abi_version`, `ww2_abi_minor_version` and `ww2_rules_hash` return numbers directly.
//! - It runs inside `catch_unwind`. A panic returns `WW2_ERR_PANIC`; nothing unwinds into C or C++.
//! - A null pointer is an error, except where its length is 0, since nothing is read or written then.
//! - After a failure, `ww2_last_error` copies a readable message for the calling thread.
//! - Results go into arrays the caller owns. View handles are released with `ww2_view_free`.
//!
//! This crate holds the interface's `unsafe` code; `ww2-core` has none.

#![allow(non_camel_case_types)] // the C names of the structs

use std::cell::RefCell;
use std::ffi::c_char;
use std::panic::{self, AssertUnwindSafe};

use ww2_core::formation::{self, Item, Shape};

mod view;
pub use view::*;

/// The interface version. It changes whenever a function signature or a struct layout in this header
/// changes. The Unreal plugin refuses to start when `ww2_abi_version()` differs from this.
pub const WW2_ABI_VERSION: u32 = 2;

/// Success.
pub const WW2_OK: i32 = 0;
/// The call panicked inside the library. The panic was caught; the message says where.
pub const WW2_ERR_PANIC: i32 = -1;
/// A pointer argument was null where something had to be read or written.
pub const WW2_ERR_NULL_POINTER: i32 = -2;
/// An argument is out of range, such as an unknown shape.
pub const WW2_ERR_BAD_ARGUMENT: i32 = -3;
/// The caller's buffer is too small.
pub const WW2_ERR_CAPACITY: i32 = -4;
/// The message is not valid UTF-8, JSON, or a valid server message.
pub const WW2_ERR_PARSE: i32 = -5;
/// The message cannot be applied before the view receives a start message.
pub const WW2_ERR_WRONG_STATE: i32 = -6;
/// A previous panic poisoned this handle. Only ww2_view_free is allowed.
pub const WW2_ERR_POISONED: i32 = -7;

/// Formation shapes, in the Formation menu's order (`SHAPES` in `shared/formation.js`). Ranks of up to
/// ten, or as many as fit the drag.
pub const WW2_SHAPE_LINE: i32 = 0;
/// A square, also widened by a drag.
pub const WW2_SHAPE_BLOCK: i32 = 1;
/// Two files deep.
pub const WW2_SHAPE_COLUMN: i32 = 2;
/// An arrowhead, one unit at the tip.
pub const WW2_SHAPE_WEDGE: i32 = 3;

/// A unit to place in a formation.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct ww2_formation_item {
    /// The caller's id for the unit, copied to its spot.
    pub id: u32,
    /// Nonzero: the unit takes the rear ranks (mortars, rockets, medics).
    pub back: u8,
    /// Where the unit stands, in metres (x east, z south).
    pub x: f64,
    pub z: f64,
    /// Its slot width in metres; `ww2_formation_slot_size` gives a unit kind's.
    pub size: f64,
}

/// Where one unit should stand, in metres.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ww2_formation_spot {
    /// The id of the item this spot is for.
    pub id: u32,
    pub x: f64,
    pub z: f64,
}

thread_local! {
    static LAST_ERROR: RefCell<String> = const { RefCell::new(String::new()) };
}

/// A failed call: its status code and the message `ww2_last_error` gives.
struct Failure {
    code: i32,
    message: String,
}

impl Failure {
    fn null(name: &str) -> Failure {
        Failure {
            code: WW2_ERR_NULL_POINTER,
            message: format!("{name} is null"),
        }
    }
}

fn set_last_error(message: String) {
    // try_with: during thread exit the slot may be gone already; the message is then lost, not a panic.
    let _ = LAST_ERROR.try_with(|last| *last.borrow_mut() = message);
}

fn panic_text(payload: &(dyn std::any::Any + Send)) -> &str {
    if let Some(text) = payload.downcast_ref::<&str>() {
        text
    } else if let Some(text) = payload.downcast_ref::<String>() {
        text
    } else {
        "no message"
    }
}

/// Runs an entry point's body: a returned failure or a caught panic becomes a status code and the
/// thread's last error.
fn guard(body: impl FnOnce() -> Result<(), Failure>) -> i32 {
    // AssertUnwindSafe: after a panic the body's own values are dropped, and the bodies below write to
    // the caller's memory only after everything that could panic, so nothing is left half written.
    match panic::catch_unwind(AssertUnwindSafe(body)) {
        Ok(Ok(())) => WW2_OK,
        Ok(Err(failure)) => {
            set_last_error(failure.message);
            failure.code
        }
        Err(payload) => {
            set_last_error(format!("panic: {}", panic_text(&*payload)));
            WW2_ERR_PANIC
        }
    }
}

/// The caller's input array as a slice: empty when `len` is 0, a failure when the pointer is null
/// otherwise.
///
/// # Safety
/// A non-null `ptr` must point to `len` initialized `T`s that stay unchanged during the call.
unsafe fn input<'a, T>(ptr: *const T, len: usize, name: &str) -> Result<&'a [T], Failure> {
    if len > isize::MAX as usize / std::mem::size_of::<T>().max(1) {
        return Err(Failure {
            code: WW2_ERR_BAD_ARGUMENT,
            message: format!("{name} length exceeds the addressable range"),
        });
    }
    if len == 0 {
        return Ok(&[]);
    }
    if ptr.is_null() {
        return Err(Failure::null(name));
    }
    // SAFETY: ptr is not null, and the caller promises len initialized items behind it.
    Ok(unsafe { std::slice::from_raw_parts(ptr, len) })
}

/// The interface version this library was built with (`WW2_ABI_VERSION`).
#[unsafe(no_mangle)]
pub extern "C" fn ww2_abi_version() -> u32 {
    // A constant cannot panic; catch_unwind keeps the rule that every entry point has it.
    panic::catch_unwind(|| WW2_ABI_VERSION).unwrap_or(0)
}

/// FNV-1a 64 of the rules data compiled into this library. Today that is the formation constants of
/// `shared/formation.js` only (the shape names and the slot, rank and spacing numbers); the Rust
/// server is to send the same number in `lobby.rules`, so a client built with other rules can warn.
#[unsafe(no_mangle)]
pub extern "C" fn ww2_rules_hash() -> u64 {
    panic::catch_unwind(|| ww2_core::rules::RULES_HASH).unwrap_or(0)
}

/// Copies the message of the calling thread's last failed call, as UTF-8 without a terminating zero,
/// into `buf`, and its length in bytes into `*out_len`. The message is empty (length 0) when no call
/// on this thread has failed yet. Call with `capacity` 0 to learn the length first.
/// Returns `WW2_ERR_CAPACITY` (copying nothing) when `capacity` is less than the length, and
/// `WW2_ERR_NULL_POINTER` when `out_len` is null, or `buf` is null with a message to copy. It never
/// changes the stored message.
///
/// # Safety
/// `buf` must be null or point to `capacity` writable bytes; `out_len` must be null or point to a
/// writable `size_t`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ww2_last_error(
    buf: *mut c_char,
    capacity: usize,
    out_len: *mut usize,
) -> i32 {
    // Not through guard: reading the message must not replace it.
    let read = || {
        if out_len.is_null() {
            return WW2_ERR_NULL_POINTER;
        }
        let message = LAST_ERROR
            .try_with(|last| last.borrow().clone())
            .unwrap_or_default();
        // SAFETY: out_len is not null, and the caller promises it points to a writable size_t.
        unsafe { out_len.write(message.len()) };
        if message.is_empty() {
            return WW2_OK;
        }
        if capacity < message.len() {
            return WW2_ERR_CAPACITY;
        }
        if buf.is_null() {
            return WW2_ERR_NULL_POINTER;
        }
        // SAFETY: buf is not null and the caller promises capacity writable bytes, at least
        // message.len(); a String the library owns cannot overlap the caller's buffer.
        unsafe { std::ptr::copy_nonoverlapping(message.as_ptr(), buf.cast::<u8>(), message.len()) };
        WW2_OK
    };
    panic::catch_unwind(read).unwrap_or(WW2_ERR_PANIC)
}

/// `slotSize` from `shared/formation.js`: a unit's slot width in metres, its diameter plus 1.5 m, and
/// at least 3 m for infantry (`infantry` nonzero) or 5 m for anything else. Writes it to `*out`.
///
/// # Safety
/// `out` must be null or point to a writable `double`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ww2_formation_slot_size(infantry: u8, radius: f64, out: *mut f64) -> i32 {
    guard(|| {
        if out.is_null() {
            return Err(Failure::null("out"));
        }
        let size = formation::slot_size(infantry != 0, radius);
        // SAFETY: out is not null, and the caller promises it points to a writable double.
        unsafe { out.write(size) };
        Ok(())
    })
}

/// `facingSpots` from `shared/formation.js`: where each of `count` units should stand for a move order.
/// The ranks run across `face` (radians; 0 faces +x, pi/2 faces +z), the first through
/// (`at_x`, `at_z`), later ones behind it. `reach` is the drag's length in metres: 0 for a plain
/// click, and anything not finite counts as 0. `shape` is a `WW2_SHAPE_*`; `spread` scales every gap
/// (1 by default).
///
/// Writes the spots rank by rank from the front into `out`, and their number into `*out_len`: `count`,
/// or 0 when the formation is empty: with no items, and, as in the JavaScript, for a line or block
/// whose average slot times `spread` is NaN, or is 0 while `reach` is 0 or not finite.
/// `out` needs room for `count` spots and is written only on success. On failure `*out_len` is 0.
///
/// Errors: `WW2_ERR_NULL_POINTER` when `out_len` is null, or `items` or `out` is null while `count` is
/// not 0; `WW2_ERR_BAD_ARGUMENT` for an unknown shape; `WW2_ERR_CAPACITY` when `out_capacity` is less
/// than `count`.
///
/// # Safety
/// `items` must be null or point to `count` readable items, `out` must be null or point to
/// `out_capacity` writable spots, and `out_len` must be null or point to a writable `size_t`. The
/// three must not overlap.
#[allow(clippy::too_many_arguments)] // the C signature the contract describes
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ww2_formation_spots(
    items: *const ww2_formation_item,
    count: usize,
    at_x: f64,
    at_z: f64,
    face: f64,
    reach: f64,
    shape: i32,
    spread: f64,
    out: *mut ww2_formation_spot,
    out_capacity: usize,
    out_len: *mut usize,
) -> i32 {
    guard(|| {
        if out_len.is_null() {
            return Err(Failure::null("out_len"));
        }
        // SAFETY: out_len is not null, and the caller promises it points to a writable size_t.
        unsafe { out_len.write(0) };
        let shape = match shape {
            WW2_SHAPE_LINE => Shape::Line,
            WW2_SHAPE_BLOCK => Shape::Block,
            WW2_SHAPE_COLUMN => Shape::Column,
            WW2_SHAPE_WEDGE => Shape::Wedge,
            other => {
                return Err(Failure {
                    code: WW2_ERR_BAD_ARGUMENT,
                    message: format!("unknown shape {other}: expected a WW2_SHAPE_* from 0 to 3"),
                });
            }
        };
        // SAFETY: the caller promises count readable items behind a non-null items.
        let items = unsafe { input(items, count, "items") }?;
        if out_capacity < count {
            return Err(Failure {
                code: WW2_ERR_CAPACITY,
                message: format!(
                    "out has room for {out_capacity} spots but there are {count} items"
                ),
            });
        }
        if out.is_null() && count > 0 {
            return Err(Failure::null("out"));
        }
        let items: Vec<Item> = items
            .iter()
            .map(|u| Item {
                id: u.id,
                x: u.x,
                z: u.z,
                size: u.size,
                back: u.back != 0,
            })
            .collect();
        let spots = formation::facing_spots(&items, at_x, at_z, face, reach, shape, spread);
        // The core gives one spot per item or none, so this never fails; if it did, it would panic
        // here rather than write past the caller's array.
        assert!(spots.len() <= out_capacity, "more spots than items");
        for (i, spot) in spots.iter().enumerate() {
            let spot = ww2_formation_spot {
                id: spot.id,
                x: spot.x,
                z: spot.z,
            };
            // SAFETY: out is not null (count > 0 here, since spots is not empty), i is below
            // spots.len() <= out_capacity, and the caller promises out_capacity writable spots.
            unsafe { out.add(i).write(spot) };
        }
        // SAFETY: as above, out_len is not null and writable.
        unsafe { out_len.write(spots.len()) };
        Ok(())
    })
}

mod queries;
pub use queries::*;

mod presentation;
pub use presentation::*;

#[cfg(test)]
mod tests {
    use super::*;

    /// A panic inside an entry point's body comes back as WW2_ERR_PANIC with a message.
    #[test]
    fn a_panic_becomes_an_error_code() {
        let code = guard(|| panic!("boom"));
        assert_eq!(code, WW2_ERR_PANIC);
        let mut buf = [0 as c_char; 64];
        let mut len = 0;
        // SAFETY: buf has 64 writable bytes and len is a writable usize.
        let read = unsafe { ww2_last_error(buf.as_mut_ptr(), buf.len(), &mut len) };
        assert_eq!(read, WW2_OK);
        let message: Vec<u8> = buf[..len].iter().map(|&c| c as u8).collect();
        assert_eq!(message, b"panic: boom");
    }
}
