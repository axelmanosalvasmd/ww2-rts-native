# EDITOR-PROVIDERS

This stream stacks on #376 and merges #336's placement query. Running World rooms remain deferred. IF-04's C export is separate. No parity status file changes.

## Literal row mapping

- MAP-23: `map_editor_tests::map23_all_33_shipped_maps_survive_http_save_disk_reopen_and_validation`; `map_editor_tests::map23_net12_http_save_preserves_the_complete_literal_field_whitelist`; `map_editor_tests::net12_editor_http_auth_limits_validation_whitelist_and_raw_reload` (inclusive 8 MiB cap).
- BASE-27, ordinary core/server: `match_construction::native_intent::runtime2::base27_recipient_preview_matches_build_reasons_and_accepted_footprint`; `building_placement::tests::base27_recipient_uses_delivered_weather_aircraft_and_remembered_terrain_only`; `building_placement::tests::base27_recipient_nodes_snap_reserve_and_claim_only_delivered_depots`. Existing component `building_placement::tests::base27_*` checks retain rounding, grade, occupancy, nodes, coastal range and reason ordering. The new ordinary test decodes actual full recipient delivery, checks preview versus canonical query and Build, verifies accepted footprints, unchanged resources on refusal, and six subsequent ticks with every-seat snapshots and no fault.
- NET-11: `map_editor_tests::net11_raw_and_public_get_preserve_projection_headers_and_route_boundaries`; `map_editor_tests::net11_public_world_projection_does_not_require_runtime_map_admission`.
- NET-12: `map_editor_tests::net12_editor_http_auth_limits_validation_whitelist_and_raw_reload`; `map_editor_tests::net12_corrupt_overrides_preserve_raw_get_default_catalogue_and_strict_refusal`; `map_editor_tests::net12_world_editor_acceptance_matches_prototype_without_relaxing_runtime`; the MAP-23 whitelist tests above; inherited `shards::tests::net12_front_persists_maps_and_both_workers_refresh_horde_catalogue` and `lobby::save_load_tests::net12_saved_editor_map_restores_exact_state_and_every_seat_through_victory`.
- NET-13: `map_editor_tests::net13_password_file_is_reused_and_override_does_not_expose_or_replace_it`; the NET-11 GET and NET-12 POST authorization tests above. The implementation hashes both values with SHA-256 and compares the fixed-size digests through `subtle::ConstantTimeEq`; the test covers generated twelve-character lowercase hex, disk reuse, trimmed file input, environment precedence, empty-environment fallback, wrong lengths and bytes.

Prototype sources: `server.js:47-63,89-162`; `shared/world-layers.js:68-80`; `shared/sim.js:1139-1164,1744-1751,3277-3313`; `client/main.js:1295-1298`, all at `e4e579a`. The whitelist expectation is literal and independent of the implementation, including nested fields and conditional omissions.

Coordination acknowledgement: issue #5 owner decisions A-P (comment `6217590741`, confirmed in `6218552386`) authorize this Rust editor scope. The prototype pin stays `e4e579a`; running World admission stays closed. Match checkpoints retain #376's version 1 contract and exact floating-point encoding. Hermes' exact-head runtime acknowledgement and hosted checks remain orchestrator gates. No issue comment is posted by this stream.

## Deferred clauses

MAP-23, NET-11 and NET-12 cover editor data, including detached World documents. They do not admit running World rooms or save/load a live World match. BASE-27's World recipient adapter, every-cell sight, territory and rail equivalence remain deferred. Its inherited World component checks are not live recipient proof. NET-13 has no separate World clause. The source-only provider review does not establish Unreal adoption or Hermes' required exact-head runtime acknowledgement.

## Integration record

Merged #336 head `5057a6b50084220ab5f9cb45908486ae7e7f6f62` into #376 head `3767e3e41bad2bcb2ec76bb6c8d8343547677650` at `64e2ac4f56076bc2f8877006eba33482211c1879`. The merge is automatic, with no hand-edited hunks. `git merge-tree --write-tree` returned `b5c54ce7233facae52d519474732e8aae3774351`, identical to the recorded merge tree. Both parent registrations remain present. #336's withdrawn World fort-transfer actuator stays withdrawn. Match-save contract and codec files remain unchanged from #376.

## Validation protocol

The PR body records the producing commit and actual check results. Run the focused library filter `test(base27) | test(map23) | test(net11) | test(net12) | test(net13) | test(map_save)` for `ww2-core` and `ww2-server` with the `ci-test` profile and `-j4`. Check affected public projection regressions with `test(map24) | test(maps::tests)`. Before publication, run `cargo fmt --all --check` and strict workspace all-target Clippy.

All Cargo work uses the shared locked slot protocol, its matching target directory, Rust 1.99.0, a 6 GiB cap and zero swap on the Vostro. The first setup attempt completed formatting but rejected duplicate nextest thread flags before compiling or executing tests; corrected invocations use only `-j4`.
