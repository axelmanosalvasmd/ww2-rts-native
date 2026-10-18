# Final parity integration, 2026-10-17

Base: `61947c5b8a8438eb6ccbefa02e7d269c4e032472`.
All merges retain exact parent commits with no-ff ancestry. No squash, rebase or
force-push. World admission remains closed and the reference pin is unchanged.
Final combined validation and all nine three-platform main CI jobs passed at
`4ff2e3daed31d10dfb27276a8c1916525f658af0`. See the
[final qualification receipt](integrate-all-final-receipt-20261017.md).
The provisional/pending statements below are chronological history, superseded
by that final receipt.

## Current union status

The sections below are a chronological merge record, not independent claims
about the final runtime. In particular, #423 supersedes the earlier split-owner
composition: one selected-delivery production owner now handles ordinary AI,
Horde defender AI and the internal Horde enemy. The logistics owner is cfg(test)
only. Tutorial retains its scripted enemy and human-only startup.

The first full optimized diagnostic at `50c4c1847c3efe714242536ffc9ccde67f57063e`
(Rust tree `87fbaee256a6192564e8b87dbec466c3de61f3f1`) compiled and ran all
2,448 enabled workspace tests: 2,396 passed, 52 failed, 16 skipped, in 1,139.344
seconds. Two failures were SIGKILL. This is not a qualification receipt.
Repairs and fresh full-tree validation remain pending. The changed expectations
and fixture repairs are enumerated with exact source heads in
[integrate-all-expectation-map-20261017.json](integrate-all-expectation-map-20261017.json).

The diagnostic passed the expanded 119/118-callback ending/ticket test, the
ordinary AI save/restore queue witness, and the existing production
`paused_scheduled_event_clear_resets_ai_alert_cursor_before_resume` witness.
The combined-role owner witness failed on operative reserve publication and
cannot yet be cited as passing. Production aircraft witnesses remain registered;
old blanket aviation refusals are intentionally replaced by #422's delivery
and malformed-flight guards, not restored as a capability restriction.

### First repair verification

The first repair prebuild passed in 18m57s. Its 53 selected checks completed in
189.709s: 45 passed and eight failed. The selection deliberately skipped 2,411
nonselected/ignored entries; this is not a full-workspace qualification.
The combined-role sole-owner/actual-queue witness and timed-v1 room-envelope
refusal now pass. The support replay previously killed in the full diagnostic
passed in 6.107s with an exclusive local resource slot weight.

The operative timing replay was still killed. An isolated unchanged-binary
measurement reproduced signal 9 after 32.434s at 6,943,104 KiB peak RSS. Kernel
OOM records were not available, so the signal is not labeled as a proven OOM.
Its repair stores the same complete tagged checkpoints as lossless gzip JSON
and compares their complete bytes, rather than retaining many JSON object trees.
Other remaining failures concerned raw-versus-wire numeric flags, an async
missing-map boundary, current-start memory refresh, and a component fixture's
null build sentinel. Fresh validation of those repairs is pending.

The second repair selection completed 53 tests in 286.167s: 52 passed and one
failed. Both previously killed tests passed, including the complete compressed
operative replay (120.947s). All canonical scenario, operative, wire-node,
Horde save, component-construction and combined-role queue repairs passed.
The remaining NET-92 expectation incorrectly included the old private policy
random stream, although the recorded contract creates a fresh commander after
restoring serialized strategy. Its corrected witness compares the entire policy
against an independent fresh-commander restore, including its RNG. The existing
core `ai63_planner_checkpoint_retains_full_memory_without_random_streams`
witness directly checks that the codec leaves the new random owner unchanged.
The final NET-92 focused run passed (1 test, 1.271 seconds; run ID
`e863d767-f7d9-4d25-b51c-d51677aff02e`). Formatting and strict workspace
Clippy passed after the repairs. These separate diagnostics do not replace
full-tree qualification, which remains pending.

### Replay and JSON-order audit

The IF-28 checkpoint helper is used by both identical replays and the reversed
order counterexample at the same original input boundaries. It serializes the
complete `Match::save_json()` value with the same `serde_json::to_writer` and
lossless gzip encoder. Full buffer equality is stricter than JSON Value equality;
no key-order-only failure has been re-pinned. The compressed replay passed.
The uncompressed signal-9 measurement does not establish a runtime memory
regression or its absence. The named test does not exist at freeze `61947c5b`
(`git grep` over that complete tree found no symbol); it was introduced by the
operative chain. Therefore an identical main-baseline RSS comparison is unavailable
without backporting a different test and its missing runtime capabilities. No
baseline RSS number is claimed, and total simulation footprint is not claimed
unchanged. The measured failure and the checkpoint-storage mitigation remain
separate facts.

`preserve_order` changes JSON object iteration globally. Byte identity to main
is not claimed. Its direct serialization effect is object-key ordering, not
array order or numeric values; other integrated gameplay/schema additions have
their own documented changes. The public map-name arrays are explicitly sorted.
A source audit found no production save/view/WebSocket JSON checksum or digest.
`map_editor.rs` SHA-256 hashes password bytes, not JSON; the lobby seed hasher
hashes unit entropy, not messages. The World metadata test digest explicitly
sorts object keys by UTF-16 before hashing (`world_recipient_metadata/tests.rs`,
`digest`). World terrain SHA-256 tests hash joined terrain character rows.
Startup-table golden bytes use the immutable compiled catalog buffer, not
`serde_json::Map` serialization. Recorded fixture hash checks refer to immutable
original captures. These checks remain enabled and their golden values are
unchanged. Same-run replay byte equality remains deliberately order-sensitive.
The full validation must still confirm all these witnesses together; this audit
is not a claim that every final-union message equals frozen-main content.

The construction component's null-to-zero unassigned build normalization matches
the original full comparator at `50c4c184`,
`rust/crates/simtest/tests/construction_tick.rs:415-416`. No new sentinel is
introduced. The synthetic Horde fixture separately asserts the Classic-only
logistics admission refusal before removing logistics state/options solely to
exercise the missing-Horde-owner guard; neither guard is narrowed.

The operative disconnect/respawn lifecycle witness additionally runs with
physical logistics disabled and enabled, requiring reserve publication on first
spawn and respawn and reserve removal with the retired body. All preexisting
lifecycle assertions are retained. Its expanded focused verification passed; full unchanged-tree qualification is pending.

### Explicit #442 save compatibility exclusion

The union deliberately does not carry #442's timed-v1 compatibility. Current-main
version-1 files carrying `authored_mission` become explicitly unsupported, never
silently stripped or defaulted. Non-authored version-1 files remain supported;
complete authored scenarios use version 2 and a required nullable clock. The
core legacy-owner refusal and the room-envelope v1 refusal are both required
witnesses. `save-format.md` describes the old owner as decode-only and refused.

Retained #442 scope: ordered localization and map binding, authoring fairness,
Horde defender scenario behavior, and C-entry coverage. This exclusion follows
the recorded handoff and [Hermes' clarification](https://github.com/axelmanosalvasmd/ww2-rts-native/issues/5#issuecomment-6232712225).
There is no new pre-landing acknowledgment gate; Hermes reviews after landing,
following full local qualification and the authorized one-pass integration.

## Chain 1: AI

Merged exact `bc30b71012ba3c4ce25f4237128d337e03849e24` automatically. The handoff
records 1,994 passing tests plus formatting and strict Clippy for this chain.
That historical chain result does not qualify the final combined tree.

## Chain 2: rooms

Merged exact `02c048f1be4ee7ae48b93369625063e5a732051a`, whose qualified Rust tree
has 1,918 passing enabled tests. Manual resolutions:

- Global lists retain the broader represented-ordinary-mode guard and use the
  shared Classic-node validator/number conversion, removing only the redundant
  older inline node construction. Wreck identity validation remains.
- Snapshot validation retains Tutorial, Assault and elimination-specific checks,
  and keeps exactly one Classic-only wallet insertion. Other ordinary modes do
  not acquire undefined prototype wallets.
- Save-string diagnostics keep the incoming escaped/debug formatting.
- Poll connectivity uses AI-aware Person::online and still appends the internal
  Tutorial enemy's connected slot. Public start retains its own roster/mode
  guards rather than accidentally inheriting a displaced trusted-start block.
- The async trusted-start owner now carries the AI branch's human-token checks:
  AI seats are allowed, human tokens remain required and distinct, teams remain
  opposed, and host/readiness/restart guards remain. Sandbox admission from the
  rooms chain is not reverted to the older AI branch's restriction.

No test or registration is intentionally removed. The later save merge carries
the separately qualified trusted addAi two-seat bound. Final combined tests are
still pending.

## Chain 3: logistics

Merged exact `5e6966ae4cdad75ced8d1aba9ff745d2c2825186`, whose qualified Rust tree
passed 1,945 enabled tests. Composition preserves both complete test modules
where AI and logistics independently added files with the same path:

- The AI chain's terrain-aware CommanderViewMemory lives in commander_view.rs;
  the hosted Horde chain's detached CommanderMemory remains in commander.rs.
- Ordinary rooms retain their delivered-snapshot AI owner, including saved
  planner memory. Horde retains its hosted owner and internal enemy schedule.
  Explicit runtime guards prevent configuring both owners on one runtime.
  Both test suites remain registered. Seat handover resets the active owner.
- The shared tick batch generation retains the Horde alert-cursor reset, while
  ordinary AI still observes the actual selected start/frame packets.
- Async trusted startup now carries ordinary mode, Assault defender and loaded
  feature flags into the canonical constructor. Its host, readiness, token,
  restart and cancellation guards remain; Horde keeps its dedicated reread path.
- Tutorial and Horde internal slots are appended exactly once. Away remains
  socket absence and non-AI; auto-pause readiness is unchanged.
- Recipient validation composes Tutorial, elimination, Assault and Horde guards.
  Classic nodes have one validated producer, with existing zero normalization.
  The malformed synthetic Horde supply witness retains UnsupportedMode from
  the common actor/finish admission guard, with all atomicity assertions intact.
- Queued aircraft retain the transactional aircraft publication helper; ground
  births retain the incoming logistics reserves and producer accounting. The
  redundant unreachable inline aircraft path is not duplicated.

These are source-composition results, not a qualification of the combined tree.

## Chain 4: save (composition in progress)

Exact parent: `82f25be388cbbebfd5a773a13e5825906c539924`, qualified at 1,959 enabled
tests. The additive C ABI revisions, authored scenarios, editor persistence and
Horde records are retained. Final combined qualification is pending.

The save chain added a third ordinary AI scheduler at the same path as the
already-integrated hosted Horde adapter. Running both would submit duplicate
orders. The merged ordinary room therefore keeps the AI chain's actual delivered
frame owner and codec. The save chain's inactive human helper state remains a
separate room helper owner, reset on seated takeover and restart. Its core tagged
AI codec remains available and tested independently. The ordinary room does not
silently serialize a second inactive planner as if it were the active AI.

The reset/restore tests are adapted to inspect the active runtime AI commander
and its full delivered-view memory rather than the replaced third scheduler;
human helper identity, order preservation, atomic failed load, spectator starts,
strategic memory, fresh attention and restart assertions remain. The existing
AI-chain codec roundtrip and whole-save clone-failure witnesses remain.

Additional composition:
- Preserve both independently added engineering suites under separate paths.
- Combine field-work and retreat snapshot checks with complete-frame arrays and
  every incoming own-unit, hidden-unit and work-row assertion. The deliberately
  superseded partial-snapshot missing-capability lists become complete delivery.
- Resource nodes retain richer position/rate/fuel/visibility rows; the incoming
  explicit node-worth compatibility field and its finite validation remain.
  Aircraft readiness uses one common air_at_base field in both implementations.
- Preserve typed authored-scenario events and reject the decode-only legacy
  authored-mission event rather than emitting it. Tutorial and Horde event
  validation remain present.
- Authored editor maps are captured for async generation; public and trusted
  startup bind to the same private map source. World admission stays closed.

The four-chain diagnostic `cargo check --locked --workspace --all-targets -j1`
passed after source composition. This compiles tests but does not run them.
Final tests, Clippy, FFI linking and three-platform CI remain pending.

Pinned-reference zero-form check: unmodified `shared/sim.js:5249` creates the
plane-arrival object with `dir: s.dir`; `server.js:195` and the seat snapshot path
at line 246 use JSON.stringify. Executing that exact emission statement and send
helper on Node v24.19.0 with controlled +0 and -0 both emits `"dir":0`.
This is a bounded serialization component check, not a full support simulation.

## Cross-chain #422

Exact parent `42e3afe6b32ff289181407ff7ed9f99236fa96b5`. Retains complete ground
flight validation, full-map watcher trajectories and the existing operative,
Tutorial, authored-scenario, Sandbox and Horde event guards. Horde spectators
use the already-recorded full-map watcher boundary; ordinary startup remains
transactionally prepared before publication.

Both incoming support payment/delivery witnesses remain in addition to the
qualified invalid-kind atomicity and full-cleanup/ending witnesses. The obsolete
fire-at refusal is represented by the existing positive fire-at ingress witness
and retained compound-order refusal; base commands retain positive canonical
queue admission. No garrison movement witness is removed.

The incoming support planner's delivered strike announcements and stationary
contact clocks are also populated by the active ordinary delivered-frame adapter,
not just the independently retained Horde adapter. Its restore accepts older
memory without stationary clocks through a default-empty field. Aircraft mission
selection is invoked once, avoiding duplicate orders from the two composed paths.

## Cross-chain #431

Exact parent `96841c7dad4afb48706256eb247f4b18132689b4`. Canonical reserve/convoy
validation and delivered owner lists compose with the existing World refusal,
all ordinary modes and companion/operative validation. Duplicate validation
branches use the new common logistics checker, with World still rejected first.

Both full-tick and NET-54 suites remain registered. Manual-convoy witnesses keep
real ticks and complete delivery plus incoming immutable preflight assertions.
The forced-withdrawal witness now uses actual enabled physical logistics and
checks each owner's reserve row, instead of relying on disabled synthetic
reserves. Cached/uncached and partial-API capability checks are retained alongside
complete-mode frames. Classic retreat complete-frame checks and all atomicity
assertions remain. Authored scenarios keep the typed v2 event owner.

The aimed-approach firing repair is retained. Production uses transactional new
reserve preparation; World training with logistics remains refused. Presence
mapping remains socket absence and non-AI. The active ordinary delivered adapter
also carries the incoming Assault defender fact from its delivered mode row.

## Cross-chain #436

Exact parent `6f4107054129e6404c39907dd61067549b6dfc74`. Seeded JavaScript/native
AI outcome comparisons remain opt-in as authored. Their earlier PR qualification
is historical evidence; this integration has not yet rerun them. AI-65 remains
`differs` under the recorded independent-stream decision.

Retain the current combined guards and complete-frame witnesses rather than
restoring older partial-mode paths. The elimination logistics test is renamed to
`classic_and_annihilation_initialize_physical_logistics_and_deliver_every_seat`
and retains its original non-structure reserve assertion plus the incoming
three-tick every-seat delivery loop. The withdrawal witness additionally proves
recipient delivery did not mutate its reserve state. Duplicate copies of the
same named fire-at ingress witness are consolidated without removing assertions.

## Cross-chain #420

Exact parent `f0b947e3b4b73fa8ee8eef00a4e299462728db7c`. Retains ordinary AI command
union and privacy tests, qualified aircraft training and aimed-approach firing.
Tutorial remains admitted alongside the incoming canonical ordinary-mode checks;
its detached-view rejection becomes a positive Tutorial packet witness, while
World refusal remains.

The obsolete bound-room Classic aircraft-training refusal is renamed to
`aircraft_purchase_and_training_pay_once_and_survive_bound_room_delivery` and
replaced by the incoming paid FIFO completion check. Every detailed original
Conquest purchase, metadata, private aircraft-row and multi-tick assertion stays.

The active ordinary delivered adapter receives public initial terrain, delivered
mines, wall-work and admitted ground-flight capability, just as the incoming
adapter does. Its public terrain comes from the already-redacted startup map;
no hidden authoritative terrain is read. Stationary and mine/engineering facts
are completed only from those public/recipient inputs. The former false
capability assertion now verifies the complete receiver is enabled.

## Cross-chain #423 (supersedes the provisional two-owner production layout)

Exact parent `4453acf5b95fe5303278ef21b80736fa70c276c7` already contains the
explicit ordinary/Horde delivered-frame owner unification required by the
handoff's merge list. Applying that change means production now uses one
room-delivery owner for all admitted AI seats, including Horde's internal enemy.
The earlier logistics-only adapter is retained only under cfg(test), with all its
original tests, so it cannot run a second scheduler in a live room. The incoming
combined-role tests and existing ordinary tests both remain.

The unified owner receives only the canonical selected start/frame snapshots;
Horde's internal seat is activated at Normal difficulty and included in the
initial selected capture. It retains the incoming scheduled-frame alert reset
and queue-full behavior. Independent inactive human helpers still reset on the
existing seated-token takeover. No new takeover rule is introduced.

The current ordinary adapter additions (delivered strikes, stationary history,
public initial terrain, mines, wall work and ground-flight capability) compose
with the incoming owner rather than disappearing through a same-path conflict.
The incoming full-map spectator contact helper retains current-pose projection
and operative identity validation; all existing full-map flight tests remain.

Final behavioral verification must cover this explicitly requested cross-chain
change; the earlier standalone Horde chain pass is not proof of equivalence.

## Cross-chain #441

Exact parent `b5c269f3fcc050a66f62830c4808b867646ea1df`. Retains operative pause
notices, combined pause/checkpoint proofs and canonical Horde gate validation.
The incoming adversarial foreign-packet privacy assertions are merged into the
active ordinary view suite. Strict compiled-name counter/reason codecs retain
all valid-reason roundtrip checks. The populated Horde delivered-intent save
witness remains rather than reverting to an empty field.

All wider admitted-mode, pending-start, complete logistics and typed authored-v2
guards remain. Existing duplicate test registrations and obsolete copies of
legacy mode guards are not reintroduced. The detached comparison implementation
nested in the incoming view file is test-only; its original tests remain.

## Cross-chain #426

Exact parent `df6c1f9e377f5e8fc079194aec1a9870be2c10ad`. Retains hosted Horde saves,
records/results, complete ending holds, fresh restart seeds and deterministic
recorded weather seeds. Horde save/load is composed with the already-admitted
ordinary modes and typed authored-v2 owner rather than narrowing those modes.

The #423 unified production owner remains. Incoming split-owner helper files are
represented by the existing expanded delivered-owner suite; their duplicate
source copies are not separate production selectors. Horde save/result tests now
inspect the same active owner through ai_commander/ai_decoded_view. Director
memory uses #426's fresh-from-recipient fallback; roster AI memory still restores.
All brain-freeze, saved-clock, queued-order, aircraft/event and ending assertions
remain. The canonical start capture includes the internal seat after restoration.

Async generation keeps private edited map capture, but defers map loading and
constructor errors until generation, preserving failed-request rollback. Existing
positive Tutorial/sandbox and host/token guards remain. New failed-restart,
aviation privacy, support-retirement/cooldown, stock precision and binary Horde
witnesses are retained in addition to the stronger current suites. The seeded
long-run test keeps its exact 3,600-tick/every-seat assertions and adds repeat
input, recipient and RNG equality from the incoming branch.

## Cross-chain #432

Exact parent `bcdb6542c98c9673133fd45ff639ba61bad24a8a`. Adds the full Tutorial
result/hold and host-End witnesses while retaining existing operative and
elimination progress tests. Literal Tutorial goals, mode/end/out fields are
checked on each captured Tutorial frame. Human-only Tutorial startup guards are
retained; its unattended Enemy is script-driven, not a newly admitted AI seat.
Horde's independently admitted spectator path is not narrowed.

The existing AI checkpoint test gains the incoming legacy field/World-memory
fallback checks without losing its full-memory, random-stream and valid-reason
assertions. Both detached and active delivered adapters are checked against the
same owner-private control facts. Typed authored-v2 events and every existing
operative/Sandbox flight witness remain. The selected receiver and common mode
row remain the single projection path.

## Cross-chain #430

Merged exact `eeca34057f33cd61aef0136e007e59d156f36489`. Canonical queued command-union admission replaces the older Conquest-only ingress slice; core prospective guards remain authoritative. Retained every existing selected-owner, authored-v2, startup, snapshot and privacy guard. Incoming grenade validation is composed with retired-operative identities and current-pose recipient contacts. Added the incoming standalone support witnesses and preserved the six-tick mode delivery assertions across all admitted modes. The deliberately replaced refusal tests and their positive canonical replacements are listed in the integration changelog. Combined compilation and full qualification remain pending.

## Cross-chain #411

Merged exact `ea9aa761636f5a3ed2a983232958633bed9e1e68`. Retained immediate construction, sandbox, arrival and group-placement caller tests alongside all authored runtime/clock tests. The integrated current-pose flight/contact and recipient owners already subsume the incoming older flight adapter. No startup or complete-delivery mode was narrowed. Full validation remains pending.

## Cross-chain #419

Merged exact `e4c0801df68f886c1f019c680f7e6da304d9cffa`. The scenario actuator now initializes physical reserves on its canonical publication path and admits the incoming ordinary modes. Replaced the old Classic recipient-only group refusal with full validated ordinary scenario composition. Preserved old/new scene helper capabilities, all trigger-budget tests, Classic ghost delivery, closed World/Horde/Tutorial authored admission and the broader complete mode projections. Explicit constructor weather seeds are composed into asynchronous generation without synchronous map parsing or duplicate entropy reads. Full validation remains pending.

## Cross-chain #442

Merged exact `6a9621eff16002c29366bf9fdece3b7370d02ddc`. The complete authoring query implementation and C-entry witnesses are composed with additive ABI 2.2, all admitted room modes, one production delivered-AI owner and the reconciled required-clock authored-v2 schema. Reintroduced older experimental authored-mission compatibility was rejected in favor of the recorded save-owner decision, retaining its explicit refusal tests and the new ordered translation binding tests. Canonical engineering job memory is preserved and round-trip witnesses use its actual schema; the older competing scheduler was not restored. Incoming detached terrain ground is also forwarded into the production delivered-view field facts. Full combined qualification remains pending.
