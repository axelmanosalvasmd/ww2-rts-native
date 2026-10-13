# Changelog

## Unreleased

- Expanded the firing source fixture to explicit area-weapon orders and limited/empty ammunition, covering every represented shooter kind. Rejects malformed timer and XP facts before unit firing changes. Native verification remains queued.

- Connected the Conquest flight driver to canonical suppression, unit/section damage and terminal terrain/blast effects, preserving contact order, launch delays and retained shooter XP. Added defined lower-ID collision ties, unsupported-mode guards and malformed-flight admission. Weapon updates now retain only scalar facts instead of cloning a unit and its route. The original-source recorder now covers 114 traces and 588 checkpoints with a 64,006-byte compressed fixture. Native differential, expiry and collision-tie checks are queued.

- Exposed the canonical target setter inside the core so reaction phases keep shared volley claims consistent when intercepting rally movement.
- Preserved terminal projectile blast direction through terrain damage so section debris receives the source impulse direction. Added explicit direction admission and an original-source directed-blast regression.

- Tightened public combat-event admission after independent review: negative suppression and unknown nonnegative last-hit owners reject the whole batch before health mutation. Expanded the existing atomicity regression; native checks are queued.

- Fixed expired sprint timers below zero rejecting real Match damage and suppression. The canonical timer remains unchanged; only the pure helper projection clamps its inactive value. Added an original-source negative-timer trace. Combined verification is queued.


- Connected ordinary ground and naval target acquisition, aim, cooldowns, per-model fire and physical salvo launches to Match. The Rust replay matches 66 original-source traces across 264 phases, including cover, moving fire, visibility, hold-fire, incoming threats and veterancy. Flights keep immutable launch solutions and shared volley identities. Contact stepping and damage hookup remain in progress; air, rail, operative, beam and active ability/terrain fire are explicitly unsupported by this phase.

- Added the current Match visibility, hull/footprint range and line-of-sight query used by attack-move. Weapon target selection, firing and projectile execution remain in progress.

- Added one canonical Match control row for aim, ability and retreat switches, attack intent, facing and waiting orders. Startup prepares the rows from unit definitions; recruitment can stage them before charging. Command execution is the next part of this lane, and no wire or C interface changed.
- Added complete event-adapter preflight for canonical retreat velocities, pose, suppression and sprint values so malformed later blast targets fail before earlier unit damage. Focused native checks pass.

- Added canonical application of combat damage/suppression events, rider placement and maintained-grid updates, explicit unit/section/terrain contact adapters, terminal blast damage, and ordered ground-unit retirement. The adapter retains shooter XP after removal and reports deferred story/FPS/convoy/routing notifications. Added building section reconciliation and vehicle wreck motion, settlement, scorch and oldest-hull cleanup through the existing environment runtime. Three focused tests pass, comparing eight original JavaScript traces with 33 actions and checking atomic rejection plus grid order. Workspace tests, strict Clippy and formatting pass. Full firing and tick orchestration remain separate lanes.

- Added optional durable wreck IDs to query state, preserving absent identity in older query fixtures. The authoritative wreck producer uses the dead unit ID; remembered-routing consumers can reject missing identity explicitly. No wire or C layout changed.

- Reserved canonical Match combat metadata for XP, incoming-hit history, death impulse and ordered cargo/carrier identities, plus the existing environment runtime. Starting units and buildings receive the original empty/default metadata; Game continues to own health, suppression and positions. The firing lane owns retarget and resets it through a typed sink. Damage/death execution and verification follow in this component lane.

- Connected Conquest point supply, capture and income to the assembled Match, using the tick's ordered unit list and one canonical objective/reserve state. Eight original-source traces compare 72 phases across all armies, allied and enemy teams, disconnection, supply obstruction, post-decision VP freezing and units spawned during a tick. Capture timestamps follow source winner gating; malformed input leaves objectives and wallets unchanged. This phase does not advance the clock or implement the remaining match systems.
- Added authoritative terrain blasts, progressive section/building damage, support failure, ignition, burning, flooding and section-debris movement/settlement. Forty-four original JavaScript traces compare 253 actions across all 33 maps, including shared building HP, beam/contact deduplication, mine clearing, float32 assignment boundaries, garrison evacuation, bridge casualties and the 96-body overflow path. A separate debris oracle checks 54 mass cases, 21 constructors and 540 body states. Fires retain insertion order while borrowing the shared environment seed, and canonical smoke now has optional remaining lifetime. Independent review found and fixed fire-escape path counter exhaustion before partial damage. Full match scheduling, vehicle wreck production/settlement, repairs and wire/ABI exports remain unfinished.

- Added standalone supply stores, payer-owned stock and donation grants, automatic fleet and job scheduling, route continuation/repair, loading/delivery/return, convoy commands, support spending and recipient projections. Original JavaScript fixtures cover 12 startup/runtime traces with 540 steps, six payer pools and five support cases. Reserves now carry optional territorial supply rate and cut-off duration. Fixed clustered member ordering, unusable-truck command mutation and delivery after alliances change during the port review. Match hookup, supply-network/path algorithms, mandatory withdrawal and wire delivery remain separate; Game, ABI and rules hash are unchanged.

- Added ground production door placement with ordered exit rings, vehicle clearance, occupancy checks, origin-region fallback, heading and spawn ghosts. Two focused tests compare 934 projections from the unchanged JavaScript on 43 terrains, including all 44 non-air kinds and 33 authored maps. Recruitment receives a placed Unit and an explicit rally routing request; wallet/ID changes, path execution, full unit-state initialization and air-base production remain outside the helper.

- Add Conquest recruitment preparation with canonical population, queue and maker checks. Placement failure drops the request without payment; successful commit charges MP once. Two focused tests cover 266 original-JavaScript buy commands and aborted transactions. Other modes, placement and Match integration remain separate.
