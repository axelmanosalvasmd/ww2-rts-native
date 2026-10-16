# ABI-QUERIES stream

Baseline: main `1c06c8f4cc9b25d6121de993c486d4f3f49da1ce` plus PR #389 at `2d5cd3cdb8868e0847fe80c9334ab87d0be127a1`. The #389 merge was automatic with no conflict or hand-edited hunk. Main then advanced through #382 to `b113bb95`; its tutorial tests were merged automatically with no conflicts or hand-edited hunks. This stream stacks on #389. Reference remains `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`; no prototype file or pin changes.

Acknowledged owner message `julio-373-merged-decisions-20261016` ([6224738109](https://github.com/axelmanosalvasmd/ww2-rts-native/issues/5#issuecomment-6224738109)): implement IF-04/IF-12 as additive C functions, retain existing functions and layouts, document exact signatures for runtime review before Unreal adoption. Runtime exact-head acknowledgment remains pending. No issue comment is posted from this stream, following its brief.

## Literal test mapping

IF-04, ordinary recipient placement:

- `building_shape_snaps_and_omits_undefined_properties`
- `blocked_kind_logistics_and_nonfinite_shape_have_no_footprint`
- `forts_preserve_plan_order_nest_letters_and_concrete_nulls`
- `fort_visibility_precedes_blockage_and_partial_plans_keep_order`
- `only_delivered_allied_observers_supply_visibility`
- `unit_and_terrain_deltas_change_placement_without_hidden_state`
- `depot_returns_the_selected_node_object_and_reserves_nodes_for_other_buildings`
- `coastal_and_logistics_refusals_keep_prototype_precedence`
- `fill_demine_and_bank_supported_bridges_use_received_terrain`
- `recipient_preview_preserves_browser_weather_plane_and_mine_mismatches`
- `delivered_smoke_and_unfinished_lookouts_bound_visibility`
- `world_refusal_is_explicit_and_does_not_poison_the_recipient`

These tests call `ww2_view_new`, `ww2_view_apply`, `ww2_view_placement_check` and `ww2_view_free`. Source is `client/availability.js:169-198` and `shared/sim.js:3261-3305`. The query uses the browser recipient adapter's known information, including its weather, aircraft and mine limitations. World initial-terrain/rail discovery history remains unavailable. Team-qualified demine on a delivered mine returns an explicit error; the entrenchment/work callers omit team, while direct `footAt` passes team and throws in the prototype for this case. No authoritative hidden-state placement is used as a witness.

IF-12, bounded authoring exports:

- `validation_and_mode_filter_preserve_editor_meaning`
- `validation_uses_source_order_and_static_house_metadata_needs_no_map`
- `migrations_retain_private_authored_metadata_and_are_idempotent`
- `preview_and_path_use_the_authored_map`
- `authored_path_keeps_the_source_clearance_around_houses`
- `every_committed_map_validates_through_the_c_entry_point`
- `spawn_filter_is_pure_and_does_not_require_runtime_map_admission`
- `connected_houses_return_group_and_cell_labels`
- `fairness_zero_tick_timeout_is_seeded_and_reports_actual_starts`
- `fairness_runs_eight_ticks_and_replays_the_same_native_outcome`
- `fairness_crosses_both_commander_beats_with_complete_recipient_delivery`
- `rejected_modes_and_insufficient_buffers_never_publish_an_answer`

These tests call every new authoring C function. Follow-up ABI-QUERIES-2 replaces the bounded opening policy with the native human commander and adds completed C-entry fairness and preview checks. Its mapping, dependencies, remaining IF-12 limits and integration record are in [abi-queries2.md](abi-queries2.md). Full IF-12 remains held for unsupported non-World startup and active match contexts. This PR does not promote a partial authoring witness to a completed row.

ABI boundary checks:

- `additive_revision_preserves_abi2_layouts_and_signatures`: major 2, minor 1, every old field offset and function type, legacy header declaration order.
- `queries_preserve_output_on_errors_and_report_required_capacity`: all authoring exports, invalid/null/oversized inputs, required sizes and unchanged output sentinels.
- Existing `committed_header_is_current`, formation ABI/error tests and view ABI/error tests.
- Existing C replay program now links every additive export and exercises JSON buffer errors, capacity and empty-view refusal.

## Execution

A direct Vostro Node v26.11.1/x64 check against the unchanged reference passed 40 pure-query expectations, including all 33 shipped maps. A separate house-clearance path expectation was read directly from the pinned pathfinder and added to the C-entry test. These checks generated no recorder, fixture or provenance bundle. All 36 FFI tests passed before the final Clippy style correction. The PR records final committed-head qualification. Cargo runs only on the Vostro, from `rust/`, with the pinned Rust 1.99 toolchain, a locked S1/S2/S3 target directory, four build jobs, 6 GiB memory and zero swap. No full workspace test run is requested. Focused FFI tests, committed header regeneration/check, formatting, strict workspace Clippy and the existing Linux C-link replay are required before publication.

Source review confirmed the caller-owned buffer/panic boundaries and the deliberate browser placement adapter limits. It found a missing authored-map wall-neighbor mask; the provider now computes the same mask before pathfinding. This is source review, separate from execution and Hermes' final-head runtime acknowledgment.
