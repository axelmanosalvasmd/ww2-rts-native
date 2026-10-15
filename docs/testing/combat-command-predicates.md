# Combat command predicates on current main

This is an unexecuted source candidate for CMB-82/87/88/89/93. This successor integrates frozen first candidate `733081885926fc8d5429f9d26c297ecb45dc6c44` onto fetched PR271 main `f7c51b1a0ebf850c5c22b560a595e4095ce39fdc`, with reference `e4e579a838aabae3b1d9dadbc17a0a87147bc52d` read-only. No row or Acceptance cell changes. Historical results and failures retain the producing heads in [the evidence record](combat-first-five-evidence.md).

## Inputs and consumers

`harness/combat-command-predicates.mjs` calls the pinned constructor, command dispatcher and complete step for two independent scenes. Both use the authored 64-by-48-cell flat map, two ordinary opposing Standard armies, Conquest, tech/supply enabled, logistics/sandbox disabled and clear living weather. Each plans 1,000 ticks and 1,001 observations, seed701, layout [0,1] and the complete finite 16,384-value tape. The tape is external input, never an outcome model. Global consumers retain their actual index, label and IEEE bits. Every wrapped environment consumer calls the original generator before taking the shared fraction. Unmapped Source global draws and tape exhaustion fail without fallback.

Each engine constructs its own actors, buildings, terrain and sidecars. Full-tick frames compare every live actor's identity, owner, kind, pose, HP, target, attack intent, firing timers, last-shot stamp and both actual visibility memberships. Every frame also compares cumulative draw consumption and actual environment seed32, wind, rain, wetness and shower state. Final receipts compare every draw, capacity/consumption/remainder/exhaustion, startup and the full layout receipt. No comparison default or absent-field mapping is applied to this focused projection. The existing broader startup observer and all its assertions remain independent and unchanged.

Source commands execute before the next `step`, in canonical tick/issuer/sequence order. Native parses each same command through `Command::from_json`, enqueues its `OrderKey`, then calls only `advance_tick`. Both use the previous completed clock for admission. Every native report must contain exactly the expected keys and denials for that boundary and leave the queue empty. There are no direct native phase calls in the new scenes. The child registration is the only change to the shared `aligned_replay.rs`; its startup registration and every incoming provider/hook remain intact. This test-consumer overlap was declared before mutation. No shipping provider, public API, wire, ABI, privacy or client contract changes.

## Ready replacement

Recorded Move commands stage natural rifles1/4/5 at (30,48)/(50,48)/(54,52) while Hold Fire and Hold Position are on, Autocast and Auto Retreat off. At tick701, ordinary Stance enables shooter1's automatic fire. Its first real volley must physically damage a still-living enemy. The recorder observes the existing reload until the next ordinary timer decrement makes the weapon ready while its target-search timer remains above0.05. The enemy owner's real Destroy command then invalidates that current target. The unchanged firing phase must select the other enemy and emit a physical flight at this exact acquisition tick. Within four more full ticks (0.2 seconds), that replacement must lose HP while the shooter's last-shot stamp still identifies the acquisition volley.

This is owner-command invalidation. It does not assert that a bullet killed the first target. The native consumer independently checks the live damaged target, naturally ready reload, positive search timer, exact Destroy issuer/selection, acquisition-tick `Flight` event/time and prompt replacement damage. Neither witness assigns HP, cooldown, target, actor pose or randomness. Existing selected-retarget still separately covers natural physical death and delayed initial acquisition, with its honest cooling-at-replacement qualification.

## Purchased medic visibility

At tick2, the recorder chooses a real completed constructor barracks whose catalog makes list contains medic. Ordinary Buy spends120MP and publishes one actual medic. At tick3, Stop cancels its ordinary initial route and Attack against the naturally unseen enemy4 must return Unseen. Ordinary enemy Move starts at tick4; at tick700, actual produced visibility must admit the target check and the unarmed medic's second Attack must return QueueFull. Source records the immediate payment before that tick's income. Native independently checks the same pre-buy wallet, actual completed maker, recruitment metadata binding and exact120MP spending-counter delta, then the purchased medic identity and visibility at both Attack boundaries.

## Complete original literal predicates

All Source paths below refer to the pinned reference. These are coverage bindings and pending assertions, not new passes.

| Row | Original assertion or rule | Existing or new consumer and evidence limit |
| --- | --- | --- |
|82|test.js7925-7928: patient heals by more than40HP over200 ticks; MP change is only income.|`match_recovery::tests::represented_recovery_phases_match_original_source`, cases medic-grid-bucket-tie-and-wound-history and medic-hot-calm-exact-boundary, plus `idle_medic_follows_its_real_route_then_heals_without_spending_manpower` keep exact component rates, no-wallet-spend and positive healing. Their two passes belong to b4ae, not this head.|
|82|test.js7929-7930: recent damage heals at half rate.|Existing medic-hot-calm-exact-boundary compares wound history and the exact five-second boundary. Replay pass remains at b4ae.|
|82|test.js7931: the medic's Attack against an unseen enemy returns Unseen.|New medic-visibility scene exercises that actual input and both real visibility producers. UNRUN. The old all-visible continuation returns QueueFull and is preserved as that control.|
|82|test.js7933-7934: idle medic walks into10m over400 ticks and patient exceeds60HP.|Existing unchanged recorded seek path plus400 native movement/recovery ticks, b4ae PASS. This is component-plus-route evidence, not a newly aligned Source continuation.|
|82|sim.js4947-4968,5654: ten-tick cadence, most-hurt ally, full-health/provision cap, idle90%/30m boundaries and retreat/rider exclusions.|Existing captured-cadence-heals-ten-ticks, medic-grid-bucket-tie-and-wound-history, medic-provision-and-shortage-limits, medic-idle-pathseek-and-exclusions and medic-near-seek-and-hurt-boundaries all remain in the complete recovery replay. b4ae attribution.|
|87|test.js2575-2578: no target before entry, then no damage before the scheduled search.|Authentic selected-retarget's initially hidden input, visibility at boundary2 and target0 before tick10, replayed in Default/Conquest.715 PASS. This is an explicit component visibility input, not physical full-match arrival.|
|87|test.js2580-2581: after0.6s, first target acquired and physically damaged.|Selected-retarget acquires by12, physical damage kills the first target14. Complete80-boundary replay;715 PASS.|
|87|test.js2582-2584: a dead current target is replaced without waiting for search.|Selected-retarget replaces at15 after physical death14. New ready-replacement independently uses a real owner Destroy while the search timer remains positive. First is715 PASS; second UNRUN.|
|87|test.js2585: a ready weapon physically launches on that acquisition tick.|New ready-replacement requires Source physical flight and native matching `Flight` at the exact tick/time. UNRUN. Old selected tick15 shots is empty; its cooldown is never relabeled ready.|
|87|test.js2586-2587: that replacement volley damages its target within0.2s.|New ready-replacement checks HP four ticks after acquisition and prohibits a later volley from supplying the damage. UNRUN. Old selected proves eventual damage only.|
|87|sim.js5569-5570: an explicit ordered target survives automatic alternatives.|Authentic selected-ordered sends actual ordinary Attack and applies the existing ground-attack consumer before firing. Farther target2 beats nearer3 in both aliases.715 PASS.|
|88|test.js1324-1326: all six rifles select one of two targets, weak gets at least one, next at least two.|Selected-spread-six runs all six actual firing consumers; exact claim rebuild, target choices, complete firing snapshots and Combat stream match in both aliases.715 PASS.|
|89|named test test.js:2585 (actual2610-2616): mortar prefers dug rifle28m away over open rifle17m away.|Original choice-dug input retains a trench under the farther target, exact Source target assertion and complete native snapshot replay in both aliases. Original131 remains unchanged;715 PASS.|
|89|test.js3282-3283: equal visible clusters choose the first target.|Selected-crowd-hidden before the neighbor becomes visible, complete firing comparison;715 PASS.|
|89|test.js3284-3285: an unseen neighbor changes nothing.|Selected-crowd-hidden has the real fourth actor excluded from the explicit visible set;715 PASS.|
|89|test.js3286-3287: making that neighbor visible changes selection.|Selected-crowd-visible differs in visibility membership, selects the other cluster;715 PASS.|
|89|test.js3288-3289: a passenger cannot enlarge the cluster.|Selected-crowd-passenger uses actual carrier5/cargo[4] and reciprocal canonical carrier metadata; visible rider4 is excluded.715 PASS.|
|93|test.js2763-2765: idle same-owner squads initially0.5m apart separate to at least2.75m after4s.|Existing idle-spacing Source reaction snapshot plus80 chronological native movement ticks; younger moves, older stays, spacing threshold holds. Two reaction passes belong to522.|
|93|sim.js4114-4122: newer squad, once-per-second reaction limit, shelter never worsens.|Existing source_reactions_retreat_and_spacing_match_canonical_state and younger_idle_squad_follows_spacing_route_without_losing_shelter retain exact path, react1, repeated-call refusal, shelter rank and unchanged random snapshot.522 attribution.|

The existing original test's end-spot formation half belongs outside CMB-93's idle-spacing scope; the tank-facing half of the spread check belongs outside CMB-88. No held CMB84/85/86/90/92 or ALIGNED83/91/140/144/145 work enters this candidate. Held b955 paid-Flak/recon and bc63 later callers remain on their preserved refs.

## Frozen Source-first selection, requiring a new grant

All executable work remains frozen during target cleanup. No free slot is a grant. Root must name this candidate's exact head, current selection and exclusive slot/resource wrapper before execution. The next scope is Source only, in this order, stopping on the first substantive failure:

```sh
node --experimental-vm-modules harness/combat-command-predicates.mjs --reference PINNED_READ_ONLY_CHECKOUT --output NEW_READY_TRACE --seed 701 --scene ready-replacement
node --experimental-vm-modules harness/combat-command-predicates.mjs --reference PINNED_READ_ONLY_CHECKOUT --output NEW_READY_TRACE --seed 701 --scene ready-replacement --check
node --experimental-vm-modules harness/combat-command-predicates.mjs --reference PINNED_READ_ONLY_CHECKOUT --output NEW_MEDIC_TRACE --seed 701 --scene medic-visibility
node --experimental-vm-modules harness/combat-command-predicates.mjs --reference PINNED_READ_ONLY_CHECKOUT --output NEW_MEDIC_TRACE --seed 701 --scene medic-visibility --check
```

Use x64 Node24.21.0. Output paths must be new, distinct, outside all checkouts and inside the granted external evidence target. Generation refuses an existing output path before engine loading. Regeneration requires the same producing head and compares the complete trace without rewriting its head attribution. Do not repeat original131/selected6 recorders or any old passing/failed job. Preserve full command/exit logs, input/source hashes, raw failure trace, all draw/startup/environment receipts and actual machine/resource limits. All processes must terminate and the slot must be released before reporting a failure. There is no silent input/expected-state repair or rerun.

If both authentic scenes generate and check successfully, freeze those outputs and provenance with a new fragment and DESIGN update before asking Root for a separate native scope. The two ignored exact names are:

- `aligned_replay::combat_command_predicates::ready_weapon_replaces_real_destroyed_target_and_launches_on_acquisition_tick`, reading `WW2_COMBAT_READY_TRACE`.
- `aligned_replay::combat_command_predicates::purchased_medic_attack_uses_actual_unseen_and_visible_producers`, reading `WW2_COMBAT_MEDIC_TRACE`.

No native, fmt, lint, discovery, CI, full-match gate, push or PR action is authorized by this document. Native first-observation mismatches print the actual input receipt; successful consumers print their complete native receipt before final equality checks. Build outputs may use only Root's granted external target. Root owns independent final review, hosted checks, Hermes readback and publication after269. This one bounded authoring task ends after reporting the frozen source head.
