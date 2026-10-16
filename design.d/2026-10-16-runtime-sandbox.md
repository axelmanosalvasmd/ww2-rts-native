## Sandbox uses the canonical actor lifecycle

R1 sandbox stays an option on the existing private Conquest room. The host owns
the lobby flag and start; every active commander can spawn, matching the pinned
prototype. Spawn commands use the existing recorded command boundary. Ground
spawns prepare ordinary actors and publish their control, combat, firing,
ability, recovery and optional reserve metadata together. Aircraft reuse the
aviation factory. No duplicate actor store or sandbox tick loop is added.

The recipient adapter applies its existing visibility and owner-field rules to
these actors. Sandbox grants no fog exemption. A sandbox train without a World
rail run stays idle, as the prototype's rail turn does. Registered World train
validation remains in place. The internal Horde component permits sandbox
enemies to activate wave zero, without opening hosted Horde or World admission.
The prototype messages, recipient rules and line references are recorded in
`docs/contracts/sandbox.md`. Sandbox save admission remains closed.

Sandbox Kaiju reuse the existing beam actuator and ephemeral events outside
Horde. The firing and recipient guards admit these events only for sandbox;
normal beam damage, public visual effects and identity redaction follow the
prototype. This is needed for a spawned Kaiju to survive its first turn and
attack without faulting delivery.
