# Interface batch 4 qualification

This branch continues checkpoint `4a849a4b` and merges main at `6db9cabb`. The assigned rows are IF-22, IF-08 and IF-03. Their server/core checks address the gaps recorded in PR #321. Parity tables stay unchanged; ABI exports and Unreal adoption remain separate.

## Evidence

IF-22 preserves hospital placement order with `OrderedAid`. Its keyed membership and insertion sequence are cloned together by World transactions. Recovery and logistics use the ordered iterator; the remaining values-only consumer tests whether a hospital exists, so iteration order cannot change that result.

The collection checks are Rust tests of the prototype's traversal intent:

- `if22_hospital_placement_order_survives_transactions_recovery_ticks_and_snapshots`
- `if22_fire_spread_uses_insertion_order_and_defers_new_fires_until_next_pass`
- `if22_flood_work_keeps_first_insertion_order_without_duplicate_neighbors`
- `if22_world_training_visits_newborn_in_the_same_live_unit_pass`
- `if22_structure_contacts_deduplicate_and_evict_the_oldest_inserted_key`
- `if22_navigation_edges_reinsert_last_and_route_cache_evicts_oldest_inserted`

The hospital check exercises owner replacement, removal/reinsertion, transaction publication and reinforcement, then ordinary ticks through a one-VP match finish and three subsequent ticks, with complete recipient snapshots for every seat and no fault. The training check submits an existing World Buy command and proves that the newborn reaches the same live ascending-ID unit pass. It does not claim World recipient delivery.

IF-08 compares all named helpers against exact pinned JavaScript values across all 33 received authored maps. Focused fixtures add house seed order and named overrides, layer material precedence, height characters, cell composition, rail yards, known-region boundaries and border segment order:

- `all_33_received_maps_match_pinned_presentation_helpers`
- `discovered_house_seeds_and_named_overrides_match_pinned_helpers`
- `layer_material_precedence_height_codec_and_composition_match_pinned_helpers`
- `known_region_boundaries_and_border_segment_order_match_pinned_helpers`

Existing weather fixtures cover every weather description and effect. Authoritative-map and World mutation fixtures also check the shared helper refactors.

IF-03 reads only the reconstructed recipient `ClientView`. Its exact fixture contains 1,000 synthetic snapshot/action pairs, 116 denial-code queries and 174 focused queries. These are pure query inputs, not 1,000 recorded match snapshots. Focused cases cover production prerequisites, concrete tiers, research/lab/resource precedence, queue and population thresholds, all active ability types and cooldown labels. The main exact checks are `if03_one_thousand_pure_snapshot_queries_and_every_deny_code_match_prototype` and `if03_focused_denial_precedence_and_threshold_queries_match_prototype`; six additional recipient tests cover selection, delivered-state changes and denial context. Existing economy fixtures cover the underlying price, population, support, builder and research helpers.

Both fixture generators were byte-checked with Node v26.11.1/x64 against `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`. The Mac checkout's reference directory is uninitialized, so execution used a separate pinned worktree under `/tmp` and temporary copies of the generators with only their input/output paths relocated. The tracked reference directory and pin were untouched.

## Runtime guard review

Inspected `Unsupported`, `refuse` and `reject` branches in `match_tick`, `recipient_snapshot` and its composition module, World elimination, and command admission. The existing logistics/convoy tick and snapshot refusals remain. World actor/elimination guards and the ordinary recipient mode boundary remain. Ordered hospital state adds no unit flag, command, event or link that needs an admission exception. The ordinary hospital tick/snapshot check exercises the actual complete delivery path.

No FFI, header, wire, contract, privacy, public ingress or World admission changes are included. Unrelated IF-25/IF-26 checkpoint changes were removed. No assigned row is proposed as a differs candidate.

## Validation

The first native compile found a contact-key borrow lifetime error in the new test. Each loop iteration now owns its damage context while its key remains alive. Strict Clippy requested a collapsed prerequisite condition and the evaluated value of a constant test XOR expression. Both changes preserve the same checks and payload. The hospital test was extended through ordinary match completion.

Rust 1.99.0 on the Mac, six jobs, using the M1/M3 slot locks and their prescribed target directories:

- Six collection tests passed.
- Twenty-nine map, availability, weather, authoritative-map and World regression tests passed.
- Seven economy fixture tests passed.
- The final hospital test and all eight availability tests passed again after the Clippy fix.
- Both Node fixture byte-checks passed with v26.11.1/x64.
- `cargo fmt --all --check` passed.
- `cargo clippy -j6 --workspace --all-targets -- -D warnings` passed.

Every Cargo invocation released its slot immediately after exit. The commands use `cargo nextest run --locked -j6 --cargo-profile ci-test -p ww2-core` with the focused filters described above.
