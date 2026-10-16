# Horde core lifecycle and recipient fields

Baseline: prototype `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`.
Owner decisions K, N and O authorize prototype Horde behavior and JSON WebSocket
v0 shapes. This change connects the core runner and recipient adapter. Hosted
room startup and scripted-seat scheduling are described by the later
[hosted Horde extension](hosted-horde.md). World admission
and the C interface are unchanged. Hermes' exact-head runtime acknowledgement
is required before merge.

The late Horde phase runs after death cleanup and point capture/income, matching
`reference/ww2-rts/shared/sim.js:5769`. Units spawned there first act on the next
tick. Horde vision uses the normal four-tick cadence (`sim.js:5320`).

The complete snapshot's `mode` object is identical to `modeRow` at
`sim.js:5895-5898`. All seats receive the same public fields:

- `kind: "horde"`, `defenderTeam: 0`, `attackerTeam: 1`, `slot`, `wave`, `left`,
  `active` and `suddenDeath: false`.
- `timeLeft` is nonnegative and rounded upward.
- `nextProfile` appears only during a break. `profile` is absent before wave 1.
- `boss` is absent before wave 1, then is null with no living Kaiju, or the pair
  `[ceil(sum of living Kaiju HP), sum of their maximum HP]`.

Initial omissions follow `setupHorde` at `sim.js:859-861`. Profile assignment and
boss totals follow `sim.js:930-955`. Purchase seeds, reserve contents, budget,
profile history and the bunker/gate identities never enter this object.

Each spawned Kaiju emits exactly
`{"k":"say","text":"THE KAIJU HAS SURFACED","x":gate.x,"z":gate.z,"pub":true}`
(`sim.js:935-939`). Breath uses the existing public shot shapes:
`{"f":shooter,"t":target,"fo":owner,"to":targetOwner,"x":end.x,"z":end.z,"hit":true,"k":"kaiju","pub":true}`
and `{"k":"shell","x":sample.x,"z":sample.z,"pub":true}`
(`sim.js:4363-4374`). Both pass through the existing recipient projection:
public effects reach every seat, while shooter and target identifiers are omitted
when unseen, as in the cached `snapshotFor` path at `sim.js:6079-6088`.

The existing snapshot refusals for retained flights, salvos, grenades and aviation
events remain. The later [hosted Horde extension](hosted-horde.md) admits rooms
with those guards and the existing aircraft weapon restriction. Complete support
and projectile coverage remains separate work. The lifecycle test uses an injured bunker and a close boss gate
as declared initial conditions, then runs actual Kaiju combat to bunker defeat
and eight ending ticks, collecting every seat on every tick.

Exact field tests: `horde_mode_wire_fields_and_private_purchase_omissions_match_prototype`
and `mode52_mode54_boss_wave_runs_to_bunker_defeat_with_every_seat_snapshot`.
