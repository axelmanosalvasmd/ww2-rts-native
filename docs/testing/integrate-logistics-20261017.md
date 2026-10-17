# Logistics integration, 2026-10-17

Branch: `integrate/logistics-20261017`. Frozen base: `61947c5b8a8438eb6ccbefa02e7d269c4e032472`.

The integration merges exact PR heads in the requested order, with no rebase or squash. It preserves World refusal and the prototype pin. The final handoff records actual test counts and the published head.

## Conflict resolutions through merge four

#404 conflicts: lib.rs union preserves commander field/world policies, numeric_keys, incoming terrain orders/terrain_fire tests. match_aircraft_commands.rs and match_commands.rs keep main combined bomber/ground FireAt dispatch and existing path callbacks, removing duplicate incoming callbacks. match_firing.rs keeps main visible_flight_contact and fire_at_weapon, ports pre-rally suppression and aimed-ability continuation. match_tick.rs keeps supply withdrawal synchronization and single main FireAt phase, adds incoming terrain validation and suppression capture. Incoming duplicate terrain runtime is reduced to metadata validation; all eight order tests use main helpers. Duplicate owner queued FireAt arm removed. Snapshot reconciliation handled separately with existing aviation/operative behavior retained.

#408: recipient_snapshot.rs retains main shared validate_snapshot/cache architecture, adds canonical Annihilation admission and elimination validation. public_state.rs retains main Classic remembered-building projection and World refusal. lobby.rs uses incoming native option/constructor helpers and unions all modules, retaining loaded false overrides and existing command-ingress tests. Snapshot path/test-helper corrections completed after #404 are included here.

#390 runtime: union lib and construction test modules; accept incoming depot claim lifecycle. Keep existing public mode rows and broader Classic/Annihilation validators and ordinary node helper, pointing incoming routing/public-state calls at it. Keep current aviation purchase admission and strict aviation validation, removing the obsolete blanket aircraft-purchase refusal. Tests delegated separately.

#405: retain main ability/visibility assertions, Annihilation logistics startup and canonical validation, main Horde fallback for six defenders/missing defend, deterministic vision implementation and assigned-only repair admission. Union AuthoredMission and Horde TickEvent/shot/simtest arms. Existing Horde idle construction/repair refusal test remains exact. Tick construction runs for Horde only when assigned facility work exists, retaining main direct-call guard and permitting incoming ordinary Horde lifecycle.

The #404 flight snapshot resolution retains the existing `flights::project` implementation, aviation/operative delivery and all four existing flight tests. Five incoming flight tests exercise that implementation through canonical tick and fog state. The malformed-flight test requires the existing exact `ShotsAndFlights` refusal and unchanged publication state; its old aircraft refusal now requires valid aircraft-source delivery. A negative launch-time guard remains.

The #390 test resolutions retain every-seat mode, actor identity, retreat flag and privacy checks while replacing partial Classic projection with complete delivery. The Classic retreat test is named `mov48_classic_retreat_traverses_multiple_legs_with_every_seat_complete_delivery`. The mode-row test includes admitted Annihilation and Horde, requires Classic nodes/ghosts, and requires their absence in the other admitted modes. Conquest and Default omit mode.

The #405 commander dispatcher test retains the complete existing policy block and exact command assertions. It now requires canonical Horde recipient projections while retaining the exact queued-tick diagnostic and the temporary hosted-attachment guard. Shared-facility recruitment and room-option tests require healthy core Horde ticks; purchase, ownership and rally assertions remain.

An independent source review found three stale Horde refusal tests in the mode-row, shared-facility and room-option checks. Those checks were rewritten to the admitted behavior before the checkpoint run. The review did not execute Cargo. Its result does not replace execution of the combined tree.

## Checkpoint diagnostics

At `c3fa3647`, the full three-package run executed 1,915 tests: 1,909 passed, six failed and ten were skipped. An earlier run was killed by the scope OOM limit during compilation, before tests; the completed run used two test threads. Future builds explicitly set `CARGO_BUILD_JOBS=2` because Nextest `-j` controls test concurrency. The memory cap stays 6 GiB with zero scope swap.

The six failures led to one runtime guard repair and exact test rewrites: restore Horde Gate refusal; update two forced-stance fixture consumers to the imported LOG-13 intent; require malformed Horde metadata errors after canonical mode admission; and require successful immediate Conquest aircraft purchase while preserving the hosted Classic training refusal. All fixture files remain unchanged. The stance difference from the pinned capture is recorded in the integration design fragment.

The focused post-fix run at `868355ed` passed all 76 selected core/simtest tests (1,636 skipped by selection), including all six repaired failures and related command, Gate, Horde, production, elimination and withdrawal checks. Independent source review accepted this commit with no findings and no execution.

## Merge five: #414

Preserve every module registration and the existing canonical Horde/Classic/Annihilation snapshot checks while adding Assault and its exact owner/timer validation. Ordinary lists reuse the existing node helper with Assault added to the whitelist. Retain per-team remembered ghosts rather than overwriting them. Lobby hosting keeps the shared option/constructor helpers and loaded false overrides, and save/load accepts the expanded ordinary-mode set without reinstating an obsolete logistics refusal. World, sandbox and roster guards remain. Test conflicts retain stronger complete delivery and actor checks while adding Assault.

## Merge six: #399

Retain the existing canonical actor predicate, logistics runner, complete mode delivery, production context, construction assignment guards and stronger every-seat tests. Register the incoming Horde lifecycle and runner test modules alongside ground production tests. Merge incoming visibility atomicity and manual-clock assertions with the exact metadata refusals. Keep the existing aircraft/operative/authored mission adapters and add explicit Horde-mode guards to breath effects. Remove duplicate Horde enum variants, event match arms and late tick phase introduced by the automatic merge. Room attachment keeps frozen main's supported aircraft implementation rather than restoring an obsolete weapon-restriction method. The core hosted-Horde guard remains until #413.

## Merge seven: #413

Core commander conflicts retain serialized intent and progress-insensitive digging comparison, adding delivered plans and the new digging predicate with serde defaults. Snapshot conflicts retain shared canonical validation, all ordinary modes, aviation/operative effects and the existing flight renderer. Incoming projectile tests exercise that renderer. Horde enters the existing global list path, and grenade timers are validated before publication.

Server conflicts retain static assets, capacity, saves, operative ingress, spectator delivery, full aviation and all ordinary startup/result modes. Add Horde host dispatch after operative handling; scoped Horde spectator refusal does not remove ordinary spectators. Roster checks count the internal Horde seat, and all modules remain registered. Native startup retains map/mission identity checks and adds Horde's canonical StartPlan. Horde starts use the shared room options and constructors, preserving loaded overrides rather than reintroducing hardcoded logistics and tech settings. The obsolete aircraft-weapon restriction call stays removed. Config, worker startup and CLI retain both parents' settings, adding authored map rereads.

Current coordination direction was reread from issue #5, including `axel-ack-julio-ack-merge-decision-20261017`: Hermes reviews after merge, World remains closed, and Unreal's pin remains Axel's decision. No issue messages were posted, as required by the integration brief.

The final Horde rewrites retain all existing assertions and replace only the deliberately superseded blanket attachment boundary: commander dispatch, Horde component/ground production, malformed physical-aircraft visibility, detached Kaiju events and room options. The room-option witness is now `mode11_horde_switches_and_shared_base_support_healthy_hosted_ticks`. A saved-intent test adds the absent delivered-plan field.

The incoming support tests pinned an older aircraft restriction that frozen main already replaced. `hosted_recorded_plane_support_pays_once_and_invalid_kind_preserves_all_state` now requires all five aircraft support kinds to pay and publish exactly, while an invalid kind retains the full unchanged-state refusal tuple. `hosted_recorded_support_pays_once_and_delivers_every_seat_through_cleanup_and_ending` extends the old artillery-only continuation check to all six kinds, natural retirement, exact plane arrival fields, paratrooper identities/metadata, victory and ending delivery. No fixture files were edited.

## Completed logistics-chain validation, 2026-10-17

Qualified source commit: `5d4b586f63c7c545143381c179330bee6e2a00d0`.
Qualified Rust tree: `38b0006e83077f7a30f824ea21dd97a691f2dd83`.
Machine: Debian cloud, 9 logical CPUs, 9.7 GiB RAM, no swap. Rust/Cargo 1.99.0,
cargo-nextest 0.9.148. Other heavy work was paused. Source, Rust tree and
Cargo.lock remained unchanged throughout qualification.

- Serial prebuild: `cargo test --locked --profile ci-test --no-run -j1 -p ww2-core -p ww2-server -p ww2-simtest`, passed.
- All four repaired focused regressions passed.
- Complete nextest: `--locked --cargo-profile ci-test --profile ci -j4` with all three packages and the external resource configuration below: **1,945 passed, zero failures, 10 pre-existing ignored tests**, 692.209 seconds.
- `cargo fmt --all --check`: passed.
- `cargo clippy --locked --workspace --all-targets -- -D warnings`: passed.
- `cargo test --locked --workspace --profile ci-test --doc`: passed; all four crates contain zero runnable doctests.

The cloud-only tool configuration, supplied through
`--tool-config-file cloud:<absolute-config-path>`, contains only:

```toml
[[profile.ci.overrides]]
filter = 'test(engineering_consumers::support_selected::) | test(movement_existing_checks::) | binary(trusted_room_process)'
threads-required = 'num-test-threads'
```

This reserves the four slots for large fixture consumers and trusted socket
process tests while other tests remain parallel. No retries, timeouts,
fail-fast, default-filter or ignore settings were changed; the repository CI
profile is unchanged. Build jobs are serialized, debug information and
incremental compilation are disabled, and the named ci-test profile retains its
normal assertions and checks.

The earlier complete run at `7597b91095657218873773334a29a9661a7d2489` passed
1,941 of 1,945 tests and found four failures. Three were obsolete fixture
expectations: floating JSON zero versus canonical wire zero, Assault refusal
after admission, and Horde refusal after admission. The fourth was a runtime
merge regression: away flags incorrectly depended on send readiness instead of
socket presence. The repair restores `connection.is_none() && !ai`, keeping the
original absent/unready/expiry regression unchanged. It matches the same
presence correction already carried by cross-chain #430.

All rewritten tests and preserved assertions are listed in
`changelog.d/2026-10-17-integrate-logistics-validation.md`. In particular, support
arrivals retain exact full-object equality and now must arrive exactly once per
seat; admitted Assault retains every mode/structure/feature check; the Horde
case becomes real two-socket start and continuing delivery while all remaining
unsupported settings and weather cases stay checked. Tutorial remains guarded
on this standalone chain and will be reconciled with the rooms chain during
final integration. World admission is unchanged.

The initial lint repair in `7597b910` only replaces indexed arrival bookkeeping
with enumeration and has its own changelog fragment. All seven exact PR heads
in this chain remain ancestors. This receipt qualifies the chain, not the final
combined workspace or three-platform main CI.
