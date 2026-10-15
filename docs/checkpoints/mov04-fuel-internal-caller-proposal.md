# MOV-04 fuel cap and internal caller proposal

Source-only proposal, with no runtime edits or execution. Root owns any future integration scope. MOV-04 stays held. BASE_AUDIT keeps sole production ownership of match_logistics.rs and LogisticsState; this document does not authorize another writer.

## Verified owners and current gap

Current main379 match_logistics.rs has blob `59f8c4734eb6869343533fc8a58d517e86c51099`, confirmed by BASE_AUDIT's direct readback. Its91f9bc12 child adds only three cfg(test) module-registration lines and causal_tests.rs;1045b096 adds BASE05 recorder/simtest work. Neither changes the production reserve consumer. The readback gives no objection to this MOV-owned source proposal, separately from Root's pending runtime authorization.

Match.logistics owns physical Reserves by authoritative unit ID. Reserves has optional fuel and emergency plus forced/stranded flags. Existing advance_unit_reserves calls tick_reserves with dt and travel0 before movement. Existing consume_unit_driving_fuel is the one post-travel debit; it preserves riding exclusion, ordinary-fuel-first consumption, forced/emergency transitions and stranding. Keep those functions and their ownership intact.

Pinned sim.js:5484-5486 computes all speed factors, then caps that final speed by min(1,(fuel+(forced?emergency:0))/dt). At5509 it charges min(dt,physicalDistance/max(finalCappedSpeed,0.001)). Native movement.rs:284-290 computes pace/order/terrain/slope/weather/march without that reserve cap. Moved.speed already returns the effective allowed speed. Native match_tick.rs:299-301 still refuses enabled logistics/convoys, and World ground also requires disabled logistics. No proposal here removes those guards or completes the whole row.

## Proposed internal data flow

| Stage | Read or write owner | Required behavior |
|---|---|---|
| Before the actual unit movement turn | Existing BASE reserve owner | Keep advance_unit_reserves(dt,travel0). It may change forced state before the cap is read. Never charge physical travel here. |
| At move_unit_with_command_routes for the live unit | Match adapter reads Match.logistics.get(id) | Read current fuel/emergency/forced after earlier callbacks, rather than a before-tick copy. No reserve object or fuelNone means factor1. Unforced emergency contributes nothing. An absent emergency cannot add fuel; canonical malformed forced reserve admission remains an owner decision, never an invented refill. |
| Within the private movement implementation | Movement owner | Add a crate-private bounded-speed input beneath the existing public entry points. Keep public signatures, C ABI, protocol, Unit layout and Rules/profile values unchanged. Existing no-reserve wrappers explicitly pass factor1. Apply the reserve factor after march and before steering/vehicle/walk calculations. Moved.speed reports the final capped value. |
| Immediately after actual physical travel | Match adapter calls the existing BASE debit | Use distance between the actual before/after unit poses and Moved.speed: min(dt,distance/max(speed,0.001)). Call consume_unit_driving_fuel exactly once. Riding and zero travel debit0. Do not include later crowd separation or contact displacement in powered distance. |
| Error and World boundaries | Root and existing phase owners | Preserve current partial-tick fault behavior, preflight order and World fast/trial-path semantics. Default internal factor1 applies while World logistics is refused. No World admission or enabled-logistics policy broadening follows from this seam. |

The proposed private seam needs both movement entry paths considered: move_unit_with_knowledge delegates to move_unit_with_march, while trial_move_with_march calls block directly. A future implementation must avoid silently applying different scalar order to trial versus committed motion. This is a design requirement, not permission to edit World or BASE providers now.

Use core math for every match-path min/max/hypot operation. Do not multiply suppression_speed by fuel: retreat and sprint select other order factors and would bypass that encoding. Do not modify base kind speed or shared Rules, because that changes other units and read sites. Do not use uncapped catalog speed for the debit. Keep the ordinary-first then emergency debit in the existing reserve owner.

## Proposed proof, not executed results

A future focused component/actual-caller capture needs: no-reserve and infantry-fuelNone controls; fuel above one tick; fractional fuel below one tick; zero ordinary fuel unforced despite emergency; forced ordinary plus emergency; zero total reserves; exactly enough fuel; and shortened/blocked travel. Record final allowed speed, physical distance, ordinary/emergency debit and forced/stranded state. Include a live-turn case where the existing reserve phase changes forced state before movement, to reject a stale before-tick input. Riding consumes0, stationary units consume0, and acceleration/turning/traffic may shorten travel without permitting a second charge.

Record every input and actual Source output through unchanged Source functions. Native must use canonical births and live internal movement/reserve owners with actual before/after poses. No synthetic actor, HP/pose/RNG reset, rule mutation, expected-state initialization, translated-model PASS or public ingress extension is part of this proof. Existing BASE LOG component tests stay intact and unconflicted.

The enabled-logistics whole-tick guard stays in place until Root separately assigns all missing caller phases and their evidence. A private component witness under that guard can prove the bounded consumer, but cannot by itself mark MOV-04 ported or claim general logistics ticks pass. No GATES command is granted by this document.
