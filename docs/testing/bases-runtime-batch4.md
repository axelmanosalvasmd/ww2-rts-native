# Bases runtime batch 4

BASE-47 is the only counted row. BASE-27 and BASE-54 remain weak and uncounted.

## World scope after merging main

PR #336 keeps main's World runtime from #301, including bounded fort-owner
transfers before elimination and finish. World tick preflight, construction,
Gate ticks, recipient snapshots and hosted admission remain guarded.

Main's regression tests remain unchanged:
`world_epilogue::late_fort_transfer_preserves_decided_result_story_and_capture_timestamps`
and
`world_phases::fort_transfer_matches_split_phases_and_logistics_refusal_keeps_income_boundaries`.
The existing `world_finish` and `world_regions` oracle tests retain main's
support-marker handling and assertions.

The test-only `base54_world_fort_ranks_are_confined_to_source_shares` checks the
constructor already on main. The former
`base54_fort_capture_refusal_keeps_elimination_atomic` expected the component's
withdrawn fort-transfer refusal. It is now
`base54_elimination_construction_plan_refusal_keeps_fort_capture_atomic`: it uses
the existing damaged fort and construction-plan blocker, calls the combined
income, capture and elimination phase, and requires the exact construction-plan
refusal. Every original assertion about units, pace bits, region state, wallets
and fort-footprint owners remains. BASE-54 stays uncounted.

The prototype transfers damaged forts in `shared/sim.js:1197-1204` at `e4e579a`.
No prototype run is claimed by this repair.

BASE-57 remains dropped: accepting a recorded convoy's return to an obstructed
source through its accessible service edge needs an owner decision.

## Building placement and tech switch

`building_placement::check` ports the building branch of pinned
`shared/sim.js:3277-3313` and the stamp predicates at `:1005-1019`. Ordinary Build
uses this query. BASE-27 component checks retain literal footprint cell order,
snapped centres, half rounding, refusal precedence, complete World-cell sight,
territory, rail-yard exclusion, node overlap, coastal distance, grade and living
ground occupancy. Equal-distance nodes keep source input order. A real ordinary
Build test compares preview reasons with `blocked`, `notVisible` and `coast`
denials, verifies no payment or birth, then advances four ticks with every-seat
snapshots. World territory and rail reasons use borrowed component facts because
World commands and delivery remain closed. No client/FFI preview adapter is added.

BASE-47 tests execute the actual Conquest tech switch, initial tier 1 and empty
Armory/labs, tier refusal, paid Mortar birth and Motor Pool site when tech is off,
blocked research, six ordinary ticks and delivered snapshots for every seat.
Another constructor test checks both Horde defenders at tier 2 and the internal
enemy at tier 3, each with zero Armory levels and empty labs. Horde full ticking
and snapshots remain guarded. Other-mode startup stays with its owning stream.

BASE-12 has partial query evidence only: nearest free node within 8 m, stable ties,
snapping, claimed/hidden refusal and exclusion of other buildings. Paid Depot
claiming and cancellation/destruction release are not claimed.

## Remaining row boundaries

- BASE-10: Classic node generation depends on the Classic startup in PR #301.
- BASE-12: paid Depot claim/release needs construction-mode admission.
- BASE-14: construction-mode support admission and delivery remain unavailable.
- BASE-18: complete skirmish-mode and aircraft recruitment need the mode and aircraft streams.
- BASE-19: Tutorial recruitment belongs to PR #287.
- BASE-22: owner job transport and construction-mode command admission need their existing contracts/callers.
- BASE-24: shared Horde rally and assistance overlap PR #295; full Horde ticking stays guarded.
- BASE-26: paid Depot binding needs construction-mode admission.
- BASE-29, BASE-30, BASE-31, BASE-33, BASE-48: partial runtime work belongs to PR #295.
- BASE-38: the World paratrooper support command remains unadmitted.
- BASE-52, BASE-53, BASE-56: all-mode startup, fortification and Classic clock work overlap PR #301.
- BASE-59: Classic owner production snapshots require recipient/transport work outside this stream.

No Source recorder ran. All new placement facts are transient query inputs or
outputs. This branch adds no persistent field; bounded fort ownership follows main. The
complete-logistics tick and snapshot guards remain closed. BASE-01, BASE-02, BASE-46, BASE-55 and BASE-57 are
outside the continuation's ownership.
