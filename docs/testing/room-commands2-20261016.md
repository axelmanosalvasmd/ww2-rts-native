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
