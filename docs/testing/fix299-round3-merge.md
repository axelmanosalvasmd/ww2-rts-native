# FIX-299 round 3 merge and delivery repairs

Merged main `b3a55606b3415bc0baa5e74771e89fc889066b22` into PR head `dc277ef4da487e6fa51b41841906061387b84233` without rebasing.

Hand-edited merge hunks:

1. `rust/crates/core/src/match_firing.rs`, immediately after target selection: retain main's complete `fire_breath` implementation and the PR's `pub(crate)` visibility on `fire_weapon`. Horde breath and aircraft firing keep their separate callers.
2. `rust/crates/core/src/match_firing.rs`, `weapon_range`: retain main's implementation, with `pub(crate)` visibility for the sibling geometry callers.
3. `rust/crates/core/src/match_firing_geometry.rs`: remove the duplicate `weapon_range`. Both implementations restrict the multiplier to MG, AT, mortar, flak and howitzer on a registered completed pit whose current cell is `T`. Both read `FORTS.nest.range`. Only their missing-rule error text differs. Prototype `shared/sim.js:445-447` at `e4e579a` uses the same `inPit` eligibility and multiplier.

All other incoming hunks merged automatically. No reference, contract, parity status or guard was weakened. The five previously approved obsolete-test rewrites are retained.
