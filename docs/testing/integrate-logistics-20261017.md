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
