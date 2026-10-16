# AI intent batch 2

Base: `40fe9e296b46060241693c6c627ecd615d4f8ea4`. This branch starts from origin/main and imports no private runtime code or branch history. PR #274 owns AI-08/20/21/22/23/24/27 and is untouched.

The remaining assignment has 56 rows: one proven candidate, 51 dropped from this batch, and four receipt-provider rows in NEXT. No parity status changes are included. The native-intent owner decision is [6206861267](https://github.com/axelmanosalvasmd/ww2-rts-native/issues/5#issuecomment-6206861267).

AI-15 tests the shared server snapshot cadence. It does not claim the missing AI observe/act loop (AI-14), seat brain binding (AI-16/17), or a whole-match headless runner (AI-65). The prototype headless runner uses a fixed beat of two; the native opening component also retains its existing fixed-two delivery test, `match_commander::tests::actual_source_observations_and_vision_phase_match`.

Prototype source at `e4e579a838aabae3b1d9dadbc17a0a87147bc52d` was inspected in the initialized reference checkout under `t3-29229094`. The assigned worktree reference submodule is uninitialized. No prototype source was changed.

Validation on the final Rust tree: four focused nextest tests passed (65 skipped), `cargo fmt --all --check` passed, and strict locked workspace/all-target Clippy passed. The new behavior test, existing exact meter fixture, real socket cadence test and winner control were selected. Jobs used shared slots S2/S3 with a 6 GiB memory cap and no swap. Hosted CI and merge remain with the orchestrator.

| Row | Prototype at e4e579a | Batch disposition |
| --- | --- | --- |
| AI-01 | `shared/ai-view.js:103-195; shared/ai.js:256-257` | NEXT: receipt decode providers are absent from main. The retained implementation needs over 1,000 provider lines plus Match/vision integration, so it is deferred from this test PR. |
| AI-02 | `shared/ai-view.js:9-21,143` | NEXT: receipt decode providers are absent from main. The retained implementation needs over 1,000 provider lines plus Match/vision integration, so it is deferred from this test PR. |
| AI-03 | `shared/ai-view.js:23-57,164-177` | Dropped: active aircraft and complete private logistics producer coverage are missing. |
| AI-04 | `shared/ai-view.js:144,161-163` | NEXT: receipt decode providers are absent from main. The retained implementation needs over 1,000 provider lines plus Match/vision integration, so it is deferred from this test PR. |
| AI-05 | `shared/ai-view.js:74-137; shared/world-layers.js:16-24; shared/sim.js:2731-2740` | Dropped: complete World unknown terrain, delivered terrain history and the human takeover path are not connected on main. |
| AI-06 | `shared/ai-view.js:146-148` | NEXT: receipt decode providers are absent from main. The retained implementation needs over 1,000 provider lines plus Match/vision integration, so it is deferred from this test PR. |
| AI-07 | `shared/ai-view.js:149-158` | Dropped: public receipt retention is covered, but complete planner effect decoding/consumption is missing. |
| AI-09 | `shared/ai-view.js:179-192` | Dropped: known-building ghosts and resource-node claim policy are absent from the quiet planner. |
| AI-10 | `shared/ai-human.js:55-117; shared/ai.js:248-252` | Dropped: live Horde wave guard and full host event integration are missing. |
| AI-11 | `shared/ai.js:253; shared/ai-human.js:291-293` | Dropped: real canonical Move admission is tested, but the complete AI command family and operation caller are missing. |
| AI-12 | `shared/ai.js:260-287; shared/ai-human.js:168` | Dropped: broad local spending/flag mirror and refused-garrison planning branch are absent. |
| AI-13 | `shared/ai.js:41-1068` | Dropped: raw_command supports Move, Cover, Buy and Technology only; complete output union is absent. |
| AI-14 | `shared/ai-schedule.js:25-35` | Dropped: handover component exists, but real all-seat observe-loop then act-loop server integration is missing. |
| AI-15 | `server/room-tick.js:8-21,45-46; tickmeter.js:12-41` | Proven candidate: intent_ai15_room_snapshot_cadence_adapts_and_winner_overrides_the_beat. Real RoomRuntime ticks and recipient packets cover initial cadence 2, adaptation to 3 and 4, strict thresholds, sample gating, recovery interruption and floor, and winner override. Existing exact meter fixtures remain unchanged. |
| AI-16 | `shared/ai-schedule.js:17-23` | Dropped: live ordinary/Horde brain selection is not connected. |
| AI-17 | `shared/ai-schedule.js:12-16,29-31` | Dropped: pure stagger/lead window is tested; real Horde always-observe and Normal-only seat binding are missing. |
| AI-18 | `shared/ai.js:173-211` | Dropped: Horde movement FIFO, four-order flush, 240-order bound and wave reset are absent. |
| AI-19 | `shared/ai-human.js:55-78; server.js:448,525,601` | Dropped: HumanState has lazy initialization and reset helpers, but the server has no human take/leave caller connected to them. A direct helper test would repeat the incomplete witness from PR #274. |
| AI-28 | `shared/ai.js:291-308` | Dropped: logistics retreat/recovery policy is absent. |
| AI-29 | `shared/ai.js:350-391` | Dropped: defense/wait/push situation state and threat selection are absent. |
| AI-30 | `shared/ai.js:356-381` | Dropped: contact/paradrop notice memory is absent; damage-alert timing is a different feature. |
| AI-31 | `shared/ai.js:762-778` | Dropped: quiet scores exist, but wait threat avoidance and Assault/Horde 70-metre restrictions are absent. |
| AI-32 | `shared/ai-mind.js:21-94` | Dropped: loss respect, recapture lessons and adaptive purchase substitution are absent. |
| AI-33 | `shared/ai.js:392-438` | Dropped: Germany MG component exists; complete adaptive chain, faction swaps, fallback and engineer policy are absent. |
| AI-34 | `shared/ai.js:500-536` | Dropped: off-map support decision chain is absent. |
| AI-35 | `shared/ai.js:603-617` | Dropped: watched target ability selection and per-look cast caps are absent. |
| AI-36 | `shared/ai.js:671-681` | Dropped: strategic hurt/last-model/suppression retreat and reinforcement waiting are absent. |
| AI-37 | `shared/ai.js:343-346,491-492,846-847` | Dropped: fight-triggered and Horde auto-retreat arming policy is absent. |
| AI-38 | `shared/ai.js:673-681` | Dropped: busy-unit guards exist; medic centre movement and vehicle shelling chores are absent. |
| AI-40 | `shared/ai.js:790-809` | Dropped: enemy objectives are guarded; assault value/ratio/failure/smoke gates are absent. |
| AI-41 | `shared/ai.js:570-585` | Dropped: depot/point defense allocation is absent. |
| AI-42 | `shared/ai.js:311-316,586-589,753-761` | Dropped: mode target lists, weather army threshold and push gating are absent. |
| AI-43 | `shared/ai.js:591-602` | Dropped: aircraft planning is explicitly unsupported. |
| AI-44 | `shared/ai.js:810-845` | Dropped: Hard focus-fire selection and onward-order preservation are absent. |
| AI-45 | `shared/ai-mind.js:133-270; shared/ai.js:855-862` | Dropped: staged assault state machine, manual release and timing are absent. |
| AI-46 | `shared/ai.js:618-628,848-853` | Dropped: remembered armor dispatch and unsafe rifle-order filtering are absent. |
| AI-47 | `shared/ai.js:863-875` | Dropped: quiet Move/attack-move order exists; retreat/operation ordering and full decision record are absent. |
| AI-48 | `shared/ai.js:41-143` | Dropped: bridge rebuilding, fill and demine planning with 90-second dispatch memory are absent. |
| AI-49 | `shared/ai.js:718-748` | Dropped: immediate Cover component exists; paid mine/trench/wire/traps and house decisions are absent. |
| AI-50 | `shared/ai.js:902-958` | Dropped: HQ fortification stages and reserves are absent. |
| AI-51 | `shared/ai.js:966-979,331-332` | Dropped: bounded research component exists; full mode/Horde exclusion, savings and Armory progression coverage is incomplete. |
| AI-52 | `shared/ai.js:878-900` | Dropped: base construction/repair/site-search policy is absent. |
| AI-53 | `shared/ai.js:981-1004` | Dropped: forward-HQ placement and exclusions are absent. |
| AI-54 | `shared/ai.js:1006-1039,333,435` | Dropped: Classic depot/build order, richer-node choice, repairs and engineer buying are absent from the quiet native planner. |
| AI-55 | `shared/ai.js:321-330,542-589` | Dropped: Classic/World counters, rush defense, value-gated base attacks, depot raids and reconnaissance are absent. |
| AI-56 | `shared/ai.js:310-316,457,766,912,986` | Dropped: Assault/Annihilation target policy, defender radius and forward-HQ/fortification exclusions are absent. |
| AI-57 | `shared/ai.js:309,338-346,438,531,671-682,971` | Dropped: Horde-only planner, stance arming, queue and ability/support policies are absent. |
| AI-58 | `shared/ai.js:423-424,434,629-715,1041-1068` | Dropped: World observation admission and recover/region/frontier/rail/carrier planning are not connected. |
| AI-59 | `shared/ai.js:233-239; shared/ai-human.js:21-25` | Dropped: Existing exact timing fixture covers Easy commander values and draws. Full Easy support, assault and casts policy is absent; complete row excluded. |
| AI-60 | `shared/ai.js:233-239; shared/ai-human.js:21-36` | Dropped: Existing exact timing fixture covers Normal commander values and defaults. Full adaptive/support planner and actual Normal Horde seat are absent; complete row excluded. |
| AI-61 | `shared/ai.js:233-239,323-324,398,798; shared/ai-human.js:21-25` | Dropped: Existing exact timing fixture covers Hard commander values. Counters, sighting-based support, focus fire and held-point guards are absent; complete row excluded. |
| AI-62 | `server.js:521-527,640-643; server/room-tick.js:21` | Dropped: Detached handover timing is a component witness. Live server rejects addAi and level in the current supported room path; full seat-level selection and takeover are absent. |
| AI-63 | `shared/ai.js:157-165; server.js:425-448; server/saves.js:16-22` | Dropped: The earlier direct reset witness does not cover the human take/leave caller. Durable planner memory save/restore, human-seat clearing and clone-failure policy are absent. |
| AI-64 | `test.js:4587-4733` | Dropped: Existing real hidden-purchase and terrain/privacy controls cover quiet Conquest components. Full all-level Classic/Assault/Horde perturbation matrix is absent. |
| AI-65 | `shared/ai-schedule.js:37-72; test-ai-human.js:13-28` | Dropped: Existing quiet policy replay test is a component witness. Complete seeded whole-match human commander and full mode runtime are absent. |
| AI-66 | `shared/ai.js:146-155,1034; shared/ai-human.js:40-61` | Dropped: Exact private commander timing stream is implemented. Planner uses the documented Ai purpose stream; complete Classic draw-count branch is absent. No new intentional difference is approved by this report. |
