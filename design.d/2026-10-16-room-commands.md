# Room command composition

Keep one canonical command queue for FireAt and convoy input. The server binds
issuer, tick and sequence; simulation owns command execution and denials. Convoy
commands select before ordinary troops, retaining prototype duplicate order and
uncapped selection. They reuse the existing convoy actuators and recipient-fed
routes rather than a second room-level logistics state. Pending inputs persist
through the existing JSON save owner.

A mixed bomber and ground FireAt selection shares the ground dispatcher and
prepared aircraft missions. Ground guns retain queued orders while bombers take
an immediate area mission even for a queued input. This preserves the prototype
intent at `shared/sim.js:3462-3477` without a parallel firing implementation. Naval FireAt uses that same salvo
path while other naval ingress remains guarded. Its witness runs an authored
destroyer on water through actual launches and every-seat delivery.

World recovery and World room admission remain deferred. The large #393 sandbox
integration was aborted; competing flight adapters need a separate reconciliation.
No FFI/header or intentional player-visible difference is introduced.
