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
A fort captured from a player losing their last region survives that elimination.

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
