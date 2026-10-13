# Conquest abilities and recovery

This internal provider executes bounded ground ability and recovery phases on canonical Match state. The complete host tick and engineering dispatcher remain separate integrations. It accepts represented Conquest state; Classic/World and generic mode enablement are excluded.

`Match.abilities` owns cooldown (source `cd`), buff, AP and automatic/walked aimed flags. `Match.commands.units[id].nade` owns the aimed point. `Match.recovery` owns previous HP, wound/heal ticks and reinforcement progress. Game owns HP, suppression, sprint, positions and routes; logistics owns provisions; firing owns physical flights/salvos/grenade tickets and Combat random draws. Startup and the shared recruitment publication callback initialize rows. There is no second geometry or mutable health store.

The caller uses these phases in source order:

1. `advance_ability_timers(id, dt)` decays only ability cooldown and buff beside Game/firing timers.
2. `apply_autocast(id)` performs the staggered source judgment before work. `Command::Ability` and `Command::Autocast` use the existing recorded command queue.
3. `step_aimed_ability(id)` uses the existing physical launcher or salvo store. It rechecks ammunition, applies cooldown only after launch, and uses the existing Nade path request budget when walking.
4. After weather/fire, `prepare_recovery(captured_ids)` captures HQ/depot/aid access. `heal_and_sweep(&frame, dt)` runs cadence-ten healing and mine detection; healing uses the source `dt * 10` scale.
5. The caller runs point supply and network refresh.
6. `reinforce(&frame, dt)` uses the earlier membership and the real cache-first provision hook, then records accepted spending. This excludes later births from the per-unit pass and preserves a depot included before its cut status changed.

`repair_building_sections(id, amount)` repairs canonical current or historical section values in authored order, checks support and heavy occupancy before restoration, updates float32 cell HP, logs and building HP, and preserves failed history. Missing authoritative maximum HP refuses before writes. A route error after earlier committed healing stops the runner; the provider does not promise complete phase rollback.

`Match.field_works` is the sole paid Dig/pit owner. `cancel_field_work(id)` verifies live identity/activity and removes the real descriptor before clearing Game.dig, without refund. Prepared transactions can use the same crate-private `cancel_field_work_state` on a final staged Unit. Ready manual aimed replacement uses this cancellation; queued/automatic/refused use retains work. Build-target and mass-entrench project metadata remain unsupported. Invalid aim retains its Blocked precedence. Retirement assembly must prevalidate all potential captured removals and clean these sidecars at each actual pre-drop boundary, including garrisons killed by earlier collapse, while retaining Combat metadata needed by flights. That hook is a separate integration patch.

The unchanged reference recorders check pin `e4e579a838aabae3b1d9dadbc17a0a87147bc52d` and clean tracked source under x64 Node 24.21.0. Ability has 36 traces, 157 actions and 193 snapshots; native replay compares 33 supported Conquest cases (147 actions), two Classic refusals and one unsupported-project boundary. Recovery compares all 23 traces, 133 actions and 156 snapshots. Canonical visibility membership is sorted, while actor/grid insertion order retains source tie behavior.

Exact compiled code/test head: `1f9a16b9299ad1bfcb99d2f530536c1bc69b765d`. After clearing all four workspace packages in both profiles, the provider passed 13 core debug checks, three ordinary-firing and two recruitment debug checks, all 177 locked release workspace tests, strict workspace/all-target Clippy, formatting and release FFI. Independent source review and copied debug/release executables passed all four ability and two recovery checks in each profile. The proof log is `/private/tmp/ww2-abilities-final-proof.log`. The managed local wrapper was `/private/tmp/ww2-native-cargo`; another agent must obtain the cache lease before using it.

Source regeneration uses `WW2_REFERENCE` with `harness/match-ability-fixture.mjs --check` and `harness/match-recovery-fixture.mjs --check`. Focused native filters are `match_abilities::tests` and `match_recovery::tests` in ww2-core. Release workspace, strict Clippy, formatting and FFI commands follow the normal workspace commands through the assigned wrapper.

Issue #138 tracks this component. The stacked PR depends on typed flights #133, logistics #130 and FieldWorks #118. Later evidence-only commits preserve the Rust/harness trees, while final CI must still run on the published head. Full tick and engineering assembly, other modes, transport, ABI and Unreal verification are not established by these checks.
