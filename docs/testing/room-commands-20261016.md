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
  `convoy_room_denials_distinguish_selection_target_disabled_and_unassignable_job`.
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
lifecycle witness; final execution is required before counting it. No Hermes
ack or client adoption is claimed.

Validation uses a real shared slot with matching target directory, Rust 1.99.0,
6 GiB memory, zero swap and -j4. Nextest selects all server library tests because room admission changed, plus
focused core command, aircraft, ground-order, flight/event, convoy lifecycle and
commander merge checks. No full workspace nextest run is used. Cargo uses the repository's ci-test build profile. Workspace formatting
and strict all-target workspace Clippy are required before publication. Exact
commands, producing head and results are recorded in the PR body.

## Final main integration

Merged main `803dcadf` after the implementation commit. One conflict region in
`commander_tactics.rs` retains both the stream's whole-ground-army front and
medic seek setup, and main's `world_free` set. The following unit loop uses both
owners. No other merge region was hand edited; incoming tutorial, Gate, AI and
server test changes are automatic. The stream does not claim the newly inherited
AI rows. Focused commander policy tests are included to check this merge region.

Integration validation found two #296 witnesses still pinning the superseded
three-kind support restriction. `every_support_kind_keeps_ordinary_r1_scheduled_snapshots_flowing`
now requires actual strike/cooldown state for all eight kinds, using a visible
owned position so paradrop visibility is genuinely satisfied. The former
`recon_refuses_before_reservation_and_keeps_ordinary_r1_snapshots_flowing` is
rewritten as `recon_reserves_once_and_keeps_ordinary_r1_snapshots_flowing`,
retaining both trusted and attached rooms and requiring payment, cooldown,
canonical sequence continuity and recipient snapshots. No runtime guard is
weakened to satisfy the tests.

The initial convoy refusal fixture correctly accepted a one-metre approach:
the actuator stops nine metres short of its target, so the authored water was
not a planning barrier. The refusal witness now uses an empty truck without a
collection source, proving an unassignable job rather than inferring route
failure from terrain. The actual transfer/hold/resume/finish witness is unchanged.

Acknowledged issue #5 comment 6225924617
(`julio-merge-without-ack-20261017`): the owner records a new orchestrator merge
process, retaining exact-head checks, stack order, source review and merge
integrity review. This stream still follows its explicit stop-after-PR brief;
it does not merge or claim a Hermes ack.

Recomputed `git merge-tree --write-tree` for merges d123277b, 8d394557,
c2d4798a and d95c5013. Comparing each automatic tree with its committed merge
finds exactly the conflict paths listed above plus this additive merge record.
No additional hand-edited source path was found.

## Publication main refresh

Merged fetched main `09340ac993ac594d1702229912fd00dac7073b00`. Seven conflict regions:

- `docs/contracts/README.md`: keep main's protocol clarification and the stream's R1 contract row.
- `elimination_modes/tests.rs`: keep the physical-logistics refusal witness and make `ids` visible to its new sibling tests as main requires.
- `recipient_snapshot.rs`: register flights, logistics and main's modes module.
- `commander_ingress.rs`: keep all stream tests and main's base-command command list. Rewrite that obsolete refusal witness to require canonical reservation with no immediate world/economy/rally mutation, because #366 already admits those commands.
- `lobby.rs`: keep main's nonseated-command guard before host controls, then retain host pause/resume/end and paused gameplay rejection. This also prevents a former host spectator from reaching a seated-slot unwrap.
- `server/src/main.rs`: retain static-root/MAX_ROOMS help, named armies, and enabled logistics.
- `trusted_room_lobby_tests.rs`: retain both logistics witnesses and the complete incoming named-army lifecycle witness.

All other refreshed-main changes are automatic.

The aborted #393 conflict paths were `match_commands.rs`, `match_firing.rs`,
`recipient_snapshot.rs`, `recipient_snapshot/composition.rs`,
`recipient_snapshot/shots.rs`, `server/src/commander_ingress.rs`,
`server/src/lobby.rs` and `server/src/main.rs`. Its remaining integration needs
one canonical live-flight projection: reconcile #393's `shots::live_flights` and
`recipient_flight_contact` with #369's `flights::project` and
`visible_flight_contact`, keeping sandbox spawn, breath validation and all
corresponding tests. Reintroducing sandbox-only flight admission would regress
#369's ordinary rooms. No aborted resolution was committed.

For merge 836cc15e, `git merge-tree --write-tree` finds exactly the seven listed
conflict paths plus the additive merge record and its changelog fragment. The
independent source review accepted that exact merge. Formatter-only changes
and the final executed head are reported separately.

The final FireAt fixtures initialize ordinary logistics after their authored gun
setup. Ground, bomber and destroyer commands therefore exercise the composed
R1 logistics and flight owners together, rather than qualifying only a
logistics-disabled room. Final execution must cover that stronger fixture.

Final workspace Clippy found four seated test adapters missing main's
`Output::Watcher` variant. The adapters in `net_runtime_tests.rs`,
`net_runtime_command_tests.rs`, `room_command_ingress_tests.rs` and
`trusted_room_lobby_tests.rs` now panic explicitly if watcher output reaches
a seated commander. No prior assertion or runtime guard is removed.
