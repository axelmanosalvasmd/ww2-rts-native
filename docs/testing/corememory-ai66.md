# AI-66 native random streams

Scope: CORE-MEMORY-ORDER. Prototype pin: `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`.

The prototype planner selects a cover cell or an open-ground angle with one random draw (`shared/ai.js:146-155`). Its Classic construction object literal also evaluates the flak angle for every eligible engineer when the next building is not a depot, including fixed-angle buildings and failed site or budget checks (`shared/ai.js:1022-1037`). The commander mixes the seat into its seed on first use and takes two mulberry32 draws for every Box-Muller timing sample (`shared/ai-human.js:40-65`).

The native planner retains its approved independent `StreamId::Ai(slot)` purpose stream. Commander motor timing retains its existing private mulberry32 stream and deterministic core math. These tests do not assert a shared global stream or whole-match equality with JavaScript.

| Row clause | Intent test |
| --- | --- |
| Cover selection and open-ground angle, one planner draw per destination, seat isolation | `commander_tactics::tests::intent_ai66_planner_cover_and_angle_use_one_seat_draw_per_destination` |
| Classic site draw once per eligible engineer, including fixed-angle buildings, insufficient money and unseen sites; no draws for repair, retreat, active construction, depots or no next building | `commander_construction::tests::intent_ai66_classic_site_draws_once_per_eligible_engineer_even_without_build` |
| Per-seat seed mixing, mulberry32 order and exactly two draws per timing sample | `human_commander_timing::ai66_tests::intent_ai66_seat_seed_and_every_box_muller_sample_consume_two_draws` |
| Classic recipient projection requires its matching canonical mode and elimination state; quiet opening policy remains Conquest-only | `human_commander_policy::tests::intent_ai66_classic_human_view_requires_matching_mode_and_keeps_quiet_policy_narrow` |
| Actual ordinary seat initialization, timing, delayed input admission, Classic construction, every-seat recipient snapshots and Conquest terminal continuation | `human_commander_policy::tests::intent_ai66_private_streams_drive_ordinary_ai_ticks_and_every_seat_snapshot` |

The existing registered `human_commander_timing::private_human_timings_match_original_source` integration test compares all timing operations against committed prototype component fixtures. The existing AI-19 reset lifecycle test remains the separate human handover/reset witness.

The lifecycle uses authored water between the opposing ground armies and delivered support cooldowns above the test duration. It retains every actual event in snapshot capture. Its Conquest point is near the first spawn, so ordinary capture income ends the match before a combat encounter. The Classic lifecycle begins with two existing depots per seat, placed through the core startup-building helper, so it reaches non-depot construction without implementing node-economy planning. The shared observation validator now admits matching ordinary Classic state for the human recipient provider, including canonical elimination validation. The public quiet opening policy keeps its Conquest-only guard. The Classic runtime repair uses only delivered observations, the existing visible site search and canonical command admission. It adds no stored field or command kind. The repair priority uses stable first-minimum distance, protected by `commander_construction::tests::if25_classic_repair_ties_keep_observation_order_for_each_engineer`.

World remains on its existing detached construction reserve path. This work does not admit a World room, add resource-node economy planning, change the FFI, or establish whole-AI input-log parity. Classic depot construction remains outside this change; the no-draw depot branch is tested.

The coordinated run at `74986341` passed the planner, Classic draw-count and timing component tests. It exposed a rush escape regression and an unrepresented shot event in the lifecycle scene. The scoped repairs preserve threatened engineers for the existing later rush escape command and bound the lifecycle to authored separated ground armies. The final producing-head qualification receipt is recorded in the PR body. Tests use existing registered modules; no registration edit is required.
