# ROOM-COMMANDS-2 event compatibility

Compatibility source: `reference/ww2-rts` at
`e4e579a838aabae3b1d9dadbc17a0a87147bc52d`. The owner decisions E, G, K,
N, O and P recorded in [issue #5 comment 6217590741](https://github.com/axelmanosalvasmd/ww2-rts-native/issues/5#issuecomment-6217590741)
authorize these prototype fields and recipient rules. This note describes their
composition from the aviation, sandbox, hosted Horde and operative dependencies.
It adds no message, ABI field, recipient exemption or World room admission.

## Ordinary shots

The existing `shots` array retains producer order. A nonlocal shot is admitted
by visible position, an allied shooter or target owner, a seen shooter or target,
or reveal. `pub: true` independently admits the shot only outside World.
Hidden `f` and `t` identities are removed independently; ordinary shots retain
the paired `fo` and `to` owner fields. Prototype: `shared/sim.js:6080-6088`.
Scenario recipient routing retains its existing owner and is unchanged here.

Operative fire has exactly `k`, `f`, `fo`, `x`, `y`, `z`, where `k` is
`"sniper"` or `"commando"`. Only an enemy hit adds `t`, `to`, `hit: true`
and, when a drawn squad member was hit, `m`. A miss has none of those hit fields.
It is an ordinary nonlocal shot, including when the shooter is an operative.
Prototype: `shared/sim.js:2094-2096`.

Kaiju breath uses the same ordinary-shot policy in Horde and sandbox:

```json
{"f":1,"t":2,"fo":0,"to":1,"x":100,"z":50,"hit":true,"k":"kaiju","pub":true}
{"k":"shell","x":20,"z":50,"pub":true}
```

The first row names the full beam endpoint. Shell samples follow at eight-metre
intervals along the beam. Hidden IDs disappear while owner fields remain.
These examples illustrate the exact keys; IDs and positions come from the
actual firing state. Prototype: `shared/sim.js:4363-4374,6080-6088`.

Inherited ordinary aviation and support rows retain their prototype shapes:

| Event | Fields before recipient redaction | Prototype `shared/sim.js` |
| --- | --- | --- |
| Anti-air | `k: "aa"`, `f`, `fo`, `t`, `to`, `x`, `z` | 2250 |
| Plane down | `k: "planedown"`, `t`, `to`, `x`, `z`, `dir`, `kill: true` | 2251 |
| Support arrival | `k` equal to support kind, `x`, `z`, `dir`, `fo`, `pub: true` | 5249 |
| Bomb or chutes | `k: "bomb"` or `"chutes"`, `x`, `z`, `pub: true` | 5253, 5262 |
| Flak | `k: "flak"`, `f`, `fo`, `x`, `z`, `pub: true` | 4704 |
| Interception | `k: "shotdown"`, `x`, `z`, `dir`, `kind`, `by: "fighters"` or `"flak"`, `pub: true` | 4709 |

## Local events and live flights

Local projectile events require visible ground-cell fog or reveal. Allied or
seen participants alone cannot admit a local event. A hidden participant loses
both its identity and paired owner field. Prototype:
`shared/sim.js:5966-5969,5998-6009,6080-6083`.

An operative grenade throw has `f`, `fo`, `x`, `z`, `k: "throw"`,
`physical: true`, `local: true`, as in `shared/sim.js:2098-2105`.
Ordinary grenade throws use the same keys (`shared/sim.js:5427`). Launch,
contact and expiry events retain the existing projectile encodings
(`shared/sim.js:4138-4139,4292-4301,4340`). A launch event's added `until`
uses the event origin, velocity, gravity and launch time. It clips at hidden
ground and has no contact profile (`shared/sim.js:6004-6006`).

The existing `flights` array is independent of the accumulated shot batch.
Each launched, currently visible flight has `flight`, `kind`, optional `effect`,
`seq`, `x`, `y`, `z`, `v: [vx, vy, vz]`, `gravity`, `time`, `launched`,
`expires`, `until`. Only a seen shooter adds `f` and `fo`. There are no target
fields or private damage/contact parameters. Prototype:
`shared/sim.js:5985-5996,6079`.

Live projection uses the original ballistic solution at the current simulation
time. Its visual horizon ends at the earlier of expiry or 0.15 seconds ahead,
then clips at hidden ground and an actual visible static contact. Hidden unit
bodies cannot shorten it, and projection neither resolves damage nor consumes
the flight (`shared/sim.js:5970-5983`).

Complete delivery validates and projects the supplied chronological batch before
publishing any selected recipient's delivery state. Retained operative grenades
continue through shooter retirement, terminal contact and metadata release.
The tests and execution boundary are listed in
[`net58-roomcmds2.md`](../testing/net58-roomcmds2.md).
