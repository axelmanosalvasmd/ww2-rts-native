# NET-43 remaining command union

This batch extends the existing command-union owner. Runtime edits admit Attack,
Ability, Autocast, Build, Board, Unload, Escort, Rail and grouped Orders through
the sole queue. Production cancellation now reaches the canonical mode-aware
executor. It removes obsolete server ground-only restrictions and connects
Classic snapshot admission and waiting Ability projection to existing owners.

## eachrowliteraltestmapping

| Row | Literal tests | Scope |
| --- | --- | --- |
| NET-43 | `net43_union_guest_pause_stamps_malformed_and_exact_recipient_local_denials` | Every newly admitted family, plus cancellation, uses stable guest authority, ignores forged issuer/tick/sequence, drops paused input, rejects malformed rows before reserving input, produces the exact prototype denial only to the issuer, and survives later ticks with every seat projected. |
| NET-43 | `net43_attack_and_grouped_orders_keep_selection_and_parent_queue_semantics` | Successful guest Attack, missing/foreign selection filtering, grouped parent queue override, partial success, ignored disallowed group parts and retained waiting Attack. |
| NET-43 | `net43_ability_autocast_and_waiting_kind_six_survive_room_delivery` | Successful guest Autocast, foreign selection exclusion, waiting aimed Ability with exact kind-6 owner row, private orders, immediate aimed Ability, sustained approach, actual throw/flight/expiry and later room frames. |
| NET-43 | `net43_board_unload_and_aircraft_escort_run_through_ordinary_rooms` | Actual boarding, riding/cargo state, mixed aircraft Escort selection, successful Unload, aircraft Move/Stop and later room frames. |
| NET-43 | `net43_build_and_worker_replacement_remain_live_after_admission` | Paid guest Build creates its site through the queue, retains construction through frames, accepts AutoRetreat and lets Stop clear the canonical assignment. |
| NET-43 | `net43_classic_waiting_production_cancels_refunds_and_delivers_every_seat` | Actual authored Classic attachment with logistics disabled, two queued purchases, original waiting-job refund, active-job `notWaiting`, foreign `unseen`, owner-only job identities, Classic currency/mode/node rows, natural HQ loss through the queue, 120 ending callbacks with 60 frames per seat, and the final Classic result and territory timeline. |
| NET-43 | `net43_classic_attachment_keeps_mode_map_binding_and_refuses_node_rounding_overflow` | Preserves authored map and mode binding, rejects overflow during node rounding without publication, and retains World snapshot refusal. |
| NET-43 | `net43_malformed_tags_preserve_exact_feedback_and_pause_ready_gates` | Numeric, array, object, null and absent command tags, plus root arrays, preserve exact issuer-only feedback without reservation; pause and connection readiness retain silence. |
| NET-43 | `net43_malformed_tags_keep_spectator_operative_stale_and_lobby_inputs_silent` | Pre-hello, spectator, operative, genuinely replaced and unknown connections receive no command feedback; lobby and primitive messages retain silence, with later snapshots for every seat. |
| NET-43 | `net43_raw_and_string_denials_share_the_same_rolling_four_reply_limit` | Raw and normal string command denials share the existing four-reply rolling window, including exact 999/1000/1100 ms boundaries and no enqueue. |
| NET-43 | `queued_ability_uses_prototype_kind_six_rounded_anchor_and_owner_privacy` | Exact tenths rounding, singleton normalized Ability projection, private orders and refusal of malformed stored variants. |

The existing FireAt/convoy room tests and the prior R1 command tests retain their
families, alongside the composed sandbox, operative, support, pause and Horde
coverage. Rail's ordinary outcome remains the prototype's `blocked`; successful
World rail/recovery is deferred under owner decision C.

Tests use the canonical authored `default` map. Additional actors are declared
through the canonical staged unit publication API before attachment. Build
placement is discovered in a separate deterministic fixture; the live room
receives the site only through its public queued command. Classic is attached
through the normal native startup validation, with no snapshot bypass. Its
cancellation test then destroys the opposing HQ through the ordinary command
queue and observes canonical elimination, the complete ending hold and lobby
results. No finish state is assigned by the test. Every
ordinary test callback checks room fault state and projects every seat; later
callbacks also require actual frame delivery to both seats.

## Rewritten tests

- `prospective_unsupported_exits_and_bad_rows_do_not_reserve_keys` becomes
  `supported_stance_is_queued_and_malformed_rows_do_not_reserve_keys`. It keeps
  malformed-input immutability and no-reservation assertions, then proves the
  supported stance through the queue. Construction replacement is covered by
  `net43_build_and_worker_replacement_remain_live_after_admission`.
- `conquest_production_cancel_is_refused_without_reserving_input` becomes
  `conquest_production_cancel_uses_canonical_denial_after_enqueue`. The exact
  Conquest `needs` result remains; it now comes from the recorded executor.
- `compound_orders_remain_blocked_at_public_ingress` becomes
  `compound_orders_use_one_identity_and_preserve_canonical_first_denial`.
  Input remains immutable until the canonical tick and consumes one queue key.
  The room matrix pins the exact first group denial, and the successful group
  test pins partial success and parent queue precedence.
- `queued_owner_adapters_refuse_unnormalized_or_unrepresented_orders` now refuses
  a missing-coordinate Ability instead of a normalized aimed Ability, which is
  representable as the prototype's existing kind 6.

## Verification

The implementation agent ran no Cargo commands, by task instruction. Compilation,
focused tests and final-head checks belong to the parent integration run. No
passing execution or full NET-43 qualification is claimed by this note alone.

## Observed aimed-approach fault and repair

The parent run at frozen head `be3ff5c7` selected 70 focused tests: 67 passed and
three failed (`/tmp/roomcmds2-focus4.log`), including the NET-43 ability room
fault. A diagnostic rerun exposed the exact tick-12 reason:
`air, rail, operative and aimed ability firing are unsupported`.

The ordinary unit loop calls `step_aimed_ability` before movement and ordinary
firing. An out-of-range aimed ability remains active while its unit approaches;
the old firing guard then rejected that valid pending state. The prototype
walks the aimed order at `shared/sim.js:5416-5430` and still runs ordinary
targeting and firing at `shared/sim.js:5557-5580`, so the repair removes only the
pending aimed-state restriction while retaining the aircraft, rail and operative
firing guards.

The same room test keeps all existing assertions and uses a target 30 metres
from the original actor pose so the direct order remains pending across six
ordinary approach callbacks, asserting movement during that interval before
requiring the real throw, retained grenade flight, cooldown and flight/ticket
expiry. Every callback still checks room fault state and snapshots for both
seats; this agent ran no Cargo, Source or Node commands, and post-repair
execution belongs to the parent run.
