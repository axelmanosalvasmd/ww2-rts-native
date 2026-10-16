# Room AI and restore ownership

Keep room planner and observation memory beside `Match`, reusing #392's external AI save owner and #372's spectator transport. Match continues to own canonical accepted orders, deterministic physics, routing, fog and pending tick events. A human reclaim replaces the private owners together without changing canonical orders. AI memory is persisted only for AI seats; human attention, motor inputs and delivered packets restart, matching the prototype's separate WeakMaps. Ordinary version 1 keeps its existing shape and accepts absent optional AI memory/difficulty with prototype defaults.

The human observation producer follows the caller's snapshot delivery beat, including adaptive odd ticks, while retaining canonical vision, ownership and map binding checks. Every requested AI observation precedes every AI action. Existing canonical command parsing and dispatch decide attempt eligibility; room delivery additionally refuses support kinds that its recipient snapshots cannot carry.

Prepare seated and spectator starts before publishing a replacement room. A failed role projection leaves the prior roster, runtime epoch and socket delivery accounts intact. The default-map checkpoint also needs the compiled house tag already repaired by the operative stream.
