# ROOM-COMMANDS-2 integration

Base: ROOM-COMMANDS #425 at `745229e9`. Main `4a88afd8` merged automatically as `848203d2`.
Stacks on #425 (#296, #366, #369 with #349/#337, and #375), plus #393, #374 and #413.
World recovery and World admission remain deferred under owner decision C.

## Sandbox merge

Incoming #393: `d374de4197b85c1fe2e719e45957291d1bc93a8e`.
Hand-resolved regions:

- `match_commands.rs` (five): union Spawn with Supply/LogisticsResume/Compound variants, JSON parsing, early execution, exhaustive command matching and eligibility exclusions.
- `match_firing.rs` (five): retain #369's canonical flight_contact_for and visible_flight_contact, including recipient-visible bodies and current poses. The equivalent incoming contact helper is superseded.
- `recipient_snapshot.rs` (one): retain unconditional canonical flight projection and optional logistics projection. Sandbox uses that same flight owner.
- `recipient_snapshot/composition.rs` (three): preserve canonical flight fields and ordinary flight admission; combine firing, grenade and salvo validation. Keep chronological aviation-buffer refusal.
- `recipient_snapshot/shots.rs` (four): preserve ordinary-flight documentation and Throw admission; add incoming sandbox Breath validation while retaining SmallArms refusal pending NET-58. The duplicate incoming live_flights implementation becomes a test-only call to the canonical projector. All incoming flight shape/contact/privacy tests remain.
- `commander_ingress.rs` (four): add Spawn recognition, name and canonical admission to the entire existing command union; preserve selection guards.
- `lobby.rs` (two): preserve readiness, private Conquest, named armies and logistics; admit the sandbox flag.
- `main.rs` (one): retain static root, room limit, named armies and logistics help; state optional sandbox.
- `trusted_room_process.rs` (one): remove obsolete sandbox/named-army refusal entries. Positive lifecycle coverage is retained from both owners.

No other source path is manually changed in this merge. All automatic changes and test registrations remain.

## Operatives merge

Incoming #374: `0a712ac4f909f130fa3518b1586b2089e6e07b0f`.

- Contract index (one): combine operative hit-position and spectator decisions with the R1 ingress row.
- Protocol (one): retain operative prototype hit positions and cached mask from incoming, ordinary spectator decision from base.
- `match_firing.rs` (five): retain the same canonical visible-contact owner as the sandbox merge.
- `recipient_snapshot.rs` (three): union all modules and exports; retain canonical flights plus logistics projection.
- Composition (two): retain ordinary flights/salvos rather than restoring operative-only restrictions. Remove the operative-only global validator; canonical firing/grenade validation remains.
- Shots (two): retain sandbox breath handling, incoming operative SmallArms validation, and ordinary Throw validation. Horde breath still awaits its mode owner.
- `lobby.rs` (five): retain both spectator and operative starts, admit operative hello before commander restrictions, preserve spectator joins in running rooms, retain optional returning-seat handling, and register both test modules.
- `room_runtime.rs` (one): retain latency telemetry and incoming operative event/list methods.
- Incoming `operative_flights::project` delegates to the canonical projector after its dedicated operative validation. This preserves original ballistic stamps and all operative tests.

## Hosted Horde merge

Incoming #413: `a1be2f7419faa90fedfb6138e7a1a4d0e692ba5d` (#399/#357 ancestry).

- `match_tick.rs` (one): retain initialized ordinary convoy validation and World logistics refusal; add Horde economy validation.
- `recipient_snapshot.rs` (two): preserve wave-zero field omission and current mode rows; retain companion validation plus projectile validation.
- Composition (two): preserve canonical flights and complete flight/grenade/salvo validation.
- Public state (one): retain the general mode owner rather than replacing it with Horde-only mode projection.
- Shots (five): union Aviation and Horde tick events; retain ordinary producer-buffer guards, operative SmallArms checks and invalid Throw test; admit breath for sandbox or Horde; retain general Throw identities and owners.
- `lobby.rs` (four): retain spectator-capable attachment and Horde canonical seat count; preserve operative/spectator hello, with five-defender Horde lobby capacity; retain both operative and Horde control dispatch and both modules.
- `main.rs` (two): retain existing help plus Horde/map-directory help and authored map configuration.
- Incoming `projectiles::for_seat` delegates to the same canonical flight projector. Its complete validator and all projectile tests remain. The duplicate geometry/contact implementation is superseded.

All inherited World, aircraft-weapon, privacy, invalid-state, terminal tick and command guards remain unless their explicitly documented runtime owner replaces the old admission restriction.
