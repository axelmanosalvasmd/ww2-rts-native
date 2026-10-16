//! Opaque recipient-view handles. The core owns all decoding and state updates.

use super::*;
use ww2_core::client_view::{ClientView, UnitRow, ViewError};

/// An opaque recipient view. Create it with ww2_view_new and release it exactly once with
/// ww2_view_free. Use one thread at a time for each handle; different handles are independent.
/// Never copy, inspect, or allocate this type in C. A panic poisons it; only free is then allowed.
pub struct ww2_view {
    state: ClientView,
    poisoned: bool,
}

/// A complete fixed-column unit row, sorted by id in ww2_view_units. Values use the server's
/// simulation units: metres (x east, z south), radians, and seconds. Flag bits are copied unchanged.
/// Wire tail fields are not exposed in ABI 2. This struct's layout is versioned by WW2_ABI_VERSION.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ww2_view_unit {
    pub id: u32,
    pub owner: i32,
    /// UTF-8 wire type name, terminated with zero and zero padded. At most 63 bytes plus terminator.
    /// A longer name or embedded zero makes ww2_view_units fail, without writing any rows.
    pub kind: [c_char; 64],
    pub x: f64,
    pub z: f64,
    pub rot: f64,
    pub aim: f64,
    pub hp: f64,
    pub supp: f64,
    pub cd: f64,
    pub built: f64,
    /// Target unit id, or zero when there is no target.
    pub target: u32,
    pub cover: u32,
    pub flags: u32,
    pub stars: u32,
}

fn row(unit: &UnitRow) -> Result<ww2_view_unit, Failure> {
    let bytes = unit.kind.as_bytes();
    if bytes.len() >= 64 || bytes.contains(&0) {
        return Err(Failure {
            code: WW2_ERR_BAD_ARGUMENT,
            message: format!(
                "unit {} type must fit in 63 UTF-8 bytes without zero bytes",
                unit.id
            ),
        });
    }
    let mut kind = [0; 64];
    for (out, byte) in kind.iter_mut().zip(bytes) {
        *out = *byte as c_char;
    }
    Ok(ww2_view_unit {
        id: unit.id,
        owner: unit.owner,
        kind,
        x: unit.x,
        z: unit.z,
        rot: unit.rot,
        aim: unit.aim,
        hp: unit.hp,
        supp: unit.supp,
        cd: unit.cd,
        built: unit.built,
        target: unit.target,
        cover: unit.cover,
        flags: unit.flags,
        stars: unit.stars,
    })
}

/// # Safety
/// view must be null or a live handle owned exclusively by this call.
unsafe fn view_guard(
    view: *mut ww2_view,
    body: impl FnOnce(&mut ClientView) -> Result<(), Failure>,
) -> i32 {
    guard(|| {
        if view.is_null() {
            return Err(Failure::null("view"));
        }
        // SAFETY: the caller promises a live handle and exclusive access for this call.
        let handle = unsafe { &mut *view };
        if handle.poisoned {
            return Err(Failure {
                code: WW2_ERR_POISONED,
                message: "view is poisoned; only ww2_view_free is allowed".into(),
            });
        }
        match panic::catch_unwind(AssertUnwindSafe(|| body(&mut handle.state))) {
            Ok(result) => result,
            Err(payload) => {
                handle.poisoned = true;
                panic::resume_unwind(payload)
            }
        }
    })
}

/// Creates an empty recipient view and writes its handle to *out. On failure *out is null.
/// The view contains no match until a valid start message arrives. Free it exactly once.
///
/// # Safety
/// out must be null or point to writable pointer storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ww2_view_new(out: *mut *mut ww2_view) -> i32 {
    guard(|| {
        if out.is_null() {
            return Err(Failure::null("out"));
        }
        // SAFETY: out is non-null writable storage, as promised by the caller.
        unsafe { out.write(std::ptr::null_mut()) };
        let handle = Box::new(ww2_view {
            state: ClientView::new(),
            poisoned: false,
        });
        // SAFETY: as above; ownership now belongs to the caller.
        unsafe { out.write(Box::into_raw(handle)) };
        Ok(())
    })
}

/// Releases a view, including a poisoned view. A null pointer succeeds. The pointer is invalid
/// afterward and must never be used or freed again.
///
/// # Safety
/// view must be null or a live handle returned by ww2_view_new, with no concurrent users.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ww2_view_free(view: *mut ww2_view) -> i32 {
    guard(|| {
        if !view.is_null() {
            // SAFETY: the caller transfers the one live allocation back to Rust.
            unsafe { drop(Box::from_raw(view)) };
        }
        Ok(())
    })
}

/// Applies one UTF-8 JSON server message (not a JSONL batch). len counts bytes, without a required
/// trailing zero. Unknown message types are ignored. Start replaces the view; snapshots before
/// start return WW2_ERR_WRONG_STATE. Invalid UTF-8, JSON or message shape returns WW2_ERR_PARSE
/// without changing the view. Messages over 16 MiB return WW2_ERR_BAD_ARGUMENT before any read.
/// A held-set mismatch
/// succeeds and sets needs_resync; the caller sends resync and feeds the resulting full snapshot.
///
/// # Safety
/// view must be null or a live exclusively owned handle. json must be null with len zero or point
/// to len readable bytes, which must not overlap view. A zero-byte message fails parsing.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ww2_view_apply(
    view: *mut ww2_view,
    json: *const c_char,
    len: usize,
) -> i32 {
    // SAFETY: the caller promises the handle rules stated above.
    unsafe {
        view_guard(view, |state| {
            if len > ww2_core::client_view::MAX_MESSAGE_BYTES {
                return Err(Failure {
                    code: WW2_ERR_BAD_ARGUMENT,
                    message: "message exceeds 16 MiB".into(),
                });
            }
            // SAFETY: the caller promises len readable bytes; input validates null and size bounds.
            let bytes = input(json.cast::<u8>(), len, "json")?;
            let text = std::str::from_utf8(bytes).map_err(|e| Failure {
                code: WW2_ERR_PARSE,
                message: format!("invalid UTF-8: {e}"),
            })?;
            state.apply(text).map(|_| ()).map_err(view_error)
        })
    }
}

/// Writes the number of current unit rows to *out. An empty view has zero units.
///
/// # Safety
/// view must be null or a live exclusively owned handle; out must be null or writable size_t
/// storage outside the handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ww2_view_unit_count(view: *mut ww2_view, out: *mut usize) -> i32 {
    // SAFETY: the caller promises the handle and output rules above.
    unsafe {
        view_guard(view, |state| {
            if out.is_null() {
                return Err(Failure::null("out"));
            }
            out.write(state.unit_count());
            Ok(())
        })
    }
}

/// Copies all fixed-column unit rows in ascending id order into caller-owned out storage. Writes
/// the required row count to *out_len, including on WW2_ERR_CAPACITY. capacity counts rows, not
/// bytes. Call with null out and capacity zero to query the count (CAPACITY for a nonempty view).
/// Any failure leaves the row array unchanged. Unrepresentable type names return BAD_ARGUMENT.
/// For other failures, out_len is unchanged unless the row count has already been reported.
///
/// # Safety
/// view must be null or a live exclusively owned handle; out must be null or point to capacity
/// writable rows; out_len must be null or writable size_t storage. These must not overlap.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ww2_view_units(
    view: *mut ww2_view,
    out: *mut ww2_view_unit,
    capacity: usize,
    out_len: *mut usize,
) -> i32 {
    // SAFETY: the caller promises the handle and output rules above.
    unsafe {
        view_guard(view, |state| {
            if out_len.is_null() {
                return Err(Failure::null("out_len"));
            }
            if capacity > isize::MAX as usize / std::mem::size_of::<ww2_view_unit>() {
                return Err(Failure {
                    code: WW2_ERR_BAD_ARGUMENT,
                    message: "unit capacity exceeds the addressable range".into(),
                });
            }
            let count = state.unit_count();
            out_len.write(count);
            if capacity < count {
                return Err(Failure {
                    code: WW2_ERR_CAPACITY,
                    message: format!(
                        "out has room for {capacity} units but the view holds {count}"
                    ),
                });
            }
            if count > 0 && out.is_null() {
                return Err(Failure::null("out"));
            }
            let rows: Vec<_> = state.units().map(row).collect::<Result<_, _>>()?;
            if !rows.is_empty() {
                std::ptr::copy_nonoverlapping(rows.as_ptr(), out, rows.len());
            }
            Ok(())
        })
    }
}

/// Writes 1 when the held unit count or id XOR differs from the server, otherwise 0. The caller
/// handles resync requests and throttling; the view performs no I/O. A matching full snapshot
/// clears the flag. An empty view does not need resync.
///
/// # Safety
/// view must be null or a live exclusively owned handle; out must be null or writable uint8_t
/// storage outside the handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ww2_view_needs_resync(view: *mut ww2_view, out: *mut u8) -> i32 {
    // SAFETY: the caller promises the handle and output rules above.
    unsafe {
        view_guard(view, |state| {
            if out.is_null() {
                return Err(Failure::null("out"));
            }
            out.write(u8::from(state.needs_resync()));
            Ok(())
        })
    }
}

fn view_error(error: ViewError) -> Failure {
    let code = match &error {
        ViewError::Parse(_) | ViewError::Invalid(_) => WW2_ERR_PARSE,
        ViewError::WrongState(_) => WW2_ERR_WRONG_STATE,
        ViewError::Limit(_) => WW2_ERR_BAD_ARGUMENT,
    };
    Failure {
        code,
        message: error.to_string(),
    }
}

/// Checks placement using only this recipient's delivered map, cells, units and snapshot fields.
/// Request JSON: {kind,x,z,dir,team}. Output follows placementCheck: {ok,reason?,cells?,x,z,c?,node?}.
/// Uses the JSON buffer rules of ww2_authoring_validate_map. No view state changes on success or
/// ordinary failure. A panic poisons the handle, as with other view calls.
///
/// # Safety
/// view must be null or a live exclusively owned handle. Input, output and count storage must be
/// valid and disjoint from each other and the handle, as for ww2_authoring_validate_map.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ww2_view_placement_check(
    view: *mut ww2_view,
    json: *const c_char,
    len: usize,
    out: *mut c_char,
    capacity: usize,
    out_len: *mut usize,
) -> i32 {
    // SAFETY: the caller promises the handle and buffer requirements documented above.
    unsafe {
        view_guard(view, |state| {
            if state.start().is_none() {
                return Err(Failure {
                    code: WW2_ERR_WRONG_STATE,
                    message: "placement requires a recipient start".into(),
                });
            }
            super::queries::json_query(json, len, out, capacity, out_len, |request| {
                ww2_core::recipient_placement::placement_check(state, request)
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panic_poisons_all_view_operations_but_free() {
        let mut view = std::ptr::null_mut();
        // SAFETY: the handle is allocated here, exclusively used, and freed exactly once.
        unsafe {
            assert_eq!(ww2_view_new(&mut view), WW2_OK);
            assert_eq!(
                view_guard(view, |_| panic!("injected view panic")),
                WW2_ERR_PANIC
            );
            let mut count = 42;
            let mut resync = 42;
            assert_eq!(ww2_view_apply(view, std::ptr::null(), 0), WW2_ERR_POISONED);
            assert_eq!(ww2_view_unit_count(view, &mut count), WW2_ERR_POISONED);
            assert_eq!(
                ww2_view_units(view, std::ptr::null_mut(), 0, &mut count),
                WW2_ERR_POISONED
            );
            assert_eq!(ww2_view_needs_resync(view, &mut resync), WW2_ERR_POISONED);
            assert_eq!((count, resync), (42, 42));
            assert_eq!(ww2_view_free(view), WW2_OK);
        }
    }
}
