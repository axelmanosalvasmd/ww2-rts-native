# Tutorial runtime behavior checks

This PR starts from main `e6d0e08d` and implements Tutorial in the Rust core. The prototype is pinned at `e4e579a`. Its Tutorial code was read directly, along with the earlier private runtime at `c5eb12f4`; no private branch history or recorder was imported.

The constructor adds the Enemy slot and connects the existing step definitions to canonical Match systems. The runner emits dialogue from tick 40, advances at most one step per tick and finishes through the common Match result. These are core behavior checks; they do not prove a server entry flow or Unreal presentation.

All test names below are in `ww2_core::match_tutorial::tests`.

| Row | Test | Prototype read |
| --- | --- | --- |
| MODE-59 | `mode59_tutorial_income_uses_attacker_base_points_and_army_scale` | `shared/sim.js:5731-5753` |
| MODE-60 | `mode60_tutorial_runner_delays_dialogue_progresses_all_steps_and_wins` | `shared/tutorial.js:71-117`, `test-tutorial.js` |
| MODE-61 | `mode61_tutorial_hedgerow_waits_for_all_allied_squads` | `shared/tutorial.js:10-15` |
| MODE-62 | `mode62_tutorial_orchard_spawns_understrength_sentries_and_waits_for_both` | `shared/tutorial.js:16-22`, `:84-99` |
| MODE-63 | `mode63_tutorial_crossroads_uses_real_point_capture` | `shared/tutorial.js:23-28` |
| MODE-64 | `mode64_tutorial_recruit_applies_wallet_floor_and_waits_for_at_gun` | `shared/tutorial.js:29-34`, `:83` |
| MODE-65 | `mode65_tutorial_tank_executes_attack_move_and_death_advances` | `shared/tutorial.js:35-41`, `:95` |
| MODE-66 | `mode66_tutorial_farm_squads_occupy_trench_and_both_must_die` | `shared/tutorial.js:42-48` |
| MODE-67 | `mode67_tutorial_retreat_completes_from_accepted_order` | `shared/tutorial.js:49-54` |
| MODE-68 | `mode68_tutorial_house_garrisons_and_artillery_cooldown_advances` | `shared/tutorial.js:55-61`, `:96` |
| MODE-69 | `mode69_tutorial_bridgehead_capture_finishes_at_second_point` | `shared/tutorial.js:62-67`, `:114` |
| MODE-70 | `mode70_tutorial_authored_map_and_retained_objectives` | `maps/tutorial.json` |

The MODE-60 journey uses accepted movement, recruitment, retreat and support commands, actual objective capture and actual house entry. It resolves fights by setting enemy HP to zero, as `test-tutorial.js` does, and removes pending shelling before the bridge crossing. It does not establish normal combat victory or artillery visibility admission. Focused step tests enter their step through the runtime's private begin method, then exercise its effects and completion through canonical ticks.

Dropped rows:

- MODE-57: server Tutorial auto-start and lobby map selection are outside this core runtime PR.
- MODE-58: roster, shared spawn, disabled features and instant ground recruitment have a focused test. Aircraft purchases remain unsupported, so the complete row is not claimed.

Mixed-mode rows remain with the other MODE stream. No parity-table cells change. Runtime acknowledgment, independent review, CI and merging belong to the orchestrator.

Validation on the final Rust content: formatting passed; all 29 selected library tests passed (14 Tutorial checks and 15 existing recruitment, ground-order and construction checks); strict workspace all-target Clippy passed with warnings denied. Cargo ran under the shared S1/S2/S3 lock and 6 GiB, zero-swap scope. No full workspace test run or Source recorder ran. Local logs: `/tmp/mode-tutorial-fmt.log`, `/tmp/mode-tutorial-regression.log` and `/tmp/mode-tutorial-clippy.log`. The first failed run is retained in `/tmp/mode-tutorial-tests-first.log`; it exposed only the retired-ID test-helper error corrected before final qualification.
