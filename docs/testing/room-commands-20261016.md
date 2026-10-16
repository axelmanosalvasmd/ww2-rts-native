# Room command stream

## Merge record

Base: PR #296 at `5725bdc2308a89ef66cf0d9f269b80780073cac4`.
Merged PR #366 at `be83659f9783d95f31282228cecf31733b01d2f0`.
The base already contains the earlier #366 integration. All seven conflict
regions retain the base side:

- `docs/contracts/r1-ingress.md`, held-dependency list: retain buy/support admission.
- Same file, PR 296 integration section: retain its limits and denial documentation.
- `commander_ingress.rs`, three regions: retain buy/support tags, parsing and command admission.
- `native_net2_socket_tests.rs`, AppState: retain `static_files: None`.
- `seat_lifecycle_tests.rs`, host pause/resume regression: retain the complete test.

All other merge changes are automatic. No guards or test registrations were removed.

Merged PR #369 at `161bbe3880f0ac06187c64bfe3ff2dab0856841c`, including its
#349 flight ancestry. One conflict region in `commander_ingress.rs` joins
the complete garrisoned movement and production-cancel tests from #296/#366
with #369's complete FireAt/compound refusal test. All three are retained.
Other merge changes are automatic, including `room_runtime.rs`.

Merged PR #375 at `d2954b59b539e82d386ed1f6c1245664c69a6d19` for convoy
tick and recipient delivery. Four conflict regions:

- `save-format.md`: retain the base optional elimination-field documentation.
- `recipient_snapshot.rs`, module registration: retain both flights and logistics.
- Same file, projection: retain both flight insertion and optional logistics insertion.
- `server/src/main.rs`, help text: retain base MAX_ROOMS/static-root help and
  update logistics to on, matching the incoming runtime configuration.

All other changes are automatic.

Attempted #393 at `d374de4197b85c1fe2e719e45957291d1bc93a8e`, then aborted
without changing either parent. Eight conflicted files included competing live-flight
adapters, snapshot admission, event guards and canonical command dispatch.
The sandbox integration remains with #393; this stream implements the assigned
non-World convoy portion of CMB-150 without replacing flight/privacy owners.

## Scope and intent mapping

- CMB-141: `fireat_ingress_preserves_denials_and_direct_shell_eligibility`,
  `fireat_ingress_salvo_unseen_ground_queue_limit_and_replacement`,
  `fireat_ingress_bomber_area_and_mixed_ground_queue_reach_both_seats`,
  `fireat_ingress_destroyer_shells_open_ground_through_ordinary_delivery`.
  The ordinary room witness covers the direct/salvo refusal matrix, eight waiting
  orders, replacement, bomber area missions, mixed queues and naval salvos.
- CMB-150 (assigned non-World convoy portion):
  `convoy_room_shapes_preserve_selection_order_duplicates_and_uncapped_orders_fallback`,
  `convoy_room_supply_stop_resume_survives_pending_save_and_finishes_with_every_seat`,
  `convoy_room_denials_distinguish_selection_target_disabled_and_disconnected_route`.
  Full-row sandbox integration remains with #393; running World recovery is deferred.
- NET-43: `net43_room_routes_fireat_and_convoys_with_guest_keys_and_exact_denials`
  supplements inherited #296/#366 tests. It checks pause, guest authority, stamped
  input, recipient-local exact denial and snapshots. Full command union remains
  unproved: attack, ability/autocast, build, board/unload, escort/rail, compound and
  sandbox are still guarded. No complete NET-43 claim.
- NET-58: inherited #369 flight/event tests are requalified on the composition.
  Ordinary shots, flights and aviation are covered. Operative small-arms and
  Horde breath events retain separate guards, so no full-row NET-58 claim.

All new test files are explicitly registered. The old public FireAt refusal is
replaced by its positive/denial witnesses while the full compound refusal remains.
Two inherited mixed-aircraft refusal witnesses are rewritten to require the new
direct/salvo mixed execution and queue semantics; their names and reasons appear
in the changelog fragment. Other inherited assertions and registrations remain.

## Coordination and validation

Acknowledged locally: `axel-interface-response-004`, owner decisions
6217590741 and 6217727398, Axel's confirmation 6218552386, and the
post-hotfix decision 6224738109. Follow E/H/I/M/N/O/P with prototype shapes;
keep C's World admission boundary. ABI 2 and the e4e579a pin are unchanged.
No issue messages are posted under this stream's no-issue-write instruction.

Source-only review accepted the bounded command implementation and found the
initial missing naval FireAt branch. The branch now receives a dedicated ship
lifecycle witness; execution is required before counting it. Hermes' exact-head
runtime ack remains pending, and no client adoption is claimed.

Validation uses a real shared slot with matching target directory, Rust 1.99.0,
6 GiB memory, zero swap and -j4. Nextest selects only core/server tests for the
new command tests and affected ground-order, flight/event, room and convoy
lifecycles. Cargo uses the repository's ci-test build profile. Workspace formatting
and strict all-target workspace Clippy are required before publication. Exact
commands, producing head and results are recorded in the PR body.

## Final main integration

Merged main `803dcadf` after the implementation commit. One conflict region in
`commander_tactics.rs` retains both the stream's whole-ground-army front and
medic seek setup, and main's `world_free` set. The following unit loop uses both
owners. No other merge region was hand edited; incoming tutorial, Gate, AI and
server test changes are automatic. The stream does not claim the newly inherited
AI rows. Focused commander policy tests are included to check this merge region.
