# FIX-299 round 3 merge and delivery repairs

Merged main `b3a55606b3415bc0baa5e74771e89fc889066b22` into PR head `dc277ef4da487e6fa51b41841906061387b84233` without rebasing.

Hand-edited merge hunks:

1. `rust/crates/core/src/match_firing.rs`, immediately after target selection: retain main's complete `fire_breath` implementation and the PR's `pub(crate)` visibility on `fire_weapon`. Horde breath and aircraft firing keep their separate callers.
2. `rust/crates/core/src/match_firing.rs`, `weapon_range`: retain main's implementation, with the PR's `pub(crate)` visibility.
3. `rust/crates/core/src/match_firing_geometry.rs`: remove the duplicate `weapon_range` and describe the remaining aircraft geometry helpers. Both implementations restrict the multiplier to MG, AT, mortar, flak and howitzer on a registered completed pit whose current cell is `T`. Both read `FORTS.nest.range`. Only their missing-rule error text differs. Prototype `shared/sim.js:445-447` at `e4e579a` uses the same `inPit` eligibility and multiplier.

All other incoming hunks merged automatically. No reference, contract, parity status or guard was weakened. The five previously approved obsolete-test rewrites are retained.

## Aircraft delivery boundary

Acknowledged issue #5 comments 6213230333 and 6214498302: parked/state-only aircraft delivery does not approve live projectile flights or aviation events. PR #338 also keeps plane-support arrival events out of rooms. No issue post is made under this task's explicit restriction.

`RoomRuntime::new` restricts aircraft weapon emission in the canonical `TickState`. `step_aircraft_turn` still performs movement, then skips weapons for that restricted owner. The standalone core keeps its aircraft firing provider and tests, including the ordered-target and bomber-stick tests for CMB-67. The room test supplies actual attack/area missions with Hold Fire off, observes a firing opportunity and requires unchanged ammunition and empty flight/salvo collections through a real income-based victory. Every seat receives a recipient snapshot on every tick. No VP, ownership or finish is injected after startup. Recipient `ShotsAndFlights` and aircraft-kind event refusals remain intact.

All 24 requested native core row claims are retained. Aircraft projectile delivery is excluded: waits on owner decision E (projectile flights). Public R1 aircraft orders remain unavailable. No new wire shape, owner decision, parity status or public ingress capability is introduced.

## Original field-work failure

The original objection is preserved: round 2 changed fixture setup rather than production field-work admission. Hosted run `38865018989`, test `unsupported_mixed_aircraft_selections_reject_before_supply_or_control_writes`, panicked at `match_aviation.rs:397` with `unrepresented or inconsistent field-work activity`. The recorder's `harness/match-aircraft-command-fixture.mjs:19` creates a bare `entrench:{jobs:[]}` without registering a project. The incoming native guard at `dc277ef4:match_commands.rs:1178-1182` requires `dig` to match active work and `entrench` to match project membership. Prototype `shared/sim.js:3677-3679` registers the project, and `:3352-3357` joins the unit and assigns the same project. The old fixture was malformed; the consistency guard remains unchanged.

A separate canonical mixed Attack refusal was found while checking that counterfactual. At the incoming head, `match_commands.rs:1050-1055` rejects a ground target whenever any selected actor is ground. Prototype `shared/sim.js:3495-3508` accepts eligible ground and aircraft actors together and cancels entrenchment. Round 2's aircraft-attack fixture changes the ground row to enemy ownership, so it does not cover that valid mixed selection. The repair first prepares aircraft orders read-only, then calls the existing ground Attack transaction on the raw ground rows, then applies the prepared aircraft orders infallibly. Missing raw ground controls are retained for validation. The new `mixed_attack_cancels_canonical_project_and_preflights_every_selected_actor` test creates a project through Entrench command admission, checks both selection orders and verifies cancellation and target assignment for both ground squads and the aircraft. It also retains complete state on four invalid cases: flag/membership mismatch, exhausted ground path identity, missing selected ground control and missing aircraft runtime. Public R1 admission is unchanged.

## Validation record

The first authored room witness refused its Attack mission before any tick because the relocated target had not been passed through visibility refresh. Refresh the actual native vision provider before mission admission; do not mark the target visible directly. The corrected focused room test passed once in ci-test profile (one Rust test, attacker and bomber cases). A separate attempted broad launcher from the repository root failed in rustup before Cargo or any test ran; subsequent Cargo commands run from `rust/`.

The first broad selection found an incoming #331 fixture conflict: `aircraft_owner_rows_and_plans_match_wire_codes_without_leaking_to_allies_or_enemies` assigned Area to a fighter, and the retained aircraft guard rejected it as Rules. Use a bomber for that matrix because Area requires its salvo weapon. All five states, four mission/plan codes, three recipient privacy checks, hidden/reveal checks and numeric row assertions remain unchanged. Existing non-bomber Area refusal tests stay intact. This is a post-merge fixture repair, not a relaxed runtime guard.

Before the final main refresh, the corrected three-package focused selection passed 218 tests (1,074 skipped). Formatting and strict locked workspace/all-target Clippy passed from `rust/`. The earlier failed selection remains separate: 174 passed, one invalid snapshot fixture failed and 43 selected tests were not run. Every invocation used one S1/S2/S3 lock, its target directory, a 6 GiB memory cap and no swap; each slot was released immediately afterward.

Final refresh: merge main `779681580cc947508665c134092220633fbb1d38` into repair commit `d65eaff7`. This merge is automatic, including DESIGN.md, with no hand-edited source or conflict hunk. The earlier `b3a55606` reconciliation above remains the complete hand-edit record.
