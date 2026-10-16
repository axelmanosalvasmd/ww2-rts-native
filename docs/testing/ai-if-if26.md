# IF-26 assessment, 2026-10-16

IF-26 is dropped from this batch. Destination grouping alone does not establish the complete row, and the pending Horde plan comparison belongs to PR #343.

The prototype at `e4e579a838aabae3b1d9dadbc17a0a87147bc52d` uses numeric string keys for destination groups (`shared/sim.js:3434,4026`), a kind and coordinate key for path retries (`:3216`), rounded quarter-metre keys for supply handoffs (`:1906`), and JSON equality for observed terrain (`:5861`). `shared/ai.js:187` fingerprints the complete pending Horde plan with `JSON.stringify`. These consumers need separate comparisons for signed zero, NaN and nearby finite values, including their actual admission boundaries.

The inspected main base is `b6d05874078f183fc95421cd25e13762b253c57c`. Native destination grouping folds signed zero in `command_destinations.rs::groups`; its public request validation rejects nonfinite coordinates. `path_requests.rs` compares typed requests and rejects nonfinite goals. `supply_routes.rs::handoff` quantizes five floats into an integer cache key after finite input validation. `observed_memory.rs::ObservedRow::same_content` compares typed rows after `validate_row` rejects nonfinite row values. None of those facts supplies the missing complete comparison matrix, particularly the JSON plan consumer.

At inspected PR #343 head `31e301ee20d69a227ad570dea8f83d112d24aa52`, `horde_commander.rs::OwnPlan` uses derived typed equality and adds a native path generation field; `HordeCommander::flush` compares it with the stored plan. That PR explicitly reports this as an AI-18 differs candidate. Its pending design decision cannot establish IF-26, and this stream does not introduce a competing Horde queue or change its equality policy.

Completion needs consumer tests for construction-path retries, handoff cache keys and remembered JSON rows, plus reconciliation of the pending Horde plan comparison with the prototype's JSON semantics. No IF-26 implementation or tests were added, and no parity status changed.
