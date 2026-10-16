# SCENARIO-ENGINE-2 intent tests

Base: PR #386 at `fa383c9cfa38bd6a7ec88c2e397ea18ad07e1225`, including #383. Prototype: `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`. World admission stays closed. This continuation adds tests to the existing ordinary authored executor; it adds no wire fields, checkpoint fields or runtime capability.

## eachrowliteraltestmapping

- MODE-81: `lobby::scenario_start_tests::mode81_custom_candidate_validation_and_preparation_leave_lobby_and_live_room_unchanged`; `lobby::scenario_start_tests::mode81_trusted_catalog_refusal_denies_start_and_restart_without_publication`.
- MODE-82: `authored_scenario::engine_tests::mode82_contexts_repeats_cooldown_and_arrival_order_survive_checkpoint_and_finish`; `authored_scenario::engine_tests::mode82_out_contexts_and_disallowed_reinforcement_skip_the_entire_batch`; `authored_scenario::engine_tests::mode82_tick_budget_stops_later_triggers_and_recovers_on_the_next_tick`.
- NET-55 (assigned ordinary scenario scope): `room_runtime::scenario_recipient_tests::net55_scenario_contexts_inherit_and_override_recipient_policy_without_reveal_leaks`.

## Behavior and source

MODE-81 follows `shared/sim.js:810-822,4884-4923` and `server.js:364-389`. The selected roster has fewer seats than the authored map, so valid map-side references can be invalid match-side references. Cases cover objective, trigger, condition, authored-unit and reinforcement sides; unknown targets and types; faction, construction-only and naval type restrictions; authored blocked terrain; and a rubble entry masked by a mine. The valid mission constructs and publishes both start envelopes. Rejected candidates preserve lobby/live state, snapshot accounts, match identity and authoring data, without partial outbound starts. The old match keeps ticking and delivering both recipients.

The authored candidate matrix uses `Match::new` and the actual loaded-map preparation transaction. The separate trusted-command test proves literal `deny/start|restart/scenario` output and preservation on a missing catalog map. It does not inject mutable authored fixtures into the embedded map catalog or claim asynchronous startup coverage.

MODE-82 follows `shared/scenarios.js:100-170` and `shared/sim.js:5638`. Tests cover match/player/team contexts, a side-filtered team represented once, out-player filtering, once/risingEdge/whileTrue counts, cooldown, maximum counts, an edge consumed during cooldown, checkpoint continuity, arrivals after triggers and before the next evaluation, dynamic 4096-work exhaustion, recovery next tick, and whole-batch refusal when reinforcements target an out player. Every tick collects all seated recipient snapshots. Ordinary VP victory and ending ticks remain deliverable. Existing MODE-83/84/85 witnesses remain registered with their assertions preserved.

NET-55 follows `shared/scenarios.js:84-87,159-175` and `shared/sim.js:6036-6037,6055-6061,6071-6072,6080`. Literal objective and announcement assertions cover contextual identities, inherited side/team policy, action overrides, objective-definition overrides, public announcements, activation order and completion. Reveal cannot disclose private rows. Checkpoint restoration and current reads retain durable objectives without replaying announcements. Nonempty covers, pending field works and found mines are delivered to allies and withheld from the opposing seat; Conquest omits construction-mode ghosts. This claim is limited to the assigned ordinary scenario scope. Classic/World building-ghost delivery and World admission are not claimed.

No Source recorder, capture fixture or parity status edit is added. The existing limitation on multi-translation announcements without an `en` entry remains unchanged; it is documented in `docs/contracts/scenario-objectives.md`.
