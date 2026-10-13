# Operative tick provider

This source-written checkpoint connects the existing operative turn to its real combat methods. It is separate from frozen PR #142 and awaits the sole tick owner's consumer composition, full-step replay and native verification.

`prepare_companion_tick` checks the canonical key/record/body/control identity joins and enough clock headroom for the next tick's death delay. It reads state only. An empty registry preserves the ordinary runner's clock bounds. Other modes and inconsistent internal mode state remain unsupported.

`step_companion_turn` calls the existing movement, input-expiry and timer phase once, then consumes its Fire or Grenade request immediately through the canonical effects methods. A decided finish suppresses live input. There is no new timer, RNG stream, flight registry or damage state. The runner owns fault handling if a later phase fails after earlier mutation.

The consumer must respawn after the clock advances and before debris. Its live unit loop calls the operative turn immediately after the dead-unit check, then continues before ordinary reserves, orders, suppression, ability/repath/sprint/retarget timers, movement or firing. Late retirement preflights every captured potential companion before generic cleanup, because building collapse can kill a later actor, then applies companion bookkeeping to returned retired records in order. Generic removal, rewards, story and retained projectile metadata keep their existing owners.

The recorder calls the unchanged source full `step`: 15 traces, 576 operations and 506 ticks, with 10 seeded Combat draws. Every recorded operation is also replayed with ordinary unobserved source objects and its complete snapshot/number bits must match. Cases cover freshness and want clearing, timer and weapon priority, actual fire kill/death/story, the exact 160-tick wait, paid and led-squad respawn, and no respawn spending tally. The grenade launch tick records its no-travel flight and immediate linked-ticket decay, then stops before contact/settlement.

Preflight distinguishes active bindings from retained dead metadata. A retired operative control may remain only with its canonical firing/combat rows, a nonzero historical ID below the allocator head, and no live body or registry reference. This keeps the existing retirement ownership without admitting a fabricated future actor.

The plain-terrain authored cases leave ordinary vision/projectile body interoperability with operative jump/crouch geometry outside this proof. Grenade contact/settlement, unavailable unrelated phases, transport and whole-game acceptance remain explicit boundaries until actually exercised and connected. Native replay and the consumer composition are pending.
