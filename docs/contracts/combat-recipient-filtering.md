# Combat recipient filtering

This records the recipient rules at prototype `e4e579a` under owner decision P,
confirmed in [Axel's comment
6218552386](https://github.com/axelmanosalvasmd/ww2-rts-native/issues/5#issuecomment-6218552386).
It documents the existing JSON v0 shapes and the approved cached-path privacy
rule. It adds no ABI, C header, command or World room admission.

Unit rows retain the prototype columns from `shared/sim.js:5890-5896`:
`[id, type, owner, x, z, rot, aim, hp, supp, target, cover, cd, flags, stars, built, ...tail]`.
Recipients see allied units and visible enemies. Enemy planes are included only
while airborne, even when the visible set is stale. Match-end reveal includes
every existing unit (`shared/sim.js:5942-5946`). A locked target is zero unless
that recipient can see it. Cooldown is zero on every nonowned unit, including
allies. Nonowned flags retain exactly `2047 | 32768 | (7 << 20) | 262144`
(`shared/sim.js:5899-5905`). Reveal changes visibility, not cooldown or flag
privacy.

Both Rust cached and direct uncached projections use that one mask. The
prototype's uncached path retains additional gate, operative and cargo bits
(`shared/sim.js:6040-6044`). This difference is the already approved P decision,
not a new privacy proposal. NET-53 remains owned by PR #378.

Classic building ghosts use the prototype's six fields:
`[id, type, owner, x, z, built]`, with positions and built progress rounded to
tenths (`shared/sim.js:5928,6036-6037`). Every team shares last observed
building facts. Visible buildings are omitted from ghosts. Hidden live changes
and hidden destruction do not update memory. A removed building is forgotten
only when an allied ground observer can see its location or an airborne plane
revisits it. A grounded plane does not see it
(`shared/sim.js:4444-4448,4456`). The existing World memory component follows
the same rule; World room admission remains closed.

The internal Classic snapshot adapter now accepts the existing Classic core
state. This does not open Classic network startup or claim complete Classic
mode, economy or node delivery. Default and Conquest continue to omit ghosts.

NET-63 captures one set of immutable unit, owner and public rows per send, then
filters copies for each selected recipient (`shared/sim.js:5908-5939`). Terrain
and remembered-list publication consume the captured rows without rebuilding
them. Direct uncached projection builds from canonical units separately and
uses the same approved mask.

Evidence: `cmb134_recipient_visibility_targets_cooldowns_and_flags_follow_each_seat`,
`net63_cached_and_uncached_reads_equal_without_mutating_shared_or_returned_rows`,
`cmb130_classic_remembers_last_seen_buildings_until_an_airborne_observer_revisits`
and `cmb130_classic_ghost_rows_keep_the_exact_six_field_shape`.
