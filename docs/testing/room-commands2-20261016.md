# ROOM-COMMANDS-2 integration

Base: ROOM-COMMANDS #425 at `745229e9`. Main `4a88afd8` merged automatically as `848203d2`.
Stacks on #425 (#296, #366, #369 with #349/#337, and #375), plus #393, #374 and #413.
World recovery and World admission remain deferred under owner decision C.

## Sandbox merge

Incoming #393: `d374de4197b85c1fe2e719e45957291d1bc93a8e`.
Hand-resolved regions:

- `match_commands.rs` (five): union Spawn with Supply/LogisticsResume/Compound variants, JSON parsing, early execution, exhaustive command matching and eligibility exclusions.
- `match_firing.rs` (five): retain #369's canonical flight_contact_for and visible_flight_contact, including recipient-visible bodies and current poses. The equivalent incoming contact helper is superseded.
- `recipient_snapshot.rs` (one): retain unconditional canonical flight projection and optional logistics projection. Sandbox uses that same flight owner.
- `recipient_snapshot/composition.rs` (three): preserve canonical flight fields and ordinary flight admission; combine firing, grenade and salvo validation. Keep chronological aviation-buffer refusal.
- `recipient_snapshot/shots.rs` (four): preserve ordinary-flight documentation and Throw admission; add incoming sandbox Breath validation while retaining SmallArms refusal pending NET-58. The duplicate incoming live_flights implementation becomes a test-only call to the canonical projector. All incoming flight shape/contact/privacy tests remain.
- `commander_ingress.rs` (four): add Spawn recognition, name and canonical admission to the entire existing command union; preserve selection guards.
- `lobby.rs` (two): preserve readiness, private Conquest, named armies and logistics; admit the sandbox flag.
- `main.rs` (one): retain static root, room limit, named armies and logistics help; state optional sandbox.
- `trusted_room_process.rs` (one): remove obsolete sandbox/named-army refusal entries. Positive lifecycle coverage is retained from both owners.

No other source path is manually changed in this merge. All automatic changes and test registrations remain.

## Operatives merge

Incoming #374: `0a712ac4f909f130fa3518b1586b2089e6e07b0f`.

- Contract index (one): combine operative hit-position and spectator decisions with the R1 ingress row.
- Protocol (one): retain operative prototype hit positions and cached mask from incoming, ordinary spectator decision from base.
- `match_firing.rs` (five): retain the same canonical visible-contact owner as the sandbox merge.
- `recipient_snapshot.rs` (three): union all modules and exports; retain canonical flights plus logistics projection.
- Composition (two): retain ordinary flights/salvos rather than restoring operative-only restrictions. Remove the operative-only global validator; canonical firing/grenade validation remains.
- Shots (two): retain sandbox breath handling and incoming operative SmallArms validation. This intermediate merge narrows Throw to operative shooters; the following Horde merge restores general Throw identity and owner validation. Horde breath still awaits its mode owner.
- `lobby.rs` (five): retain both spectator and operative starts, admit operative hello before commander restrictions, preserve spectator joins in running rooms, retain optional returning-seat handling, and register both test modules.
- `room_runtime.rs` (one): retain latency telemetry and incoming operative event/list methods.
- The intermediate incoming `operative_flights::project` delegates to the canonical projector. The implementation successor removes that unused entry point and connects its operative-flight validation through the common projectile validator, scoped to operative-produced flights so ordinary ground flights remain admitted. This preserves original ballistic stamps and all operative tests.

## Hosted Horde merge

Incoming #413: `a1be2f7419faa90fedfb6138e7a1a4d0e692ba5d` (#399/#357 ancestry).

- `match_tick.rs` (one): retain initialized ordinary convoy validation and World logistics refusal; add Horde economy validation.
- `recipient_snapshot.rs` (two): preserve wave-zero field omission and current mode rows; retain companion validation plus projectile validation.
- Composition (two): preserve canonical flights and complete flight/grenade/salvo validation.
- Public state (one): retain the general mode owner rather than replacing it with Horde-only mode projection.
- Shots (five): union Aviation and Horde tick events; retain ordinary producer-buffer guards, operative SmallArms checks and invalid Throw test; admit breath for sandbox or Horde; retain general Throw identities and owners.
- `lobby.rs` (four): retain spectator-capable attachment and Horde canonical seat count; preserve operative/spectator hello, with five-defender Horde lobby capacity; retain both operative and Horde control dispatch and both modules.
- `main.rs` (two): retain existing help plus Horde/map-directory help and authored map configuration.
- Incoming `projectiles::for_seat` delegates to the same canonical flight projector. Its complete validator and all projectile tests remain. The duplicate geometry/contact implementation is superseded.

All inherited World, aircraft-weapon, privacy, invalid-state, terminal tick and command guards remain unless their explicitly documented runtime owner replaces the old admission restriction.

## Row accounting

- CMB-150 (R1): `cmb150_sandbox_spawn_and_convoy_commands_share_ordinary_ticks_and_recipient_delivery`, `cmb150_nonworld_recovery_and_unknown_orders_keep_exact_blocked_denial`; inherited `convoy_room_shapes_preserve_selection_order_duplicates_and_uncapped_orders_fallback`, `convoy_room_supply_stop_resume_survives_pending_save_and_finishes_with_every_seat`, and `convoy_room_denials_distinguish_selection_target_disabled_and_unassignable_job`; #393 sandbox dispatch, refusal, aircraft/grenade flight and terminal-delivery tests. The new composition sends Spawn, Supply, Stop and LogisticsResume through the same recorded queue and captures every seat after each tick. Non-World Recover and unknown commands retain `blocked`. World recovery remains deferred and unclaimed.
- NET-43: see the complete command-union mapping in `net43-roomcmds2.md`.
- NET-58: see the event composition mapping in `net58-roomcmds2.md`.

Owner decisions acknowledged: E/O/G/K/M for prototype events, operatives, Horde and commands, plus H for the inherited convoy owner. ABI 2 and prototype `e4e579a` remain unchanged. Issue #5 comments 6217590741, 6218552386, 6224738109 and 6225924617 supply the recorded grant, World deferral and current queue process. No issue write, Source recorder, FFI change or Unreal execution occurs. The orchestrator retains hosted checks and merge.

Independent merge review found a missing test gate on the operative module after the additive registration merge. The implementation commit restores `#[cfg(test)]` on `operative_server_tests.rs`; the spectator module retains its own gate. This corrects the merge without changing either test body.
The same review found that the retained Horde projectile tests depended on imports from their former parent implementation. Their module now imports `Point`, `Point3` and projectile geometry directly. No test predicate changes.

The independent review also identified unused operative flight helpers after projector unification. The successor connects the retained validator to the common projectile preflight, filtering only operative-produced flights and their grenade tickets. Generic firing validation still checks every flight and grenade; operative shooter, source-kind, effect and blast checks apply to the operative subset. The unused projector entry point is removed.

Independent merge-tree audit recomputed automatic trees `97bcd539` (main), `7fa4ea2a` (#393), `16ea4246` (#374) and `f392b069` (#413). The main merge matches exactly. Each dependency merge differs only in the paths recorded above and this integration record. No hidden hand-edited path was found. A successor bounded source review accepts the documented repairs and NET-58 additions; execution and a final combined-head review remain separate gates.

The sandbox start test's historical Horde refusal is replaced by `sandbox_horde_start_and_spawn_keep_every_canonical_seat_live`: host-started sandbox Horde, guest enemy spawn, correct Horde owner and six ordinary ticks with all three canonical snapshots. World refusal remains. The seated/operative output adapters in `operative_server_tests.rs` and `horde_host_tests.rs` explicitly reject watcher output after composing the spectator enum variant; their earlier assertions remain.

The first shared-target diagnostic selected zero tests from reused binaries and counts as no execution evidence. Qualification forces current crate recompilation before interpreting any test inventory or result.

The combined `59ed284c` review found Classic terminal results still restricted to Conquest/Horde. The successor admits Classic's existing territory timeline and extends the cancellation witness through queued headquarters destruction, natural elimination, 120 hold callbacks, 60 frames per seat, recipient outcomes and stable lobby return. Public Classic start remains guarded.

Workspace formatting applies only mechanical Rust layout in the owned logistics test, recipient list/cache/composition/operative validation/owner tests/projectile tests/public state files, and server ingress/lobby/operative output/command-union/sandbox tests. No formatter-only predicate or guard changes.

The first rebuilt compile found the inherited Horde projectile test calling the removed `TickState::restrict_aircraft_weapons_for_delivery` method. The canonical aviation owner already supersedes that historical delivery restriction. Remove only the obsolete setter call from `ordinary_horde_grenade_and_barrage_deliver_every_seat_until_retained_state_expires`; the fixture contains no aircraft, and every grenade/barrage/flight/lifecycle assertion remains. This compile failure supplies no test pass.

The next rebuilt compile found one missing inherited room field (`Person.map_pings`) in operative admission and six server-test API mistakes: test-only core `into_json` helpers and `MatchCommands` Debug were unavailable across the crate boundary. Initialize the same empty map-ping history used by other room members, wrap exported recipient fields as JSON, and compare cloned elimination/production/control state plus pending-command count. No denial, privacy or mutation assertion is removed. This second compile failure supplies no test pass.

The first executable focused batch ran 13 of 70 tests: 12 passed and the inherited sandbox wave-zero Kaiju test failed because the hosted Horde row omitted a real boss bar whenever `wave == 0`. Prototype `shared/sim.js:945-955` computes the bar from live bosses even in an active sandbox wave zero. Keep profile omission until wave one, but publish the existing boss field whenever its canonical bar is present. The original test and every assertion remain. The other 57 selected tests were not run after fail-fast.

The binary settings matrix also inherited a Horde start refusal that #413 deliberately replaces. Remove only Horde from that refusal matrix and add `ordinary_binary_horde_and_sandbox_horde_start_and_deliver_to_both_sockets`, requiring both real sockets to receive the three-seat start and subsequent Horde frames with sandbox off and on. World, Classic and the other unsupported setup refusals remain. The stream PR records its separate process execution result.

The complete focused run at `be3ff5c7` executed 70 tests: 67 passed and three failed. The inherited malformed-flight test used `usize::MAX` as a ground projectile's launch cell, which the combined canonical validator correctly rejects before the intended NaN mutation. Its scene now uses the actual launch cell. The sandbox flight shape expected floating zero values while the canonical projector normalizes them to integer zero, matching JSON.stringify. Only those zero literals change; every shape, fog, contact, privacy and atomic-refusal assertion remains. The third failure, the NET-43 aimed Ability room fault, is investigated separately with the actual fault included in the existing assertion.

Fresh main `6f02f83ad1f4fff6c7945ad3e5ac263a8bf41c58` was merged as `4ee4d51e2aaf0ecb4b38c1bf7ad104a2a60082ae`. It records the spectator merge and has the same tree as previous main `4a88afd8`. There were no conflicts or source changes. The merge tree matches the automatic tree `1bb34554041f1427edbd27eb0edc24774e7de35e` exactly. Issue #5 owner reply 6227193135 acknowledges the queue process without a pre-merge Hermes acknowledgement; World admission and the Unreal FFI pin remain unchanged. This stream makes no issue write.

## Main composition after combat filtering and AI saves

Incoming main `52a8315f` includes #396 combat filtering, #392 numeric keys and #391 AI planning. The merge starts from `bb4d5d87`; its conflicted automatic tree is `d3844e8526a30d538d46eab77222c39ee1a913dc`. Every nonautomatic edit is listed here:

- `DESIGN.md` (one conflict): retain the existing medic, house-shelling and FireAt decisions and the incoming external AI-memory decision.
- `horde_commander.rs` (one conflict, plus serialization repair): retain delivered plans, `is_digging`, incoming `same_intent` and its numeric-key comparison, and incoming save derives. Add the same derives to `DeliveredPlan` and `DeliveredIntent`; default a missing `OwnPlan.delivered` to `None` for older AI memory.
- `recipient_snapshot.rs` (three conflicts): retain the admitted-match description. Keep incoming immutable row reuse, cached-constructor counter and common recipient projection. Move every existing mode, Classic actor/elimination/production, Horde economy, shape, identity, engineering, aircraft, companion and projectile check to the common validator used by cached and uncached paths. Common projection retains canonical flights, falling and optional logistics. Keep `seen_by` available to its existing crate sibling. Do not restore Logistics to the missing-capability list.
- `ai_view/commander.rs` (one automatic-merge compile repair): supply incoming `hold_fire`, `auto_cast` and `cooldown` from delivered `holdFire`, `auto` and `cd`, with owner gating. Read no canonical state.
- `seat_commander/saves/tests.rs` (one automatic-merge compile repair): initialize the existing delivered-plan field to `None` in the incoming fixture; retain every save assertion.
- `recipient_snapshot/combat_filter_tests.rs` (one test reconciliation): the malformed orphan operative is rejected earlier by retained companion validation as unit 0 with Operatives capability. Preserve the test and both cached/uncached refusal assertions, changing only the expected unit attribution.
- This integration record and `changelog.d/2026-10-16-roomcmds2-main-composition.md` document these changes.

Automatic delivery changes retain row reuse through event/list/terrain publication. Classic nodes and incoming building ghosts coexist. Public Classic startup, World admission, and canonical guard boundaries remain unchanged. The stream PR records the final merge tree comparison and qualification head.

## Main composition after spectator missions

Incoming main `3c33cd040b8fcaa71d2795b3b3030c749b522706` adds #400 spectator mission paths. From parent `8c1d1dae8fe6a13a5feb9f737eadd613fa73caed`, its conflicted automatic tree is `83183c06893709b4600d2e7f78bc00a91807975e` (computed with those literal parent SHAs). Every hand-edited region follows:

- `match_human_observation.rs` (one): retain both Horde and AuthoredMission as noncombat observation events.
- `match_tick.rs` (one): retain both event variants.
- `recipient_snapshot/shots.rs` (three): retain both borrowed variants, both validated raw-event arms and authored mission chronology checks. Preserve the existing admitted Aviation conversion instead of restoring the incoming historical Air refusal.
- `native_startup.rs` (one): retain incoming authored-mission map-definition binding, then choose the existing Horde, Classic or ordinary StartPlan builder. Keep every map/layout and mode guard.
- `simtest/tests/common/companion_tick.rs` (one): handle both event variants without changing fixture predicates.
- `simtest/tests/match_tick_events.rs` (two): convert both event families and retain their refusal in the original four-family fixture. Extend its panic text to name AuthoredMission; all assertions remain.
- This record documents the merge. No other path is manually changed in this merge.

The automatic authored-mission field initializers, validation, spectator filtering and tests remain. No prototype, C ABI or parity-status file is edited by this reconciliation.

## Broader qualification repairs

At `036d5abb4723c06a010229d86eb956d82849181d`, all 70 focused tests pass. The broader affected-package run executes 575 tests: 570 pass and five fail. The existing aircraft/artillery full-match test exposes a retained Horde-only projectile guard rejecting an actual aircraft round as `Unsupported { unit: 0, capability: Air }`. Remove that obsolete source-kind exclusion, preserving canonical firing validation (including the aviation sortie owner), all generic trajectory/grenade/salvo checks, and operative validation. Both aircraft and ground rounds use the same already documented flight projector. The original full-match test and all its assertions remain unchanged.

The other four failures concern obsolete support and mode admission expectations, the newly canonical Attack denial, and connection-readiness fixture semantics. Their test reconciliations and reasons are recorded separately. Passing focused tests at `036d5abb` are historical evidence, not a claim that a later repair head has executed.
