//! The additive revision keeps the ABI 2 layouts, signatures and declaration order.
use std::{
    ffi::c_char,
    mem::{align_of, offset_of, size_of},
    ptr,
};
use ww2_ffi::*;

type Query = unsafe extern "C" fn(*const c_char, usize, *mut c_char, usize, *mut usize) -> i32;

#[test]
fn additive_revision_preserves_abi2_layouts_and_signatures() {
    assert_eq!(ww2_abi_version(), 2);
    assert_eq!(WW2_ABI_VERSION, 2);
    assert_eq!(ww2_abi_minor_version(), 2);
    assert_eq!(WW2_ABI_MINOR_VERSION, 2);
    assert_eq!(
        (
            size_of::<ww2_formation_item>(),
            align_of::<ww2_formation_item>()
        ),
        (32, 8)
    );
    assert_eq!(
        [
            offset_of!(ww2_formation_item, id),
            offset_of!(ww2_formation_item, back),
            offset_of!(ww2_formation_item, x),
            offset_of!(ww2_formation_item, z),
            offset_of!(ww2_formation_item, size)
        ],
        [0, 4, 8, 16, 24]
    );
    assert_eq!(
        (
            size_of::<ww2_formation_spot>(),
            align_of::<ww2_formation_spot>()
        ),
        (24, 8)
    );
    assert_eq!(
        [
            offset_of!(ww2_formation_spot, id),
            offset_of!(ww2_formation_spot, x),
            offset_of!(ww2_formation_spot, z)
        ],
        [0, 8, 16]
    );
    assert_eq!(
        (size_of::<ww2_view_unit>(), align_of::<ww2_view_unit>()),
        (152, 8)
    );
    assert_eq!(
        [
            offset_of!(ww2_view_unit, id),
            offset_of!(ww2_view_unit, owner),
            offset_of!(ww2_view_unit, kind),
            offset_of!(ww2_view_unit, x),
            offset_of!(ww2_view_unit, z),
            offset_of!(ww2_view_unit, rot),
            offset_of!(ww2_view_unit, aim),
            offset_of!(ww2_view_unit, hp),
            offset_of!(ww2_view_unit, supp),
            offset_of!(ww2_view_unit, cd),
            offset_of!(ww2_view_unit, built),
            offset_of!(ww2_view_unit, target),
            offset_of!(ww2_view_unit, cover),
            offset_of!(ww2_view_unit, flags),
            offset_of!(ww2_view_unit, stars),
        ],
        [
            0, 4, 8, 72, 80, 88, 96, 104, 112, 120, 128, 136, 140, 144, 148
        ]
    );
    let _: extern "C" fn() -> u32 = ww2_abi_version;
    let _: extern "C" fn() -> u64 = ww2_rules_hash;
    let _: unsafe extern "C" fn(*mut c_char, usize, *mut usize) -> i32 = ww2_last_error;
    let _: unsafe extern "C" fn(u8, f64, *mut f64) -> i32 = ww2_formation_slot_size;
    let _: unsafe extern "C" fn(
        *const ww2_formation_item,
        usize,
        f64,
        f64,
        f64,
        f64,
        i32,
        f64,
        *mut ww2_formation_spot,
        usize,
        *mut usize,
    ) -> i32 = ww2_formation_spots;
    let _: unsafe extern "C" fn(*mut *mut ww2_view) -> i32 = ww2_view_new;
    let _: unsafe extern "C" fn(*mut ww2_view) -> i32 = ww2_view_free;
    let _: unsafe extern "C" fn(*mut ww2_view, *const c_char, usize) -> i32 = ww2_view_apply;
    let _: unsafe extern "C" fn(*mut ww2_view, *mut usize) -> i32 = ww2_view_unit_count;
    let _: unsafe extern "C" fn(*mut ww2_view, *mut ww2_view_unit, usize, *mut usize) -> i32 =
        ww2_view_units;
    let _: unsafe extern "C" fn(*mut ww2_view, *mut u8) -> i32 = ww2_view_needs_resync;
    let header = include_str!("../include/ww2_core.h");
    let names = [
        "ww2_abi_version(",
        "ww2_rules_hash(",
        "ww2_last_error(",
        "ww2_formation_slot_size(",
        "ww2_formation_spots(",
        "ww2_view_new(",
        "ww2_view_free(",
        "ww2_view_apply(",
        "ww2_view_unit_count(",
        "ww2_view_units(",
        "ww2_view_needs_resync(",
        "ww2_view_placement_check(",
        "ww2_abi_minor_version(",
        "ww2_authoring_validate_map(",
        "ww2_authoring_migrate_scenario(",
        "ww2_authoring_migrate_map(",
        "ww2_authoring_spawns_for(",
        "ww2_authoring_create_preview(",
        "ww2_authoring_find_path(",
        "ww2_authoring_house_kinds(",
        "ww2_authoring_play_match(",
    ];
    let positions: Vec<_> = names
        .iter()
        .map(|name| header.find(name).expect(name))
        .collect();
    assert!(
        positions.windows(2).all(|p| p[0] < p[1]),
        "ABI 2 declarations reordered"
    );
}

#[test]
fn queries_preserve_output_on_errors_and_report_required_capacity() {
    let calls: [Query; 13] = [
        ww2_movement_profile,
        ww2_is_ground_vehicle,
        ww2_body_radius,
        ww2_flight_at,
        ww2_velocity_at,
        ww2_authoring_validate_map,
        ww2_authoring_migrate_scenario,
        ww2_authoring_migrate_map,
        ww2_authoring_spawns_for,
        ww2_authoring_create_preview,
        ww2_authoring_find_path,
        ww2_authoring_house_kinds,
        ww2_authoring_play_match,
    ];
    for call in calls {
        let mut bytes = [0x5a_u8; 64];
        let mut len = 99;
        // SAFETY: all non-null pointers have the declared capacity and are disjoint.
        unsafe {
            assert_eq!(
                call(ptr::null(), 1, bytes.as_mut_ptr().cast(), 64, &mut len),
                WW2_ERR_NULL_POINTER
            );
            assert_eq!(len, 0);
            assert_eq!(bytes, [0x5a; 64]);
            assert_eq!(
                call(
                    b"{".as_ptr().cast(),
                    1,
                    bytes.as_mut_ptr().cast(),
                    64,
                    &mut len
                ),
                WW2_ERR_PARSE
            );
            assert_eq!(len, 0);
            assert_eq!(bytes, [0x5a; 64]);
            assert_eq!(
                call(
                    b"{}".as_ptr().cast(),
                    2,
                    bytes.as_mut_ptr().cast(),
                    64,
                    ptr::null_mut()
                ),
                WW2_ERR_NULL_POINTER
            );
            assert_eq!(
                call(
                    ptr::null(),
                    16 * 1024 * 1024 + 1,
                    bytes.as_mut_ptr().cast(),
                    64,
                    &mut len
                ),
                WW2_ERR_BAD_ARGUMENT
            );
            assert_eq!(
                call(
                    b"{}".as_ptr().cast(),
                    2,
                    ptr::null_mut(),
                    usize::MAX,
                    &mut len
                ),
                WW2_ERR_BAD_ARGUMENT
            );
        }
    }
    let mut len = 0;
    // SAFETY: readable null JSON literal, writable count and explicitly empty output.
    assert_eq!(
        unsafe {
            ww2_authoring_validate_map(b"null".as_ptr().cast(), 4, ptr::null_mut(), 0, &mut len)
        },
        WW2_ERR_CAPACITY
    );
    assert!(len > 0);
    let mut bytes = vec![0x5a_u8; len];
    let mut next = 0;
    // SAFETY: sized writable output, disjoint readable input and count.
    unsafe {
        assert_eq!(
            ww2_authoring_validate_map(
                b"null".as_ptr().cast(),
                4,
                bytes.as_mut_ptr().cast(),
                len - 1,
                &mut next
            ),
            WW2_ERR_CAPACITY
        );
        assert_eq!(next, len);
        assert!(bytes.iter().all(|&b| b == 0x5a));
        assert_eq!(
            ww2_authoring_validate_map(b"null".as_ptr().cast(), 4, ptr::null_mut(), len, &mut next),
            WW2_ERR_NULL_POINTER
        );
        assert_eq!(
            ww2_authoring_validate_map(
                b"null".as_ptr().cast(),
                4,
                bytes.as_mut_ptr().cast(),
                len,
                &mut next
            ),
            WW2_OK
        );
    }
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&bytes).unwrap(),
        "not a map"
    );
}
