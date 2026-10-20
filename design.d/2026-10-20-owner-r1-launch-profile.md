## Explicit bounded owner-R1 startup profile

The existing prototype weather contract and tests intentionally expect per-start
weather entropy, logistics by mode, and match seed progression. Restoring the
older owner profile globally would change that behavior. A host-only opt-in
selects the requested fixed seed, absent weather-seed override, and logistics
off, retaining the canonical constructor and existing publication transaction.
The bounded profile admits Conquest, Standard army, sandbox off and two human
commanders. Horde is routed through the same rejection path rather than bypassing
profile validation. Match identities remain fresh and independent of the seed.

The CLI and library config carry the same enum through sharded hosts. No wire,
ABI, core, reference pin, ingress or fault-recovery policy is changed. Runtime
receipts retain their existing shape. This server fix does not qualify Unreal
adoption, saved-game Load, physical input or hardware performance.
