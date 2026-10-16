# Five engineering rows: native intent map

Feature: paid queued fieldwork, reservations and cancellation. Range: ENG-15, ENG-16, ENG-17, ENG-23 and ENG-24, exactly five rows. Root combines this contribution with IMPL_ENGINEERING's 36 rows in one 41-row engineering batch after PR272. No separate engineering PR is proposed.

[Axel's owner decision 6206861267](https://github.com/axelmanosalvasmd/ww2-rts-native/issues/5#issuecomment-6206861267) approves [rules 1-5](https://github.com/axelmanosalvasmd/ww2-rts-native/issues/5#issuecomment-6206834165) without added conditions. Behavior rows use native tests plus prototype intent review. Exact comparison remains for data tables, pure functions and single-component fixtures. Hermes comment 6206844573 adds no adopted condition.

The table below maps the existing fourteen native checks to the assigned row intents. Prototype references were read at `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`, directly from Git objects because this physical checkout has no populated reference submodule. This replacement author reread the prototype and native bodies. Root owns the reviewer confirmation and future integrated-head qualification.

| Row | Literal native tests | Prototype intent read | Native behavior checked |
| --- | --- | --- | --- |
| ENG-15 | `malformed_dig_names_do_not_start_work_or_spend_manpower`, `queued_dig_defers_payment_and_skips_unaffordable_work`, `queued_dig_preserves_paid_work_and_tallies_only_actual_activation`, `queued_dig_validates_placement_and_capacity_without_reserving_manpower`, `queued_dig_errors_preserve_queue_drift_wallet_story_and_views`, `queued_dig_uses_real_single_fort_reservations_and_rechecks_at_activation`, `admission_guards_reject_before_payment_or_cancellation`, `queued_dig_losing_sight_or_ground_skips_without_payment_and_continues_move` | `shared/sim.js:3635-3650`; `test.js:349-357`, `6025-6057` | Malformed names refuse without payment/work. Waiting Dig preserves paid work, normalizes the first unit and direction, reserves no MP, checks placement/capacity and hospital reservations, pays/tallies once at activation, finishes cells and physically continues. Unaffordable or invalidated work skips; fallible activation preserves full state. |
| ENG-16 | `hidden_placement_and_queued_sight_loss_do_not_reveal_or_pay`, `queued_dig_losing_sight_or_ground_skips_without_payment_and_continues_move` | `shared/sim.js:459-469`, `3264-3276`; `test.js:1364-1388` | Hidden ground/building/water and partial sight refuse without exposed cells or payment. An intact scout permits activation; actual scout deletion or ground loss prevents payment/work and permits the following Move. |
| ENG-17 | `five_segment_line_pays_once_per_segment_and_completes_twenty_cells`, `field_projects::original_shared_pattern_order_front_and_segment_prices`, `mass_line_waits_for_manpower_and_replacement_move_leaves_it_unfinished` | `shared/sim.js:512`; `test.js:1428-1466` | Three workers receive distinct segments, including the middle. Five segments complete twenty cells and spend 150 MP without component income. A 45-MP purse finishes one segment, waits, then resumes after explicit refill; replacement Move leaves work unfinished without refund. Existing pure-function comparisons retain supported costs. |
| ENG-23 | `strongpoint_completes_twelve_trench_and_ten_forward_wire_cells`, `field_projects::original_shared_pattern_order_front_and_segment_prices` | `shared/sim.js:499-506`; `test.js:1358`, `1468-1474` | Ordered plan contains four Trench then two Wire segments. Actual work finishes twelve trench and ten forward wire cells and releases the workers. Existing pure-function comparisons retain plan geometry/order. |
| ENG-24 | `trap_line_executes_and_non_line_materials_are_refused`, `field_projects::original_shared_pattern_order_front_and_segment_prices`, `mass_line_waits_for_manpower_and_replacement_move_leaves_it_unfinished` | `shared/sim.js:449`, `508-509`, `3661-3664`; `test.js:1350`, `1452-1466`, `1504-1512` | Middle-first allocation, fifteen wire cells, four trap cells, retained wire, bridge/nest/unknown/ring-wire refusals, purse waiting and cancellation by replacement Move. Existing supported plan/material comparisons remain exact. |

No new native control is needed for these five assigned checks after the intent readback. This does not extend the assignment to the other clauses of the shared-project callback, ENG-25/26/27, client preview, bridge/fill/concrete execution, public ingress, owner-view kind 7 or the MOV28 bridge proposal. Their existing limits remain explicit. Intentional player-visible differences still require DESIGN, `differs` and Axel's agreement. Runtime integration still requires Hermes's exact-head acknowledgment.

## Existing native selection

Checks 1-11 and 13-14 use the core library target. Checks 1-6 and 13-14 have prefix `field_works::match_projects::tests::original_checks::`; checks 7-11 have prefix `field_works::match_engineering::tests::`. Check 12 is the core `it` target with the full selector shown. All fourteen are enabled existing tests. The library includes `match_engineering_tests.rs` through `match_engineering.rs:432-433` and the original checks through `match_projects_tests.rs:26-27`; `tests/it/main.rs:32-33` registers the pure-function test with the consolidated `it` target (`autotests = false`).

1. `hidden_placement_and_queued_sight_loss_do_not_reveal_or_pay` (rust/crates/core/src/match_projects_original_checks.rs:84)
2. `five_segment_line_pays_once_per_segment_and_completes_twenty_cells` (rust/crates/core/src/match_projects_original_checks.rs:142)
3. `strongpoint_completes_twelve_trench_and_ten_forward_wire_cells` (rust/crates/core/src/match_projects_original_checks.rs:211)
4. `trap_line_executes_and_non_line_materials_are_refused` (rust/crates/core/src/match_projects_original_checks.rs:316)
5. `malformed_dig_names_do_not_start_work_or_spend_manpower` (rust/crates/core/src/match_projects_original_checks.rs:369)
6. `queued_dig_defers_payment_and_skips_unaffordable_work` (rust/crates/core/src/match_projects_original_checks.rs:388)
7. `queued_dig_preserves_paid_work_and_tallies_only_actual_activation` (rust/crates/core/src/match_engineering_tests.rs:374)
8. `queued_dig_validates_placement_and_capacity_without_reserving_manpower` (rust/crates/core/src/match_engineering_tests.rs:466)
9. `queued_dig_errors_preserve_queue_drift_wallet_story_and_views` (rust/crates/core/src/match_engineering_tests.rs:687)
10. `queued_dig_uses_real_single_fort_reservations_and_rechecks_at_activation` (rust/crates/core/src/match_engineering_tests.rs:843)
11. `admission_guards_reject_before_payment_or_cancellation` (rust/crates/core/src/match_engineering_tests.rs:229)
12. `field_projects::original_shared_pattern_order_front_and_segment_prices` (rust/crates/core/tests/field_projects.rs:19)
13. `mass_line_waits_for_manpower_and_replacement_move_leaves_it_unfinished` (rust/crates/core/src/match_projects_original_checks.rs:232)
14. `queued_dig_losing_sight_or_ground_skips_without_payment_and_continues_move` (rust/crates/core/src/match_projects_original_checks.rs:466)

Native sources: `rust/crates/core/src/match_projects_original_checks.rs`, `rust/crates/core/src/match_engineering_tests.rs` and `rust/crates/core/tests/field_projects.rs`. The component clock calls canonical command preparation, project allocation, paid work and physical movement. It omits income/combat; it imports no expected state or replacement RNG to finish work.

## Qualification boundary

At historical producing head `b28ca8e531426cc240103b6936a8abd8cb68986d`, Rust tree `866795e6e303acc529e53381e45c7fa4b6b37baa`, all fourteen exact selections passed once, formatting exited 0 and strict Clippy exited 0. The private docs child `a4892b834c69433e06b71d27bc9edf6c30843253` retains that Rust tree. Prior failures and their raw outputs remain unchanged. Historical passes do not qualify the future integrated head.

No new Source recorder, capture, fixture or provenance bundle is created or required for these behavior rows. Existing exact pure-function/fixture comparisons and optional historical capture remain intact. Root supplies the literal incoming main and integration instruction, then a NEW exact-head grouped batch grant. Future qualification uses one focused Cargo/nextest job for the batch, with `--no-fail-fast` to isolate failed rows, no semantic retry and no job per row. Failed/incomplete rows stay in progress with reasons while other rows continue. No project execution occurs in this documentation change.
