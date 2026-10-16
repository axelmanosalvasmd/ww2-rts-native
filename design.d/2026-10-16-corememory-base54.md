
## Detached World fort capture (2026-10-16)

`Match::apply_world_regions_with_fort_capture` explicitly selects fort transfer
inside the existing transactional region pipeline. Ordinary bounded region APIs
retain refusal when capture requires a fort transfer. This separates detached
component proof from World tick and hosted-room admission while keeping one
owner for capture, elimination and finish preparation. Captured Gate controls
reset only after preparation succeeds; matching building-cell rows and spatial
owner indexes follow the unit's new owner. Damage and unrelated footprint rows
remain unchanged. No persistent state or external interface changes are added.
