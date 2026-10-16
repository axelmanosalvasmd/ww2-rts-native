## Named armies in trusted Conquest startup (2026-10-16)

Trusted Conquest startup passes the lobby's validated army name into the existing
ArmySize owner. The core retains the prototype's population and income factors;
the private host receipt records the actual choice. The existing recipient army
object carries those factors without a new field or recipient policy. Invalid
internal names refuse startup before publication. This expands the existing
private two-human Conquest profile to its four named army settings. Other mode,
World, sandbox and role admission keep their existing guards.

Native constructor checks cover all six non-World modes. They replace the
Conquest-only base check and partial wallet plan check, with an authored
causeway that forces base clearance and moves trapped starting squads before
ordinary ticks and both recipient snapshots.
