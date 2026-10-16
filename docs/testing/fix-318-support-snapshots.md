# PR 318 support snapshot repair

The AI planner now reads `recipient_snapshot::SUPPORTED_SUPPORT_KINDS`, the same list used by snapshot capture. The constant and export match PR 296. Dive, Recon, Bombing, Strafe and Para stay unavailable to this planner until the adapter represents them. Canonical support admission is unchanged.

`commander_support_runtime::tests::preferred_dive_falls_back_or_waits_without_tick_or_recipient_faults` delivers a watched tank with enough money and zero Dive cooldown. It checks artillery fallback and no-call behavior, then advances eight ordinary ticks and captures every seat after each tick. Planner tests cover all five refused kinds, artillery fallback, supported cover, notice history, wallet reserves, cooldowns and the Easy call gap. Tests that expected unsupported air support were replaced; the price and reserve test now uses artillery.

Row accounting after this repair:

- AI-34: partial. Supported cover and artillery, targeting, reserves, cooldowns and accepted-call clocks remain covered. The complete row is held because Dive, Para, Bombing, Strafe and Recon are closed.
- AI-35: `commander_abilities::tests::each_ability_uses_its_combat_trigger`; `commander_ability_policy::tests::abilities_wait_for_observed_notice_then_use_canonical_admission`.
- AI-42: `commander_attack_policy::tests::intent_ai42_mode_targets_only_visible_bunkers_and_remembered_production`; `commander_attack_policy::tests::intent_ai42_weather_count_push_strength_and_hands_are_independent_gates`. Broader modes remain detached planner proof, with Conquest admission only.
- AI-44: `commander_attack_policy::tests::intent_ai44_hard_combines_fire_finishes_hurt_target_and_requeues_onward_orders`; `commander_focus_runtime::tests::focus_notice_then_attack_and_queued_onward_use_actual_command_admission`.
- AI-47: `commander_look::tests::intent_ai47_emission_order_and_complete_decision_survive_the_next_look`; `commander_look::tests::intent_ai47_claims_and_unexpired_manual_releases_filter_ground_orders_only`.

AI-33 and AI-38 retain the original PR's dropped status. No FFI, C header, wire, contract or privacy change.

Validation on the repaired Rust tree after merging main `7ed527b04b0244d9affaec4fc42d88612ccf3c98`: focused nextest ran 74 tests, all passed (594 skipped). The filter included commander modules, human commander policy, Match commander integration and snapshot support tests. Formatting and strict workspace all-target Clippy passed. Checks used Vostro slot S1, four Cargo jobs, a 6 GiB memory scope and no swap.
