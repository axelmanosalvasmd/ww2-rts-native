# Core memory and recipient composition

This stream stacks on #392 (`084cbe3d9344e9687a903c001333d8e289d67e97`).
It merges #396 (`bc9088f7b7a616795d0581527bdee3085e5b716e`) for Classic
building ghosts, #394 (`d2b7e3b772930c72b26c43363512cd2ca3224e08`) for mission
recipients and #336 (`5057a6b50084220ab5f9cb45908486ae7e7f6f62`) for fort
construction and the retained default capture refusal. These components are
needed by the assigned rows. The #373 prerequisite is fulfilled: its hotfix is now on main.

Acknowledged locally, without issue writes: owner decisions C and L at #5
comment 6217590741 and Axel's confirmation 6218552386. AI reads the recipient
view only. World room admission remains closed. ABI 2, canonical meters and
JSON WebSocket v0 retain their existing meaning. Runtime exact-head owner
acknowledgment, hosted CI, parity status and merge belong to the orchestrator.

## Each row literal test mapping

- MAP-100 (non-World):
  `room_runtime::core_memory_order_tests::map100_ordinary_reconnect_rescout_and_hidden_counterfactual_keep_delivered_memory`.
  A three-seat Conquest match receives an incremental changed-road row, withdraws its allied
  scout and retains the observed row. Two restored branches differ only in
  hidden terrain and enemy health, yet complete allied snapshots and reconnect
  start envelopes remain equal. Rescouting delivers the current road and elevation;
  never-observed changes remain absent. Each advancing tick delivers every
  seat. The authored map remains public in ordinary modes, as in
  `server.js:463-465`; remembered changes arrive in `cells`.
- NET-55 (remaining ordinary composition):
  `room_runtime::core_memory_order_tests::net55_classic_nonempty_team_ghosts_compose_with_mission_recipients_through_finish`.
  The shared team's scout genuinely observes an enemy building, leaves and
  receives a nonempty six-field ghost while a private objective and Say arrive.
  The opposing seat receives neither private mission rows nor those ghosts.
  Hidden completion changes preserve the ghost; rediscovery replaces it with
  the live building. Checkpoints preserve ghosts and objectives. A normal
  Classic finish and its 120-callback hold continue collecting every seat.
  Complement: #394's
  `net55_scenario_contexts_inherit_and_override_recipient_policy_without_reveal_leaks`
  covers player/team/match policies and nonempty covers, works and found mines.
- NET-55 admission guard:
  `room_runtime::core_memory_order_tests::net55_classic_mission_recipient_extension_keeps_spawn_and_damage_actuators_refused`.
  Classic admits objective and announcement actions only; its authored groups,
  reinforcement and terrain damage remain outside this composition.
- BASE-54: [detached component mapping](corememory-base54.md).
- AI-66: [RNG and recipient-only lifecycle mapping](corememory-ai66.md).
- IF-25: [complete inventory and bounded consumer evidence](corememory-if25.md).

MAP-100's **World reconnect clause is deferred** under C: "World admission
 deferred; reopen when World opens". World region envelopes, World geography
 masking and World reconnect spawns are not proven by the ordinary test.
NET-55's World delivery clause likewise remains deferred. Detached BASE-54
capture evidence does not open a World room or its tick path.

## Prototype and runtime boundaries

The source pin is `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`.
`test-world-acceptance.js:222-264,300-336` supplies the reconnect/rescout and
hidden-counterfactual intent. The ordinary collector and snapshot paths are
`shared/sim.js:5843-5880,6073-6074`; ordinary reconnect is
`server.js:455-468`. Team ghost selection is `shared/sim.js:6036-6037`, with
team building memory at `:4441-4456`. Objectives and Say use
`shared/scenarios.js:159-175`. No Source recorder or capture fixture was added.

Classic previously rejected every authored mission. It now permits only empty
groups with objectiveActivate/objectiveComplete/Say actions, through the same
validated authored state, scheduling, snapshot, checkpoint and normal ending
owners as Conquest. The actuator preflight independently refuses other Classic
actions. Public Classic network startup and World room admission remain closed.
The ordinary raw-list cache now accepts the same Classic mode pair as the recipient adapter, fixing the assembled RoomRuntime and checkpoint guard. No field or message shape is added. The recipient contract update names this
additional composition and the unchanged prototype fields.

## Merge record

#396 merged automatically. #394 had one textual conflict at the appended
sections of `docs/contracts/save-format.md`: retained both external AI codec
and authored schema sections, and clarified that the AI codec itself does not
increment the envelope version (non-authored v1, authored v2). All Rust and
registration hunks merged automatically. #336 merged automatically.

Main `b113bb959f95499f04b47213bd27efb805f18e95` was merged automatically
in `f7f01d2ae3154e307493450f299f7506b612d50a`. A merge-tree comparison found
no manual differences. The incoming changes are the #381/#382 startup and
Tutorial tests and their changelog fragments. The refreshed #5 message
`julio-373-merged-decisions-20261016` confirms the hotfix merge and keeps
deferred World clauses named for R1. No parity status is edited here.

The BASE-54 document lists every additional hand edit required to reconcile
inherited #301 positive World-fort expectations with #336's retained refusal.
These are separate from the automatic merge results.

## Validation

At `74986341f62a650758905423a233fdd6f73b872d`, 203 focused tests compiled
and ran: 199 passed and four failed. The complete log is
`/tmp/corememory-qualified-v2.log`. MAP-100, all six BASE-54 constructor/capture
checks, positive detached MODE-74 checks and restored World refusal tests passed.
Failures exposed Classic raw-list admission, rush engineer assignment, the AI
witness's unsupported events, and a Classic production fixture using the wrong
producer. Repairs and renewed qualification follow; this partial run alone does
not qualify the final tree. The preceding `400ce57d` attempt failed compilation
on two new test assertions; both were corrected before this run.

The final producing-head qualification receipt belongs in the PR body. This
record preserves the earlier failing run and its exact scope; it does not
transfer those results to a successor tree.
