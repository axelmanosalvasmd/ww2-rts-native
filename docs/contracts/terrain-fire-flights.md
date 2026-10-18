# Ground terrain fire and retained flights

Status: prototype-identical JSON WebSocket v0 projection under owner groups E
and O, recorded in [issue #5 comment 6217590741](https://github.com/axelmanosalvasmd/ww2-rts-native/issues/5#issuecomment-6217590741).
Reference: `ww2-rts` at `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`.
Hermes' exact-head runtime acknowledgement remains a separate integration gate.
There is no ABI, header, transport version or client command change here.

The existing snapshot `flights` array carries each currently launched ground
projectile visible to that recipient. It uses the pinned prototype's existing
fields and recipient rules. `shared/sim.js:5985-5996` constructs the rows;
`:6079` puts them in the complete snapshot.

| Fields | Value and recipient rule | Prototype source |
| --- | --- | --- |
| `flight`, `kind`, optional `effect`, `seq` | Retained flight ID, source kind, optional effect and current contact sequence. Undefined effect is omitted. | `shared/sim.js:5991-5992` |
| `x`, `y`, `z`, `v`, `gravity` | Current ballistic position, current velocity as `[vx,vy,vz]`, and gravity. No rounding. | `shared/sim.js:5989-5992`; `shared/projectiles.js:22-29` |
| `time`, `launched`, `expires`, `until` | Current match seconds, original launch time, expiry and recipient extrapolation bound. | `shared/sim.js:5986-5992` |
| Optional `f`, `fo` | Shooter ID and owner appear together only when `seenBy` permits that shooter for this seat. No target identity fields appear in a retained row. | `shared/sim.js:5993` |

A future launch does not appear before `launched`, and visibility samples the
projectile's ground cell regardless of its altitude. Reveal permits all cells;
allied ownership alone does not reveal the flight's cell. See
`shared/sim.js:5966-5969,5988-5990`.

`until` starts at `min(expires, time + 0.15)`. The segment ends at the first
hidden ground cell boundary. For a source with a weapon, an earlier static
contact also bounds the segment. That contact query uses current target poses
and excludes units the seat cannot see, while preserving terrain contacts.
This projection does not resolve damage (`shared/sim.js:5970-5983,4226-4248`).
Launch event clipping still uses only fog because
`flightEventFor` reconstructs no weapon (`:6005-6006`).

Ground launch, contact, expiry and salvo events keep their existing JSON
encodings and projection. Grenade throws now pass through the same local-event
projection as the prototype's `{f,fo,x,z,k:"throw",physical:true,local:true}`
(`shared/sim.js:5424-5427`). A hidden shooter loses both `f` and `fo`;
the event itself requires a visible ground cell (`:5998-6006`). Physical
salvo and grenade tickets are internal schedules and add no snapshot field
(`:4146-4153,5425-5427,5616-5629`).

All complete seat adapters preserve projected retained rows. Validation happens
before terrain, list or fog publication. Undrained firing, support, aviation or
external environment batches still refuse complete capture; callers must use
the existing chronological tick-event batch. Aircraft retained flights remain
guarded in this change.

`recipient_snapshot/flights_tests.rs` contains exact shape, current trajectory,
optional-field, ground-fog, future-launch, expiry-bound, visible-contact and
hidden-shooter checks. It also captures every seat with retained ground flights
and physical tickets and checks that undrained events refuse without changing
delivery acknowledgements. These tests are authored from source inspection. The PR records native
validation; the prototype was not executed.

Waiting FireAt orders retain the existing owner-only order row: `5,x,z` after
the unit ID and order count (`shared/sim.js:3382-3389,5893-5894,5915-5924`). The stored
point is the target cell centre selected at command admission (`:3463-3475`).
Only normalized singleton, nonqueued stored commands are projected; other
seats receive no waiting order from this actor.
