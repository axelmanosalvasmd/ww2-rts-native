# Sandbox on R1

Compatibility source: `reference/ww2-rts` at
`e4e579a838aabae3b1d9dadbc17a0a87147bc52d`. Owner decisions M, N and P
in issue #5 comment 6217590741 authorize the prototype commands, shapes and
recipient rules. This implementation still needs Hermes' exact-head runtime ack.

The existing private, two-human Conquest R1 room accepts sandbox as an optional
flag. World and hosted Horde admission remain closed. The internal Horde spawn
component follows the prototype, including activating a wave before wave one.
Sandbox saves remain refused by the existing save capability guard.

## Messages

- `{ "t": "sandbox", "v": value }`: the host may change the flag before play.
  JavaScript truthiness becomes a boolean in the lobby and match options.
  Guests cannot change it. Source: `server.js:676-677`, `shared/sim.js:747`.
- The existing `start` message includes `"sandbox": true` for a sandbox match,
  with the same fields as an ordinary start. Restart creates a fresh match with
  the room's flag. Source: `server.js:380`, `:463-466`, `:693-694`.
- `{ "t": "spawn", "unit": name, "x": number, "z": number, "enemy": value }`:
  any active seated commander may spawn while unpaused. `enemy` is optional and
  uses JavaScript truthiness. The server derives the issuer from the current
  connection and records the command for the next simulation boundary; client
  slot, tick and sequence fields do not choose authority. Source permission and
  payload: `server.js:718-720`, `shared/sim.js:3409-3417`, `:4348-4360`.
- Successful spawning sends no dedicated reply. Invalid sandbox, unit,
  structure, coordinate or count returns the existing
  `{ "t": "deny", "cmd": "spawn", "reason": "blocked" }`, subject to the
  existing denial limiter. Source: `server.js:719-720`, `shared/sim.js:4350-4352`.

Spawning is free, ignores faction and normal population limits, and requires a
known non-structure unit and finite numeric coordinates. The exact guard rejects
when the current count is greater than 2000, so count 2000 permits unit 2001.
Falsy `enemy` chooses the sender. Truthy `enemy` chooses the first nonallied
roster seat, or Horde's internal seat. No enemy means `blocked`. Ground units use
nearest-free terrain placement at the requested point, including clamping outside
the map. Aircraft keep their normal air base and ignore that point. Source:
`shared/sim.js:1773-1808`, `:4348-4360`. An idle sandbox train has no World run and
skips its turn, matching `shared/sim.js:5354`.

## Recipient rule

Sandbox introduces no visibility exemption. Each seated recipient receives
spawned units through the existing recipient view and unit delta path:

- Own and allied units are visible; enemy units require ordinary sight. Parked
  enemy aircraft stay hidden between vision updates. End reveal shows all units.
  Source: `shared/sim.js:5942-5945`, `:6042`.
- Foreign cooldown and private stance bits remain redacted, even during reveal.
  Target identity follows the recipient's visibility. Source:
  `shared/sim.js:6042-6044`.
- Terrain, fog, shots, aircraft metadata and owner plans retain their existing
  per-field recipient rules. No extra sandbox list or message is added.
  Source: `server.js:246`, `shared/sim.js:6000-6082`.

The server tests check the exact existing start keys, the spawn/deny payloads,
nonhost dispatch, hidden enemies, owner-field redaction after victory, and every
non-structure catalogue type through six ticks with snapshots for both seats.
Core tests cover the first enemy in roster order, no-enemy refusal, the exact
2000/2001 boundary and the internal Horde activation/bunker intent.

Sandbox Kaiju beam effects use the existing prototype event shapes:
`{ "f": shooter, "t": target, "fo": owner, "to": targetOwner, "x": x,
"z": z, "hit": true, "k": "kaiju", "pub": true }` and
`{ "k": "shell", "x": x, "z": z, "pub": true }`.
The beam endpoint and shell samples are public outside World, even under fog.
Each recipient loses hidden `f` and `t` identities; the nonlocal event retains
`fo` and `to`. Source: `shared/sim.js:4363-4377`, `:6080-6088`.
Exact event-shape tests cover hidden identities and public delivery, and the
server lifecycle test requires an actual beam hit followed by both seat views.
