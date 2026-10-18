# RESULTS-TUTORIAL

This branch stacks on #426 and #387. It completes the Tutorial parts of MODE-90 and MODE-96. Ordinary and hosted Horde coverage is inherited and rerun. World remains deferred under owner decision C; no World room is admitted. No parity-status files, FFI/header, prototype or Source recorder changes.

## Each row literal test mapping

| Row | Literal Tutorial proof | Inherited coverage |
| --- | --- | --- |
| MODE-90 | `lobby::tutorial_room_tests::mode90_mode96_tutorial_publishes_complete_results_and_exact_snapshots_through_hold`; `lobby::tutorial_room_tests::mode90_tutorial_host_end_publishes_only_ended_for_every_human` | `results2_tests::mode90_complete_ordinary_results_publish_story_timeline_and_each_seat_outcome`; `results2_tests::mode90_host_end_publishes_only_ended_and_clears_pause_after_live_snapshots`; `horde_host::tests::hosted_results::mode55_mode90_net94_hosted_completion_records_and_sandbox_exclusion` |
| MODE-96 | `lobby::tutorial_room_tests::mode90_mode96_tutorial_publishes_complete_results_and_exact_snapshots_through_hold`, including `assert_tutorial_snapshot` on each core frame | `results2_tests::mode96_ordinary_modes_deliver_exact_rows_through_victory_and_full_hold`; `horde_host::tests::hosted_results::mode96_horde_exact_break_active_boss_and_later_break_rows`; the hosted completion/hold tests |

The Tutorial journey checks all nine literal goals and exact field omission, all game-seat `out` entries, absence of `end` before victory, and its exact reason/coordinates afterward. It retains real movement, capture, recruitment, grenade, garrison and artillery lifecycles. Scripted fights use explicit resolution, as the prototype Tutorial test does. Every game seat receives snapshots during progression. The human socket receives all 60 scheduled ending frames while Enemy receives current frames during the hold. The runtime releases only after callback 120.

Result proof compares the full object, including rounded values for all seven counters in both story rows, frozen end tick/timeline, live human name after reconnect, Enemy, `winVp:null`, and the human victory outcome. A separate two-human room verifies that a nonhost cannot end the match, while host End publishes `{ended:true}` and no story or outcome, including when both seats retain outcomes from an earlier match.

[Merge record](results-tutorial-merge-20261016.md) lists every conflict and additional manual edit. [Tutorial contract](../contracts/tutorial-room-start.md) cites prototype e4e579a file/line. Owner groups E/O, M, N and P authorize the inherited prototype fields and commands; no new wire meaning is introduced. The latest #5 merge-process decision (comment 6225924617) is acknowledged. This stream stops after PR publication; CI and merging belong to the orchestrator.

Both assigned rows are covered; no rows are dropped and no differs decisions are proposed.
