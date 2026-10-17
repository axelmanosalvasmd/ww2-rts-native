# Server logistics enablement

Native baseline: `58986df2a7c13b6fd9c9902d8de0623e985fce08`.
Prototype: `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`.
Scope: room option selection and the existing trusted two-human Conquest profile.
This documents the prototype option mapping and adds no protocol field or message.

`server.js:353-356` enables logistics for Conquest, Classic, Annihilation and
World. Assault, Horde, Tutorial and other mode strings default to disabled.
A truthy authored `scenario` overrides that table: only the literal JSON boolean
`scenario.logistics: true` enables logistics. Missing, false, null, numeric,
string, array and object logistics values do not opt in.

`server.js:380-381` passes that result to `createGame`, unless a loaded room has
`room.logistics === false`. Loaded matches retain their serialized logistics switch. The native room also
retains loaded logistics and tech booleans across restart, so a loaded false
continues to disable the corresponding feature (`server.js:419`). A loaded true
still passes through the mode and authored-map selection. There is no public
logistics or tech toggle. A fresh trusted Conquest start supplies the mapped
option from the immutable authoritative map. Existing room, scenario, mode,
World, AI and spectator admission stays in place. The helper's complete table
does not admit those modes.

The private receipt records the resolved boolean in its existing options object.
The normal runtime and recipient adapter own reserves, convoys and delivery.
Their projection follows the prototype's existing logistics shape, documented
separately. Startup envelopes retain their existing fields; scheduled snapshots
carry logistics through the ordinary recipient view.

Focused checks:

- `logistics_enablement_matches_prototype_mode_table_and_strict_scenario_opt_in`
  checks the complete table, strict boolean opt-in and falsy scenario fallback.
- `trusted_conquest_logistics_survives_room_ticks_and_both_recipient_snapshots`
  starts the trusted room, runs twenty ordinary ticks, completes snapshot writes
  and checks enabled, nonempty private reserve rows for both seats without a
  runtime fault.
- The existing canonical map/weather startup matrix expects initialized reserves
  and convoy state. Existing process and socket tests expect logistics enabled
  in the private receipt.

These checks cover LOG-01's server mapping and the admitted Conquest slice of
MODE-11 and NET-39. The LOGISTICS-2 checks in [logistics2.md](../runtime/logistics2.md) add the remaining
mode/option switch matrix and loaded-false restart coverage. Unsupported room
admission remains separate.
