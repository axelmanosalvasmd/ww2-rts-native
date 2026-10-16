# Classic and Annihilation recipient snapshots

Prototype baseline: `e4e579a`. This extends the existing ordinary recipient
adapter to Classic and Annihilation. Owner group H permits full logistics with
prototype visibility (issue #5, comment 6217590741). Runtime acceptance still
requires review of the final head. No ABI or header changes are included.

Every field below matches the pinned prototype. Existing unit, terrain, fog,
production and logistics fields keep their recipient rules. World, Assault,
Horde and Tutorial remain excluded from this complete snapshot adapter.

| Field | Shape, source and recipient rule |
| --- | --- |
| `mode.kind` | Public `"classic"` or `"annihilation"`, from the canonical mode owner, as at prototype `shared/sim.js:5895-5897,6032`. |
| `mode.suddenDeath` | Public boolean, including false. Same mode-row source at `shared/sim.js:5896`. |
| `mode.timeLeft` | Classic only: ceiling of remaining seconds, clamped to zero. Annihilation omits this field because its canonical timer is absent. Prototype `shared/sim.js:5896`; constructors at `:792-795,1677-1682`. |
| `mode.bunkers` | Annihilation only: public team-indexed counts from the canonical elimination owner. Classic omits it. Prototype `shared/sim.js:792-795,5896`. The internal `teams` count never enters the mode row. |
| `mun`, `fuel` | Classic only: floor of the receiving player's own currency. No allied or enemy wallet is included. Prototype initializes these in `shared/sim.js:1682` and projects them at `:6017`. Canonical accounting zeros in Annihilation do not create wire fields. |
| `fuelInc`, `upkeep` | Existing optional receiving-player fields, rounded to one decimal. Omitted until their canonical optional values exist. Prototype `shared/sim.js:6017,6019`. |
| `nodes` | Classic only: every canonical resource node in insertion order, including nodes under fog and enemy-controlled nodes. Each row is `[x,z,rate,fuel]`, with positions rounded to one decimal, unrounded rate and fuel encoded as 0 or 1. No depot identity, claim or owner is added. An empty Classic collection is `[]`; Annihilation omits the field. Prototype `shared/sim.js:5932,6019`; node creation at `:1705-1721`. |
| `ghosts` | Classic only: team-shared remembered enemy building rows `[id,type,owner,x,z,built]`, filtered against current recipient visibility. Positions and construction fraction are rounded to one decimal. An empty memory produces `[]`. The adapter reads remembered facts rather than hidden live buildings. Prototype `shared/sim.js:4456,5928,6037`. |

The existing logistics object follows [full-logistics.md](full-logistics.md).
Classic and Annihilation use the same seat-private reserves, trucks and events,
and allied authorized stores. Reveal and alliance do not expose another seat's
private rows. No World region-rate field is supplied.

Global node list versioning reads the same canonical Classic rows as recipient
projection. The ordinary server trims unchanged nodes and resends them for a
full/reconnect frame, as at prototype `server.js:225-244`. Neither snapshot
projection nor version preparation mutates node ownership.

Focused source tests are registered in `recipient_snapshot::elimination_tests`:

- `classic_complete_snapshots_keep_private_currency_and_public_prototype_nodes`
  checks all three seats, allied wallet isolation, hidden live unit filtering,
  remembered building rows, node rounding and empty nodes, and selected full
  delivery.
- `annihilation_complete_snapshots_match_mode_shape_and_omit_classic_fields`
  checks the complete mode object and omitted Classic-only fields on every seat.
- `elimination_snapshot_admission_rejects_missing_state_and_other_modes` checks
  missing owners, invalid timers, mismatched mode pairs and excluded modes.

These tests are source additions until the integration run executes them. Full
match lifecycle validation belongs to the LOGISTICS-2 integration checks.
