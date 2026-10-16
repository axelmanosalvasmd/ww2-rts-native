# MOV_IMPL native intent batch 1

Base: `40fe9e296b46060241693c6c627ecd615d4f8ea4` (`origin/main` when this branch was created). Reference: `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`.

The old `9080dacd` branch has 57 passing but partial witnesses and seven ignored missing-actuator cases. All 64 owned rows were checked against REVIEWER_1's 2026-10-16 expectations and the pinned prototype. This batch strengthens 11 selected witnesses. The other 46 partial witnesses are dropped from this PR; the seven missing actuators remain NEXT. Historical execution does not qualify this branch.

Only the new test module, its `cfg(test)` registration and test documentation are extracted after `cfcda299`. The private movement appendix and unpublished runtime ancestry are excluded. Authored scenarios call existing native consumers; no behavior Source recorder, captured fixture or expected-output installation is added. Public runtime and parity status tables remain unchanged. The source review is a coverage check, not both-owner acceptance.

## Selected coverage

- MOV-41: `mov_41_retry_cap_resets_on_new_command_or_observed_knowledge`. Real disconnected route searches reach the retry cap. A fresh command and a changed observed knowledge version each restore a full allowance for the same impossible goal. Prototype: `shared/sim.js:3086`, `shared/sim.js:3217-3228`.
- MOV-44: `mov_44_nearest_free_masks_bfs_clamping_and_origin_region`. Exact east/west/south/north BFS cells, two-hop ties, both clamped corners, unrelated flag bits, origin-region restriction and all-blocked fallback for all four movement masks. Prototype: `shared/sim.js:2884-2905`.
- MOV-47: `mov_47_idle_spacing_preserves_cover_and_respects_reaction_and_target`. The newer own squad spaces itself on open/covered ground. Isolated cover prevents worse-ground placement; active reaction and target states prevent spacing. Prototype: `shared/sim.js:4112-4121`, `shared/sim.js:135`.
- MOV-50: `mov_50_displaced_world_guard_returns_and_drops_route`. A native local guard stays at the inclusive 16 m boundary, then snaps home beyond it and clears movement/attack intent. An ordinary player actor stays put. Prototype: `shared/sim.js:5348`.
- MOV-52: `mov_52_priority_tiers_age_cap_ties_and_traffic_durations`. Exact traffic constants, every priority tier, wait aging and cap, occupied-lane and ID ties, plus actual horizon, delayed yield/expiry, replan and ghost timing. Prototype: `shared/local-traffic.js:3-17`.
- MOV-63: `mov_63_bridge_and_ford_peers_keep_forward_progress`. Same-way bridge and ford squads keep forward displacement while separating sideways; an ordinary-ground control permits the normal backward separation. Prototype: `shared/sim.js:5191-5197`, `shared/sim.js:5215-5225`.
- MOV-64: `mov_64_held_workers_do_not_push_routed_friends`. Held, building, digging, entrenching and covered squads retain their positions and do not push routed friends backward. Prototype: `shared/sim.js:2407-2408`, `shared/sim.js:5226-5229`.
- MOV-72: `mov_72_carrier_tables_and_boarding_enforce_capacity`. All nine capacities and rider/reinforcement attributes match the pin. Real Board commands and boarding enforce the light tank, Tiger, halftrack and lorry seat limits. Prototype: `shared/sim.js:198`, `shared/sim.js:245-266`, `shared/sim.js:286`, `shared/sim.js:313-321`, `shared/sim.js:3524`.
- MOV-95: `mov_95_recon_delayed_rotated_unit_and_terrain_strip_is_shared_and_expires`. Delayed, rotated 80 by 30 m recon reveals units and terrain to the team, excludes outside boundaries/enemy knowledge and expires after its lifetime. Prototype: `shared/sim.js:400`, `shared/sim.js:4433`, `shared/sim.js:4600-4605`, `shared/sim.js:5269`, `shared/sim.js:5300`.
- MOV-97: `mov_97_rotated_strafe_arrives_once_with_friendly_damage_and_exclusions`. Delayed rotated 36 by 8 m strafe hits friendly/enemy infantry and vehicles once, adds suppression and excludes outside, aircraft and riding actors. Prototype: `shared/sim.js:402`, `shared/sim.js:5292-5297`.
- MOV-103: `mov_103_naval_shoals_are_ten_cardinal_cells_fixed_at_start_and_route_deep_ships`. Initial naval masks include exact ten-cell cardinal shoals. Shallow and deep routing differ, and terrain changes retain the startup shoal mask. Prototype: `shared/sim.js:171-180`, `shared/sim.js:740`, `shared/sim.js:2665-2678`.

## Dropped from this PR

These remain incomplete; no native runtime defect is inferred from a missing witness.

- MOV-01: Phase labels do not prove observable dependencies across the live tick. Prototype: `shared/sim.js:5305-5354`, `shared/sim.js:5585-5642`, `shared/sim.js:5672-5692`.
- MOV-02: Missing newborn same-tick action and removed-train iteration. Prototype: `shared/sim.js:1806`, `shared/sim.js:5342-5354`, `shared/sim.js:1298`, `shared/sim.js:5672-5678`.
- MOV-03: Missing complete per-unit reaction, recovery, fuel, terrain and targeting composition. Prototype: `shared/sim.js:5374-5387`, `shared/sim.js:5428-5447`, `shared/sim.js:5475-5582`.
- MOV-04: Missing pace, terrain, slope, weather, march, fuel and displacement controls. Prototype: `shared/sim.js:3817`, `shared/sim.js:5483-5486`.
- MOV-08: Missing calm boundaries, enemy-region and mode/type exclusions. Prototype: `shared/sim.js:117`, `shared/sim.js:2375-2381`.
- MOV-15: Missing rotated wall/cell overlaps using delivered hull dimensions. Prototype: `shared/vehicle-motion.js:15-41`.
- MOV-20: Missing rear-goal yielding and reverse reset when the endpoint changes. Prototype: `shared/movement.js:17`, `shared/sim.js:4071-4075`, `shared/sim.js:5477-5480`.
- MOV-21: Injected clearance bypasses automatic blocked-turn escape selection and expiry. Prototype: `shared/movement.js:18-38`.
- MOV-42: Missing multiple legs, World retreat threshold and delivered owner plans/outcomes. Prototype: `shared/sim.js:3190-3194`, `shared/sim.js:5349-5350`, `shared/sim.js:6017-6025`.
- MOV-45: Missing ninth queue attempt, coordinate/exit handling, reverse and plane dispatch. Prototype: `shared/sim.js:3364-3380`, `shared/sim.js:3421-3461`.
- MOV-46: Missing unreachable fallback, cover, Hold destination and hidden World controls. Prototype: `shared/sim.js:3950-3986`, `shared/sim.js:4019-4067`.
- MOV-48: Missing cancellation/exits, mode-specific home selection and plane return. Prototype: `shared/sim.js:1725-1733`, `shared/sim.js:3335-3343`, `shared/sim.js:5484`.
- MOV-51: Missing building rally, return-fire continuation and mode/type exclusions. Prototype: `shared/sim.js:1071-1077`, `shared/sim.js:3389-3392`, `shared/sim.js:4093-4097`, `shared/sim.js:5256-5263`.
- MOV-53: Missing steering consumer exclusions and heavy-body/filter controls. Prototype: `shared/local-traffic.js:18`, `shared/local-traffic.js:95-102`, `shared/sim.js:5490-5495`.
- MOV-54: Missing horizon/margin boundaries and earliest-time/lower-ID selection. Prototype: `shared/local-traffic.js:19-22`, `shared/local-traffic.js:103-122`.
- MOV-55: Missing waiting-chain cycle, sustained waits and ghost/replan exclusions. Prototype: `shared/local-traffic.js:23-39`, `shared/local-traffic.js:116-122`, `shared/local-traffic.js:168-169`.
- MOV-56: Missing occupied/nearer-middle priority and lane refresh on changed inputs. Prototype: `shared/local-traffic.js:44-92`, `shared/local-traffic.js:125-128`.
- MOV-57: Sequence was repaired historically, but candidate rejection, back/trail choice, expiry and trail spacing remain incomplete. Prototype: `shared/local-traffic.js:172-207`.
- MOV-58: Steering without movement does not complete passing legs or prove safe resumption/replan. Prototype: `shared/local-traffic.js:134-160`, `shared/local-traffic.js:184-195`.
- MOV-59: Missing stuck-leader give-way and complete friendly-gridlock recovery. Prototype: `shared/local-traffic.js:126-133`, `shared/local-traffic.js:171-201`.
- MOV-60: Missing expiry and friendly/enemy controls through traffic, separation and swept movement. Prototype: `shared/local-traffic.js:7-9`, `shared/local-traffic.js:129-132`, `shared/movement.js:11-12`, `shared/movement.js:53`, `shared/sim.js:1096-1099`, `shared/sim.js:5212`.
- MOV-61: Old test covers overlap separation, not hidden actor/terrain privacy. Prototype: `shared/sim.js:5487-5495`, `shared/sim.js:1636-1660`.
- MOV-67: Direct grid.update calls do not prove relocation consumers maintain membership. Prototype: `shared/sim.js:1769-1772`, `shared/sim.js:1295`, `shared/sim.js:2202`, `shared/sim.js:2420-2428`, `shared/sim.js:5261`.
- MOV-68: Missing indexed scatter, origin connectivity, naval placement and immediate grid publication. Prototype: `shared/sim.js:1773-1808`.
- MOV-69: Missing actual bounded spawn-exit ring-search caller. Prototype: `shared/sim.js:1080-1096`.
- MOV-71: Missing full crushing/mine/type/wet-ground and stationary movement-effect matrix. Prototype: `shared/sim.js:78-91`, `shared/sim.js:5513-5533`.
- MOV-73: Missing eligibility/reachability and boat bank/cliff/building controls. Prototype: `shared/sim.js:3520-3538`, `shared/sim.js:2441-2445`, `shared/sim.js:5441-5446`.
- MOV-74: Missing ordinary-action skipping through a live tick and recipient cargo/rider flags. Prototype: `shared/sim.js:3419`, `shared/sim.js:5355-5361`, `shared/sim.js:5881-5904`.
- MOV-75: Missing multi-rider nearest-free unload and naval shore admission/denial. Prototype: `shared/sim.js:2422-2430`, `shared/sim.js:3564-3570`.
- MOV-78: Missing exact pace/price and moving/range/team/strength exclusions. Prototype: `shared/sim.js:109`, `shared/sim.js:5651-5665`.
- MOV-84: Aircraft birth/base/rearm checks reached recipient qualification, but main rejects the snapshot with Unsupported Air. Prototype: `shared/sim.js:1804`, `shared/sim.js:2182-2189`, `shared/sim.js:2201-2204`.
- MOV-85: Missing return triggers, orbit/health restoration and state visibility/damage controls. Prototype: `shared/sim.js:2181-2234`, `shared/sim.js:3819-3822`, `shared/sim.js:5942-5945`.
- MOV-86: Missing complete mission commands, guards and explicit-target Hold-fire cases. Prototype: `shared/sim.js:2193-2196`, `shared/sim.js:2212-2234`, `shared/sim.js:3337`, `shared/sim.js:3442-3519`, `shared/sim.js:2242-2244`.
- MOV-89: Missing real-tick range/damage/exclusion and single shoot-down checks. Prototype: `shared/sim.js:216-230`, `shared/sim.js:279`, `shared/sim.js:2236-2251`, `shared/sim.js:5632`.
- MOV-90: Missing applicable blast/strafe/bridge immunity, no-wreck and population controls. Prototype: `shared/sim.js:535`, `shared/sim.js:2198-2226`, `shared/sim.js:2589`, `shared/sim.js:4733`, `shared/sim.js:5200`, `shared/sim.js:5295`, `shared/sim.js:5682`.
- MOV-91: Missing all support kinds, currencies, delays and direction/effect isolation. Prototype: `shared/sim.js:398-418`, `shared/sim.js:515-517`, `shared/sim.js:549`, `shared/sim.js:3680-3688`.
- MOV-92: Missing World recipient filtering and plane-arrival announcements. Prototype: `shared/sim.js:5247-5250`, `shared/sim.js:5935`, `shared/sim.js:6052`.
- MOV-94: Missing recon exemption, eligible flak, partial delivery and hidden identity checks. Prototype: `shared/sim.js:413`, `shared/sim.js:4692-4711`, `shared/sim.js:5247-5250`, `shared/sim.js:6080-6087`.
- MOV-99: Missing cadence, rotated strip, blast/crater and interception truncation. Prototype: `shared/sim.js:405`, `shared/sim.js:4710`, `shared/sim.js:5279-5284`.
- MOV-100: Missing delay/interception and configured blast/damage/crater boundaries. Prototype: `shared/sim.js:408`, `shared/sim.js:5247-5255`.
- MOV-101: Missing unseen/World admission and per-landing elimination/population guards. Prototype: `shared/sim.js:409`, `shared/sim.js:534-548`, `shared/sim.js:3683-3684`, `shared/sim.js:5256-5263`.
- MOV-104: Missing capacity/max tables and actual naval-only purchase/Shipyard launch. Prototype: `shared/sim.js:263-279`, `shared/sim.js:1071-1075`, `shared/sim.js:1773-1775`, `shared/sim.js:3781-3784`.
- MOV-105: Missing weather/terrain/march controls and contact distinctions. Prototype: `shared/movement.js:75-87`, `shared/sim.js:2377-2385`, `shared/sim.js:5212-5213`, `shared/sim.js:5490`.
- MOV-106: Standalone debris does not prove wreck creation, contacts, settlement terrain, overflow/eviction or grid updates. Prototype: `shared/debris-motion.js:2-58`, `shared/sim.js:5125-5182`, `shared/sim.js:5679-5692`.
- MOV-107: Producer-only motion fields do not prove native client interpolation or grounding. Prototype: `shared/sim.js:5884-5891`, `client/engine-presentation.js:137-166`.
- MOV-108: Static geometry does not prove stateful visual followers, reset conditions or selection/footprint bounds. Prototype: `client/squad-motion.js:1`, `client/squad-motion.js:15-77`.

## NEXT

MOV-70, MOV-76, MOV-77, MOV-81, MOV-82, MOV-87 and MOV-88 retain their missing-actuator boundary. Their ignored bodies remain on the private old branch and are not imported.

## Verification

On the Vostro, the final focused `ci-test` selection ran 11 tests: 11 passed, 149 skipped. `cargo fmt --all --check` passed. Strict workspace all-target Clippy passed with `-D warnings`. Commands run from `rust/`:

```text
cargo nextest run --profile ci --cargo-profile ci-test --locked -j4 -p ww2-core --lib -E 'test(mov_intent_batch::)'
cargo fmt --all --check
cargo clippy --locked --workspace --all-targets -j4 -- -D warnings
```

The first nine-test candidate ran seven passes and two failures. MOV-52's authored blocker lost priority as the waiting mover aged, bypassing the intended replan branch; a retreating blocker now keeps that conflict active, and the final test passes. MOV-84 reached its snapshot check and was refused with `Unsupported { unit: 1000, capability: Air }`; its full-row witness is dropped. Raw first and final logs remain at `/tmp/mov-impl-main-first-20261016.log` and `/tmp/mov-impl-final-tests-20261016.log` on the Vostro. No runtime defect is inferred from the authored MOV-52 setup.

Source review accepted the final 11 intents. The eight retained primary-module tests received an independent review; the three tail tests received author self-review and implementation-owner source review. Source review supplies no native or owner-acceptance result.

All Cargo invocations use an exclusive S1/S2/S3 lock, its target directory, a 6 GiB memory limit and zero swap after checking at least 20 GiB free disk. The final tests and format check used S2; strict Clippy used S3. All locks were released when each Cargo process exited. No Source execution was run. The task stops after opening one test PR; CI, status promotion and merge belong to the coordinator.

Issue #5 policy readback (including handover 6207924049 and readback 6207938018): owner decision 6206861267 and the test/status split recorded at 6207528476 are acknowledged. No interface change is proposed; draft contracts remain drafts. The exact-final-head runtime acknowledgment rule would apply only if shipping behavior changed.
