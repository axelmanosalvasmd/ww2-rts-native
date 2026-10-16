# Ordinary scenario objectives and announcements

Prototype baseline: `reference/ww2-rts` at `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`.
Owner authority: decisions N/P in the stream brief, relaying issue #5 comment 6217590741.
The recipient fields below are identical to the prototype. This document adds no C ABI fields.

## Admitted execution

Default and Conquest matches admit authored scenarios with no authored groups, match-scoped once-only triggers, and Say, objectiveActivate and objectiveComplete actions. Conditions are elapsed match time. Unsupported actions, repeat modes and scopes refuse before match creation. World, Horde, Tutorial, Classic and Annihilation admission stays closed for authored scenarios. Legacy timed triggers keep their existing executor.

Announcement text may be a string, an array, an object with `en`, or a single-translation object. Multiple translations without `en` remain refused because their fallback requires the prototype's object insertion order. No authored mission ships in this batch and public map rows continue to omit private scenario definitions.

## Recipient snapshot

`shared/scenarios.js:171-174` emits exactly:

```json
{"version":1,"objectives":[{"id":"front","scope":"match","text":{"en":"Hold"},"state":"active","recipients":"team","side":0,"updated":0.05,"execution":"scenario:1"}]}
```

The outer `scenario` field appears only when scenario state exists. Objectives retain first activation order. Each objective carries `id`, `scope`, `text`, `state`, `recipients`, `side`, `updated` and `execution`. Text retains its authored JSON shape. Time uses match seconds and execution identities use `scenario:<number>`. These fields are assigned at `shared/scenarios.js:161-164`. Completing an objective is final; later activation does not replace it.

At `shared/scenarios.js:87`, `public` reaches every seated recipient, `side` reaches only the named slot, and `team` reaches slots whose current team equals the named side's team. Fog, reveal and spectator selection do not override these seated recipient rules. This batch admits ordinary seated recipients only. Reading a snapshot neither advances triggers nor changes objectives. Objectives never choose the match winner.

## Announcement shots

`shared/scenarios.js:159-160` emits exactly:

```json
{"k":"say","text":"Hold","localized":{"en":"Hold"},"scenarioRecipients":"team","scenarioSide":0,"pub":false,"execution":"scenario:1","atTick":1}
```

`localized` retains the authored text value, including a string when the author used a string. `text` uses that string, its `en` translation, or its first translation. `scenarioRecipients` inherits from the action, trigger, then `public`; `scenarioSide` inherits from the action, trigger, then context slot (`shared/scenarios.js:86,159`). `pub` is true only for public announcements. `atTick` is the simulation tick and `execution` is the trigger execution identity.

`shared/scenarios.js:175` and `shared/sim.js:6080` apply the same recipient policy before publishing shots. Private announcements bypass fog only for permitted recipients. They are never made public by reveal. Scheduled sends retain the existing chronological batch order and reset behavior; current/reconnect reads do not execute or consume a scenario.

## Native checkpoint state

The optional `Match.authored_scenario` owner contains the validated definition `data`, executed trigger identities `executions`, insertion-ordered `objectives`, and `next_execution`. It uses the existing tagged checkpoint codec. Older checkpoints with this owner absent deserialize as `None`. The existing `scenario` owner continues to carry legacy triggers; both owners cannot be active together. The additional `TickEvent.AuthoredScenario` variant retains pending announcements through checkpoint save/load. Room saves containing authored missions use envelope version 2. Existing non-authored room saves remain version 1, and the new loader accepts both versions. A version 1 envelope carrying authored state is refused. Older binaries refuse version 2 before decoding the state, preventing silent loss of mission state.

The native state decomposition differs from JavaScript's single `g.scenario` object; recipient fields and behavior remain identical within the admitted subset (`shared/scenarios.js:88-98,136-175`).

## Verification

`scenario_recipient_tests::mode86_ordinary_room_filters_scenario_reads_without_execution_and_keeps_normal_victory` exercises a real RoomRuntime with three seats, two allied. Literal assertions check all objective and Say keys, recipient filtering, localization, insertion order, completion, reveal, repeated current reads, authoring/start binding, pending-event and objective checkpoints, ordinary VP victory, and the 120-callback ending hold. Every advancing ending step collects all three recipient snapshots and asserts no room fault.

`authored_objectives_save_v2_rejects_downgrade_and_restores_mission_and_pending_say` checks actual room save/load version selection, atomic downgrade refusal, restored objective state and the pending announcement.

MODE-81, MODE-82, MODE-83, MODE-84, MODE-85 and NET-55 remain unproved by this batch. General conditions, contexts, repeats, damage and reinforcement actions need their own runtime work. No parity status changes are included.
