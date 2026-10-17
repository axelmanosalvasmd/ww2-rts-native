## Recorded AI delivery beats (2026-10-16)

The existing shared AI scheduler takes `Beat { tick, sent, finished }` as an
explicit input. The beat now supports serialization for host-private replay
logs. Replay supplies the recorded values rather than measuring server load
again. This adds no client message, Match checkpoint field or timer to the core.
The host remains responsible for retaining the beat beside its input log.

The legacy core human-observation producer accepts odd ticks when its caller
chooses to deliver there. It still requires the canonical vision phase and an
ordinary supported match. An adaptive three-tick cadence can select odd ticks;
the observation producer must not replace that input with a fixed even cadence.
The existing fixed-cadence opening helper retains its separate two-tick policy.

IF-29 is tested by recording and decoding 320 input beats with cadence changes
across three, two and four ticks. Fresh native matches use those beats with the actual
seat commanders, delayed input queues, canonical command dispatcher and
recipient producer for every seat. The replay compares delivery ticks, all
emitted commands and every recipient frame. A fixed-two-tick negative control
must change the commands. The test includes Easy, Normal and Hard seats.

This is native delivery-beat replay evidence. It does not establish equal
JavaScript and Rust AI order logs or whole-match hashes: native purpose streams
and schedules are independent under the approved parity criteria. Room AI
activation and recipient-only room adapters remain separate work. No World,
FFI, header or recipient field is added.

## Local AI planning assumptions (2026-10-16)

Every planning stage receives a copy of the seat's delivered facts with earlier
queued attempts mirrored into it. The copy debits buy fuel and manpower,
support currency and cooldowns, building and dig costs, and research costs.
Own unit rows mirror pending work, house entry, aimed fire, abilities and
stances. Cooldown and stance facts remain internal observations and add no
recipient fields. The supplied unit slices are remapped into each local copy
so later decisions cannot accidentally read the original rows.

Queued submission has no synchronous refusal receipt, matching
`shared/ai.js:260-287` and `shared/ai-human.js:168` at prototype `e4e579a`.
Actual command execution can refuse an attempt later. A new look starts from
its next delivered packet and discards the old assumptions. The large-army
shopping loop also reserves fuel on each purchase instead of admitting four
purchases against one unchanged fuel balance.

AI-12's component tests cover each wallet and own-unit flag family. Its
counterfactual shopping test gives enough manpower for four units but fuel for
one. Its delayed-garrison witness uses the production mirror adapter, proves
that the local entry assumption suppresses a later movement decision, executes
the real refusal on visible ground without a house, and runs six ordinary ticks
with snapshots for both seats. A fresh look releases the refused reservation.
This proves the mirror operation and its planning integration; AI-13's complete
command-emission union remains separate.

AI-43 stays held for pending #369. Current complete recipient composition refuses
live flights and undrained aviation events, and the existing aircraft witness
sets hold-fire to avoid them. That cannot qualify an unrestricted aircraft AI
mission. AI-13's complete emission union and AI-64's actual Classic, Assault and
Horde privacy matrix remain next work. IF-30 is a differs candidate: exact
cross-language six-AI end hashes conflict with the approved independent random
streams and outcome-based whole-match comparison. This change claims no new
whole-match JavaScript equality or parity status.
