# AI-PLANNING-2 evidence and remaining gates

Baseline: PR #391 at `b3492aa2360778df322419075a1dbd451242b93c`.
Prototype: `reference/ww2-rts` at `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`.
Owner decision L requires recipient-only room AI facts. This branch does not
activate a room AI path or change parity status tables.

## Row accounting

Neither complete row is claimed. The tests below provide specific prerequisites
for AI-13 and AI-64. Passing a subset must not promote either full row.

AI-13 ground outputs:

| Command family | Intent witness |
| --- | --- |
| Ground abilities: grenade, suppress, AP, smoke, satchel, Ura, barrage | `commander_actions::tests::ai13_ground_abilities_emit_for_each_family_from_visible_facts` |
| Ability notice, visibility, cooldown, cast count, budget and friends | The other `commander_actions::tests::ai13_ground_*` ability controls |
| Focus attack and queued onward attack-move | `commander_actions::tests::ai13_ground_focus_emits_attack_and_queued_resume_with_visibility_range_los_controls` |
| Focus release and weapon suitability | `commander_actions::tests::ai13_ground_focus_releases_visible_retreating_ranged_or_occluded_target`; `ai13_ground_focus_assigns_weapons_to_suitable_visible_targets` |
| House shelling output only | `commander_actions::tests::ai13_ground_fireat_shells_watched_occupied_house_and_reserves_shooter` |
| Bridge, fill and detected-mine repair | `commander_engineering::tests::ai13_repairs_emit_bridge_fill_demine_and_wait_for_observed_completion` |
| Repair approach, full stamp visibility, eligible workers and research priority | The `commander_engineering::tests::ai13_unseen_*`, `ai13_repair_stamp_*`, `ai13_engineering_*` and `ai13_research_*` controls |
| Point mines, arc, strongpoint, wire, traps and house entry | `commander_engineering::tests::ai13_point_holder_emits_mines_arc_strongpoint_wire_traps_and_garrison` |
| Persistent vehicle lesson for traps | `commander_engineering::tests::ai13_traps_keep_the_first_visible_vehicle_lesson_after_contacts_expire` |
| Headquarters wall and gate outputs | `commander_tactics::shopping_inputs_tests::intent_ai33_wall_plans_defer_payment_and_gate_reserves_its_price` |
| Ability/focus execution and complete ordinary delivery through victory | `commander_actions::tests::ai13_ground_emitted_ability_and_focus_orders_survive_ordinary_match_delivery_and_finish` |
| Entrench execution and complete ordinary delivery through victory | `commander_engineering::tests::ai13_emitted_holder_project_survives_canonical_queue_every_seat_delivery_and_finish` |

The focus lifecycle retains weapon cooldowns to isolate order state. It does
not establish live-flight delivery or unrestricted firing.

Existing tests in #391's code cover buy, support, build, assist, research, move,
attack-move, retreat, stance and cover. The final focused selection includes the
commander modules to detect regressions in those paths. This is not a complete
AI-13 emitted-command-union receipt. The internal `ground_flights` capability remains false for actual human packets.
Aimed grenade, satchel and barrage outputs and FireAt are tested only in
explicit detached-capability cases; a negative control prevents them from
entering unsupported ordinary delivery. Ground FireAt needs #337's canonical
actuator and ground flights; unrestricted aircraft missions need #369's aviation
flight and event delivery. Rail, boarding, unloading and recover are World-only
in `shared/ai.js:630-710` and `commander_shopping.rs`; World admission stays closed.

AI-64 prerequisite mapping:

- `match_human_observation::tests::privacy_matrix::ai64_conquest_hidden_perturbations_preserve_real_snapshots_and_human_policy_commands`: 216 paired cases across Conquest with houses and without houses, two seeds, two delivery ticks and all three levels. Real complete recipient snapshots must match before detached views and actual human policy commands are compared. Perturbations cover enemy wallets, hidden armies, added tanks, secret depots, visible enemies' private state, wire precision, hidden footprints, future weather and hidden mines.
- `ai64_visible_health_and_own_economy_change_real_human_commands`: visible negative controls.
- `ai64_own_motion_and_context_decode_the_recipient_rows`: own motion, queued attack-move, attack target and mines come from recipient rows.
- `ai64_recipient_packet_and_human_commands_survive_ordinary_ticks_for_every_seat`: ordinary lifecycle and all-seat delivery.

The matrix invokes the production human planner on the scripted look, as the
prototype row does. The camera and motor delay layer is outside this matrix.

Full AI-64 still needs actual Classic, Assault and Horde recipients and the
scripted Horde seat. #390 owns Classic snapshot admission; #357 explicitly leaves
hosted Horde/full recipient integration guarded. No mode is represented by a
renamed Conquest fixture or a fabricated complete snapshot. Logistics-enabled
human observations now refuse transactionally until #375 provides recipient
stocks. The old AI-28 authoritative-stock witness is replaced with that refusal
check, without claiming or changing AI-28's parity status.

Continuation must enable `ground_flights` from the qualified receiver capability
after #337/#369, then execute the aimed families through live flights and every
seat's delivery. For AI-64, derive defender/director roles from real mode rows
and extend the matrix to actual Classic, Assault and Horde matches, including
the scripted Horde seat.

## Validation

The PR body records the producing head and actual results. Required checks
are formatting, the focused core selection and strict workspace Clippy, under
the prescribed shared slot and matching target, Rust 1.99.0, 6 GiB memory and
zero swap. No Source recorder is used.
