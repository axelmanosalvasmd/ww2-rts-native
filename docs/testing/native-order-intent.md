# Native order-intent tests

## Native order intent batch

[Axel's decision 6206861267](https://github.com/axelmanosalvasmd/ww2-rts-native/issues/5#issuecomment-6206861267) approves native behavior tests plus prototype intent review, without new Source captures. These tests use the ordinary native constructor, commands and tick runner. At producing head `eddedccdf5f07186cdb238f1407f9816be2a0bba`, the focused native batch ran all nine tests once: six passed and three failed. No row is ported before prototype review.

All literal names below have prefix `native_order_intent_tests::` in `rust/crates/core/src/native_order_intent_tests.rs`.

| Row | Native test names | Prototype read at e4e579a | Remaining qualification |
| --- | --- | --- | --- |
| `CMB-83` | `cmb83_stance_validation_owner_flags_and_explicit_attack` | shared/sim.js:3333-3334,3574-3579,5900-5903; test.js:1226-1250 | Native PASS; reviewer prototype confirmation pending. |
| `CMB-91` | `cmb91_mov49_attack_move_halts_kills_resumes_and_finishes` | shared/sim.js:5449-5455; test.js:724-740 | Native PASS; reviewer prototype confirmation pending. Shared with MOV-49. |
| `CMB-140` | `cmb140_stop_clears_live_orders_and_paid_build_work`; `cmb140_cmb144_aircraft_stop_patrols_here_and_retreat_flies_home` | shared/sim.js:3511-3513 | Both native tests PASS; reviewer prototype confirmation pending. They exercise live attack/attack-move/queued orders, paid building and project cancellation, and aircraft patrol intent. |
| `CMB-144` | `cmb144_retreat_clears_group_orders_sprints_takes_quarter_damage_and_arrives`; `cmb144_retreat_cancels_paid_build_and_project_work_without_refund`; `cmb140_cmb144_aircraft_stop_patrols_here_and_retreat_flies_home`; `cmb144_foreign_construction_metadata_does_not_block_unrelated_retreat` | shared/sim.js:3335-3344,3571-3573; test.js:377-389 | HOLD. Foreign-construction FAIL repeats `Err("unrepresented or inconsistent construction activity")` versus `Ok(Some(Needs))`. Group Retreat FAIL reaches the authored 3 m home-distance assertion after its quarter-hit/sprint/no-fire assertions pass. That bound is not an established runtime defect. Paid build/project cancellation and aircraft PASS. Historical 0290fcf6 failure is unchanged. Garrison exit and capture exclusion remain uncovered. |
| `CMB-145` | `cmb145_scuttle_filters_carried_foreign_dead_and_retires_without_refund_or_bounty`; `cmb145_scuttle_after_ordinary_unload_retires_without_refund_or_bounty` | shared/sim.js:3760-3765 | HOLD. Ordinary-Unload scuttle PASS. Loaded scuttle FAIL at tick 786: `thrown carrier ejection tick phase is not connected`. No guard is bypassed; the unloaded result does not complete the loaded branch. |

The focused batch included the ignored CMB-144 regression and ran all nine selected tests with `--no-fail-fast --retries 0`. Failed or incomplete rows remain in progress while independent passing rows await prototype review. The earlier Source captures, failed native controls and producing-head results remain unchanged. Root owns literal execution grants, prototype review and publication. Under the 03:35 test-PR/status-PR split, test PRs leave every parity file unchanged; merged proofs await a separate status PR and Root-owned ledger entry.

Normal results: `/tmp/native-feature-batches-0303/aligned_replay/native.log` and `RESULT.json`. The runner and systemd launcher ended with exit 100 and confirmed inherited flock release before row reporting. The earlier 0258 invocation ended at argument parsing with exit 2 and no compilation or native tests; it remains separately preserved. The producing result stays bound to Rust tree `e67a66462828168f5fae6a519ffb38752ed8e321`. The later source-only scope child adds ignore attributes to the two newly failed diagnostics, with their exact actual-failure reasons. Removing those two attributes reconstructs the complete tested module byte for byte. The original foreign regression remains ignored. No body, input or assertion changes, semantic fix or retry are included. The new source child has not been executed.


## Native attack-move intent

MOV-49 maps literally to `native_order_intent_tests::cmb91_mov49_attack_move_halts_kills_resumes_and_finishes` in `rust/crates/core/src/native_order_intent_tests.rs`, shared with CMB-91. The test orders real native units to halt for a shootable target, kill it, resume and finish within 2.5 m. A separate plain-Move control continues past the enemy. Prototype references: shared/sim.js:5449-5455 and test.js:724-740 at e4e579a.

The native test PASS belongs to producing `eddedccdf5f07186cdb238f1407f9816be2a0bba`, Rust tree `e67a66462828168f5fae6a519ffb38752ed8e321`. Reviewer prototype confirmation is pending. MOV-49 has no status change here under [Axel's approved native-intent rule](https://github.com/axelmanosalvasmd/ww2-rts-native/issues/5#issuecomment-6206861267). No new Source capture or provenance bundle is required or created. Root may integrate this own test delta independently; a later status PR cites the merged test PR after reviewer confirmation.

## Task-only integration boundary

The task starts after `0290fcf602eb5cc9f030c862358e5cfa2b836560`. Integrate only the forward delta from that parent to the final source child, never this entire inherited unpublished branch. Its Rust delta is the new `native_order_intent_tests.rs` module and its two-line `cfg(test)` registration in `lib.rs`. There is no shipping runtime change. The normal DESIGN and CHANGELOG additions and this mapping document accompany that own delta. Both parity files are byte-identical to the pre-task parent, including all other authors' rows.

Four candidate rows have passing native checks on the producing head: CMB-83, CMB-91, CMB-140 and MOV-49. CMB-144 and CMB-145 are excluded. Prototype confirmation and final integration-head qualification remain Root-owned. This preparation runs no executable job and makes no public or ledger change.
