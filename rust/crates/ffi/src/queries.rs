//! Additive ABI 2 queries. JSON outputs always belong to the caller.

use super::*;
use serde_json::Value;

/// Additive revision of ABI 2. Existing signatures and layouts retain version 2.
pub const WW2_ABI_MINOR_VERSION: u32 = 2;

/// Additive revision supported by this library. Check ww2_abi_version first.
#[unsafe(no_mangle)]
pub extern "C" fn ww2_abi_minor_version() -> u32 {
    panic::catch_unwind(|| WW2_ABI_MINOR_VERSION).unwrap_or(0)
}

fn bad(message: impl Into<String>) -> Failure {
    Failure {
        code: WW2_ERR_BAD_ARGUMENT,
        message: message.into(),
    }
}

/// # Safety
/// Input and output buffers obey the exported query contracts and do not overlap.
pub(crate) unsafe fn json_query(
    json: *const c_char,
    len: usize,
    out: *mut c_char,
    capacity: usize,
    out_len: *mut usize,
    query: impl FnOnce(&Value) -> Result<Value, String>,
) -> Result<(), Failure> {
    if out_len.is_null() {
        return Err(Failure::null("out_len"));
    }
    // SAFETY: the caller promises writable count storage, separate from the buffers.
    unsafe { out_len.write(0) };
    if len > ww2_core::map::MAX_MAP_BYTES {
        return Err(bad("query input exceeds 16 MiB"));
    }
    if capacity > isize::MAX as usize {
        return Err(bad("output capacity exceeds the addressable range"));
    }
    // SAFETY: input checks null/length; the caller promises readable bytes.
    let bytes = unsafe { input(json.cast::<u8>(), len, "json") }?;
    let value: Value = serde_json::from_slice(bytes).map_err(|e| Failure {
        code: WW2_ERR_PARSE,
        message: format!("invalid query JSON: {e}"),
    })?;
    let result = query(&value).map_err(bad)?;
    let bytes = serde_json::to_vec(&result).map_err(|e| bad(e.to_string()))?;
    // SAFETY: as above, out_len is writable. No output bytes change until every check passes.
    unsafe { out_len.write(bytes.len()) };
    if capacity < bytes.len() {
        return Err(Failure {
            code: WW2_ERR_CAPACITY,
            message: format!("query needs {} output bytes", bytes.len()),
        });
    }
    if out.is_null() && !bytes.is_empty() {
        return Err(Failure::null("out"));
    }
    // SAFETY: sufficient caller capacity, non-null output and no overlap with this owned Vec.
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), out.cast::<u8>(), bytes.len()) };
    Ok(())
}

/// Validates a raw map. JSON result is null on success or the map diagnostic string.
/// Shared query buffers: UTF-8 JSON input, at most 16 MiB, no terminating zero required.
/// Output is UTF-8 JSON without a terminating zero. out_len receives the required byte count.
/// Insufficient capacity returns WW2_ERR_CAPACITY and writes no output bytes. Query size with
/// null out and capacity zero. Malformed JSON returns WW2_ERR_PARSE; bad query arguments return
/// WW2_ERR_BAD_ARGUMENT. No query allocates memory that the caller must free.
///
/// # Safety
/// json must be null with len zero or point to len readable bytes. out must be null with capacity
/// zero or point to capacity writable bytes. out_len must be writable size_t storage. All input,
/// output and count storage must be disjoint and remain valid throughout the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ww2_authoring_validate_map(
    json: *const c_char,
    len: usize,
    out: *mut c_char,
    capacity: usize,
    out_len: *mut usize,
) -> i32 {
    guard(|| unsafe {
        json_query(json, len, out, capacity, out_len, |v| {
            Ok(ww2_core::authoring_queries::validate_map(v))
        })
    })
}

/// Migrates the raw map's scenario or legacy triggers. Returns a scenario or JSON null.
/// Uses the JSON buffer, error and ownership rules of ww2_authoring_validate_map.
///
/// # Safety
/// Readable input, writable output and count storage must be valid and disjoint as documented
/// for ww2_authoring_validate_map.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ww2_authoring_migrate_scenario(
    json: *const c_char,
    len: usize,
    out: *mut c_char,
    capacity: usize,
    out_len: *mut usize,
) -> i32 {
    guard(|| unsafe {
        json_query(
            json,
            len,
            out,
            capacity,
            out_len,
            ww2_core::authoring_queries::migrate_scenario,
        )
    })
}

/// Migrates raw map layers to worldVersion 2, preserving authoring fields.
/// Uses the JSON buffer, error and ownership rules of ww2_authoring_validate_map.
///
/// # Safety
/// Readable input, writable output and count storage must be valid and disjoint as documented
/// for ww2_authoring_validate_map.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ww2_authoring_migrate_map(
    json: *const c_char,
    len: usize,
    out: *mut c_char,
    capacity: usize,
    out_len: *mut usize,
) -> i32 {
    guard(|| unsafe {
        json_query(
            json,
            len,
            out,
            capacity,
            out_len,
            ww2_core::authoring_queries::migrate_map,
        )
    })
}

/// Returns usable spawn indices for {map,mode}.
/// Uses the JSON buffer, error and ownership rules of ww2_authoring_validate_map.
///
/// # Safety
/// Readable input, writable output and count storage must be valid and disjoint as documented
/// for ww2_authoring_validate_map.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ww2_authoring_spawns_for(
    json: *const c_char,
    len: usize,
    out: *mut c_char,
    capacity: usize,
    out_len: *mut usize,
) -> i32 {
    guard(|| unsafe {
        json_query(
            json,
            len,
            out,
            capacity,
            out_len,
            ww2_core::authoring_queries::spawns_for,
        )
    })
}

/// Creates a detached setup view for {map,options,seed}. Running World admission is refused.
/// Uses the JSON buffer, error and ownership rules of ww2_authoring_validate_map.
///
/// # Safety
/// Readable input, writable output and count storage must be valid and disjoint as documented
/// for ww2_authoring_validate_map.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ww2_authoring_create_preview(
    json: *const c_char,
    len: usize,
    out: *mut c_char,
    capacity: usize,
    out_len: *mut usize,
) -> i32 {
    guard(|| unsafe {
        json_query(
            json,
            len,
            out,
            capacity,
            out_len,
            ww2_core::authoring_queries::create_preview,
        )
    })
}

/// Returns infantry waypoints for {map,from:{x,z},to:{x,z}} in simulation metres.
/// Uses the JSON buffer, error and ownership rules of ww2_authoring_validate_map.
///
/// # Safety
/// Readable input, writable output and count storage must be valid and disjoint as documented
/// for ww2_authoring_validate_map.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ww2_authoring_find_path(
    json: *const c_char,
    len: usize,
    out: *mut c_char,
    capacity: usize,
    out_len: *mut usize,
) -> i32 {
    guard(|| unsafe {
        json_query(
            json,
            len,
            out,
            capacity,
            out_len,
            ww2_core::authoring_queries::find_path,
        )
    })
}

/// Returns deterministic house labels and metadata from the raw authored map.
/// Uses the JSON buffer, error and ownership rules of ww2_authoring_validate_map.
///
/// # Safety
/// Readable input, writable output and count storage must be valid and disjoint as documented
/// for ww2_authoring_validate_map.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ww2_authoring_house_kinds(
    json: *const c_char,
    len: usize,
    out: *mut c_char,
    capacity: usize,
    out_len: *mut usize,
) -> i32 {
    guard(|| unsafe {
        json_query(
            json,
            len,
            out,
            capacity,
            out_len,
            ww2_core::authoring_queries::house_kinds,
        )
    })
}

/// Runs a bounded seeded AI match for {map,options,seed,max_ticks}; returns winner, ticks and player_vp.
/// Uses the JSON buffer, error and ownership rules of ww2_authoring_validate_map.
///
/// # Safety
/// Readable input, writable output and count storage must be valid and disjoint as documented
/// for ww2_authoring_validate_map.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ww2_authoring_play_match(
    json: *const c_char,
    len: usize,
    out: *mut c_char,
    capacity: usize,
    out_len: *mut usize,
) -> i32 {
    guard(|| unsafe {
        json_query(
            json,
            len,
            out,
            capacity,
            out_len,
            ww2_core::authoring_queries::play_match,
        )
    })
}
