# FIX-302 round 3 merge resolutions

Merge commit: `92d559a658ea6f9d00d4b2744d52690a5e56e5c2`.
Parents: reviewed movement head `c21c1551114b45324a2de6a5296fdf7a81c80a38` and fetched main `782a930128ffb9635b5bc7c804ba207a1f9d89ae`.

The merge had 10 conflict regions in nine combined diff hunks across five files. These resolutions preserve main's loaded-carrier scuttling and the movement branch's boarding, riding and wreck exits.

1. `core/src/match_carrier_tick.rs`, parent lookup and reciprocal cargo (one hunk, two conflict regions): keep the movement branch's bound parent and require reciprocal cargo only for a live carrier. Keep main's subsequent `prepare_carrier_ejection` preflight, including before a live carrier can be scuttled during dispatch. Missing or dead carriers retain their physical exit path.
2. `core/src/match_carriers.rs`, outcome enum: adopt main's `RidingOutcome::Ejected(Hurt)` and its compatible derives. The result carries the actual passenger, owner, exit position and killed flag.
3. `core/src/match_carriers.rs`, preparation helper: retain main's complete `prepare_carrier_ejection`, with read-only passenger validation, nearest free placement, shore reach, eviction injury and finite-health checks. Remove the movement branch's duplicate `eject_wrecked_rider` implementation. Manual unloading keeps its separate behavior.
4. `core/src/match_carriers.rs`, riding/ejection branch: retain main's existing-carrier validation and structured hurt return, with the movement branch's live-only reciprocal cargo requirement. Dead or missing carriers use the shared preparation helper, clear both relations, set position/health and update the grid. Live riding and boarding behavior remains intact.
5. `core/src/match_carriers/tests.rs`, dead-carrier case: retain the movement branch's successful ejection, riding reset and cargo cleanup assertions, matching the new `Ejected(Hurt { killed: false, .. })` result. Adapt the two nonconflicting MOV-76 injury/drowning outcome assertions to the same payload shape without changing their health or relation checks.
6. `core/src/match_tick.rs`, ejection event: publish main's supplied hurt payload directly, and retain the movement branch's killed-passenger `continue`. Surviving passengers continue their ordinary ground turn.
7. `simtest/tests/match_tick.rs`, rejected actor list: retain main's `noninfantry riding` case.
8. `simtest/tests/match_tick.rs`, rejected actor setup: retain main's tank riding a nonexistent carrier, so noninfantry riding remains rejected before mutations.
9. `simtest/tests/match_tick.rs`, rejection reason: retain main's `unsupported riding tick actor` expectation and the movement branch's `unregistered World train` expectation for its unsupported rail setup.

Main's `match_destroy.rs` stays byte-identical to the fetched main, including ordinary-tick scuttling tests for survival, fatal injury, stranding and an already removed carrier. A separate read-only source review accepted the carrier conflict resolutions with zero executions or edits. Full workspace validation is reported separately for the final runtime head.
