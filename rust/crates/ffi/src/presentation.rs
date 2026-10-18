//! Additive ABI 2.2 presentation queries. Results use caller-owned JSON buffers.

use super::*;

/// Returns the compiled movement profile or null for {type}.
/// Uses the JSON buffer, error and ownership rules of ww2_authoring_validate_map.
///
/// # Safety
/// Readable input, writable output and count storage must be valid and disjoint as documented
/// for ww2_authoring_validate_map.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ww2_movement_profile(
    json: *const c_char,
    len: usize,
    out: *mut c_char,
    capacity: usize,
    out_len: *mut usize,
) -> i32 {
    // SAFETY: the caller provides disjoint buffers and writable count storage.
    guard(|| unsafe {
        queries::json_query(
            json,
            len,
            out,
            capacity,
            out_len,
            ww2_core::presentation_queries::movement_profile,
        )
    })
}

/// Returns whether the compiled unit {type} is a ground vehicle.
/// Uses the JSON buffer, error and ownership rules of ww2_authoring_validate_map.
///
/// # Safety
/// Readable input, writable output and count storage must be valid and disjoint as documented
/// for ww2_authoring_validate_map.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ww2_is_ground_vehicle(
    json: *const c_char,
    len: usize,
    out: *mut c_char,
    capacity: usize,
    out_len: *mut usize,
) -> i32 {
    // SAFETY: the caller provides disjoint buffers and writable count storage.
    guard(|| unsafe {
        queries::json_query(
            json,
            len,
            out,
            capacity,
            out_len,
            ww2_core::presentation_queries::is_ground_vehicle,
        )
    })
}

/// Returns the compiled body radius in metres for {type}.
/// Uses the JSON buffer, error and ownership rules of ww2_authoring_validate_map.
///
/// # Safety
/// Readable input, writable output and count storage must be valid and disjoint as documented
/// for ww2_authoring_validate_map.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ww2_body_radius(
    json: *const c_char,
    len: usize,
    out: *mut c_char,
    capacity: usize,
    out_len: *mut usize,
) -> i32 {
    // SAFETY: the caller provides disjoint buffers and writable count storage.
    guard(|| unsafe {
        queries::json_query(
            json,
            len,
            out,
            capacity,
            out_len,
            ww2_core::presentation_queries::body_radius,
        )
    })
}

/// Returns projectile position for {flight,time}, in metres and simulation seconds.
/// Uses the JSON buffer, error and ownership rules of ww2_authoring_validate_map.
///
/// # Safety
/// Readable input, writable output and count storage must be valid and disjoint as documented
/// for ww2_authoring_validate_map.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ww2_flight_at(
    json: *const c_char,
    len: usize,
    out: *mut c_char,
    capacity: usize,
    out_len: *mut usize,
) -> i32 {
    // SAFETY: the caller provides disjoint buffers and writable count storage.
    guard(|| unsafe {
        queries::json_query(
            json,
            len,
            out,
            capacity,
            out_len,
            ww2_core::presentation_queries::flight_at,
        )
    })
}

/// Returns projectile velocity for {flight,time}, in metres per second.
/// Uses the JSON buffer, error and ownership rules of ww2_authoring_validate_map.
///
/// # Safety
/// Readable input, writable output and count storage must be valid and disjoint as documented
/// for ww2_authoring_validate_map.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ww2_velocity_at(
    json: *const c_char,
    len: usize,
    out: *mut c_char,
    capacity: usize,
    out_len: *mut usize,
) -> i32 {
    // SAFETY: the caller provides disjoint buffers and writable count storage.
    guard(|| unsafe {
        queries::json_query(
            json,
            len,
            out,
            capacity,
            out_len,
            ww2_core::presentation_queries::velocity_at,
        )
    })
}
