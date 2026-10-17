## Recipient-fed AI engineering proposals

The ordinary AI planner remembers bridges delivered to its seat and proposes bridge, fill and mine-clearing work over received terrain and contacts. Newly delivered bridge cells are sorted numerically before new cells append to existing memory. Fill and mine-clear scans reuse their sorted crew arrays, preserving the earlier ordering when a later target ties.

Engineering uses the existing delayed human-command input path. A proposed Dig retains a pending crew job until a later delivered observation shows digging, completion, loss or the prototype's 90-second expiry. This avoids duplicating a command while motor inputs wait. The detached planner checks delivered span visibility and proposes the prototype's approach move when the span is unseen. It receives no synchronous canonical denial receipt, so a visible Dig refused by canonical admission cannot trigger an immediate approach fallback and may wait for pending expiry. No wire field or command is added.

## Integration with the delivered field packet

Main's delivered field policy is the single dispatcher when a complete field packet is present. It retains canonical visibility preflight, budget accounting, approach and denial fallbacks. Its bridge memory now preserves discovery batches and its bridge, fill, mine and holder selections use the same stable numeric sorter. Field and World private policy state gain serialization with default outer fields for older saved policies. World admission remains deferred.

The earlier engineering and holder providers remain only for existing component observations without a field packet. A planning look never runs both implementations. Active lifecycle tests retain the field packet and inspect the saved live field state. The fallback's pending-denial limitation above applies only to those older component observations.
