# Bases runtime batch 4

BASE-47 is the only counted row. BASE-27 and BASE-54 remain weak and uncounted.

## World scope after the CI repair

PR #336 preserves main's World runtime. The proposed fort-owner transfer,
elimination exemption, finish-owner transition and removed tick preflight guard
are withdrawn. The World region, tick, elimination and finish implementations
are restored verbatim from main. World construction, Gate ticks, recipient
snapshots and hosted admission remain guarded.

Both reported regressions remain unchanged:
`world_epilogue::late_fort_refusal_preserves_decided_phase_and_the_original_terminal_owners`
and
`world_phases::combined_refusal_is_atomic_but_separate_income_is_an_explicit_committed_phase`.
The existing `world_finish` and `world_regions` oracle tests also regain main's
support-marker handling. Refusal and atomicity assertions remain intact.

The test-only `base54_world_fort_ranks_are_confined_to_source_shares` checks the
constructor already on main. `base54_fort_capture_refusal_keeps_elimination_atomic`
retains the atomicity assertions and uses a damaged fort. The four positive
transfer tests are removed with the deferred actuator: damaged-owner/footprint
transfer, Gate transfer, survival through previous-owner elimination, and
replacement-footprint preservation. These are not counted evidence.

The prototype transfers damaged forts in `shared/sim.js:1197-1204` at `e4e579a`.
This repair chooses case (a), preserving main's bounded World support rather than
changing expectations to admit more World behavior. No prototype run is claimed.

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
outputs. World fort ownership is unchanged; no persistent field is added. The
complete-logistics tick and snapshot guards remain closed. BASE-01, BASE-02, BASE-46, BASE-55 and BASE-57 are
outside the continuation's ownership.
