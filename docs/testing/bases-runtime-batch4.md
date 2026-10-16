# Bases runtime batch 4

BASE-54 World core transfers live Pillboxes, Towers, Walls and Gates to the captor
at their current HP. Source intent is `shared/sim.js:1197-1225` and `:1324`.
The clear, flat rural World test retains the native constructor actors. It requires
Pillboxes in exactly the farthest 40% of guard regions and Towers and wall runs in
exactly the farthest 15%, with fractional rank cutoffs rounded upward. It also
checks finished forts, the Pillbox bearing and 14 m target toward the nearest
home, and four walls on each side with the matching crosswise bearing.

Capture stages a validated fort-owner map before army elimination and finish
preparation. The transaction updates units and the spatial grid. Publication
updates only matching retained footprint entries and currently visible remembered
building owners. Missing footprint entries and replacement buildings are preserved.
A fort captured from a player losing their last region survives that elimination
and the resulting last-nation match finish.

The six `base54_` tests cover ranks, current HP and sections, footprint/grid/memory
owners, capture followed by ordinary World ticks, canonical Gate transfer,
previous-owner elimination, a replaced footprint entry and atomic refusal.
Gate ticks and World recipient snapshots remain unadmitted. The server's existing
attachment preflight rejects World before it can become a hosted room. No Gate
physics, World admission, ABI, wire or privacy policy is added.

BASE-57 is dropped from this batch. Its positive recorded-convoy return test
reached an obstructed source: the route endpoint was 4 m from the source centre,
and the existing convoy preflight returned `Unreachable`. Accepting a route to
the source's accessible service edge is a differs candidate, not part of this PR.
The exploratory home-selector code and tests were removed.

The Mac continuation ran all seven selected `base54_` checks successfully on the
incoming candidate, including all six World fort witnesses. Formatting and strict
Clippy found formatting changes and a collapsible capture conditional, corrected
in this continuation. The earlier Vostro slot failures remain historical; they do
not describe the Mac result. Final merged-head qualification is reported in the PR.

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
outputs. Existing fort ownership uses the established Unit, grid, footprint and
World-memory owners; no persistent field is added. The complete-logistics tick and
snapshot guards remain closed. BASE-01, BASE-02, BASE-46, BASE-55 and BASE-57 are
outside the continuation's ownership.
