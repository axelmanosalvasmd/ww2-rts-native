# AI cut-point reconnection preparation

## Frozen plan

Owner: IMPL_AI. Baseline: `34edf52a7eaa6cb6edba3e802c2190226228276f`.
Source: `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`, read from
`/home/judiazm/Projects/ww2-rts-native/reference/ww2-rts`.
Ownership was checked in the Root handoff before editing. The seven selected
rows belong to IMPL_AI and none is ported. All statuses remain unchanged.

Readback: issue #5 decisions 085, 090 and 097 and readback 129 and Root restart 6185610295 are acknowledged
locally. No GitHub reply is authorized. R1 permits exactly two human commanders,
so this work does not activate AI seats in a room. Whole-match evidence criteria,
privacy, spectator behavior and new gameplay decisions remain with the owners.
No external interface is needed or changed.

Source inspection found two bounded integration gaps: the quiet planner requires
an unowned point even when an allied cut point needs reopening, and the human
policy serializer refuses the planner's existing attack-move variant. Extend the
quiet predicate to an allied cut objective for rifle-only rosters without research,
and preserve its 180-second limit and all health, enemy, mode and economic
restrictions. Support-role rosters can trigger the unported staged assault when
no neutral point remains, so they stay outside this extension. Serialize that existing plain
attack-move into its original `t/orders` shape. Do not add a new command kind.

The recorder must execute the complete original test.js check named
"Supply reconnection remains immediate at every level, including Easy's opening grace period".
Extract its exact body and original aiCommands helpers, retain every assertion,
and use its actual createGame, clearFixtureUnits, spawnUnit and viewFor calls.
The test's own owner/cut view overlay is an explicit input, not a native patch.
Capture full delivered view, before/after planner memory, complete raw commands,
ordered actual RNG fractions and original duplicate/formation/input helper outputs.
Do not insert an unowned sentinel, replace the thinker or create expected commands
in the recorder. Keep startup RNG separate from the original seed-37 look RNG.

## Selected row contracts at the baseline

The following rows are copied unchanged as the acceptance contract. Their broad
checks remain required even when the reconnection component passes.

| ID | Behavior / content | Evidence | Rust (Julio) | Unreal (Axel) | Depends on | Acceptance check | Status |
|---|---|---|---|---|---|---|---|
| AI-11 | Every AI order goes through the same `command(g, slot, cmd)` as a human message, with the same costs, cooldowns, limits and denials. The server applies the human path at `server.js:713`; the commander calls `command` from its input queue. | `shared/ai.js:253`, `shared/ai-human.js:291-293`, `server.js:712-714` | port: one command validator for both | none | NET, CMB, BASE | `test.js` "Replay an AI seat's actual commands as a human"; `test-engine-ai.js` "operation submits accepted normal commands" | in progress |
| AI-25 | Order filtering: drop moves to spots a unit already heads for (3 m), attacks on the current target, retreats of retreating units. Plain moves or attack-moves whose targets lie within 15 m and whose units fit one screen merge into one order. | `shared/ai-human.js:209-241` | port: `news`, `formationMoves` | none | none | AI-65 input log | todo |
| AI-26 | Inputs per order: camera move when units or target are off screen (move and attack-move may click the minimap instead), a select when the selection changes, a key for keyed orders, a click unless the order has none. The command runs on the order's last input. | `shared/ai-human.js:30-33`, `shared/ai-human.js:252-269` | port: `inputsFor` | none | none | `test-ai-human.js` "placed on its screen" | todo |
| AI-31 | Point choice: score = distance from the scout (or the unit) + 40 × units already headed there − 25 × (VP − 1) − 10 × MP + 40 × failures (max 3) − 18 if it is the current aim. Skip own uncut points, locked points, and (Assault and Horde defenders) points over 70 m from spawn. A `wait` picks a "safe" point over 28 m from the threat. | `shared/ai.js:451-483`, `shared/ai.js:762-778` | port: same | none | MODE | `test.js` "Losses stay in the commander's head after the killer leaves sight, and a point that ate squads is avoided" | in progress |
| AI-39 | Points: infantry stays on a point it is capturing; the first infantry on each held point holds it (AI-49); free units head for their chosen point; neutral and own points take any free unit (capped by `hands`); an own cut point is reopened at once with attack-moves, not capped. | `shared/ai.js:716-750`, `shared/ai.js:782-789` | port: same | none | LOG-45 | `test.js` "Supply reconnection remains immediate at every level, including Easy's opening grace period" | in progress |
| AI-47 | End of look: order emission is retreat, then move, then attack-move, then operation commands; the operation and a decision record (situation, action, buy, aim) are stored. | `shared/ai.js:863-875` | port: same emission order | none | none | AI-65 command log | todo |
| AI-66 | Randomness: the planner draws from global `Math.random` (`spotNear` cover pick or angle; Classic site angle, drawn once per engineer per look whenever the next building is not a depot, even when it is not a flak position). The commander uses its own mulberry32 stream per seat seeded from `g.seed ^ imul(slot + 1, 0x9E3779B1)` at first use; each sample uses 2 draws (Box-Muller). | `shared/ai.js:146-155`, `shared/ai.js:1034`, `shared/ai-human.js:40-53`, `shared/ai-human.js:61` | port: same streams and draw counts | none | IF | covered by AI-65; `test.js` `aiCommands` counts draws | in progress |

## Predicate map and remaining consumers

| Row | Prepared predicate | Full acceptance still required |
| --- | --- | --- |
| AI-11 | The planner's plain attack-move survives raw serialization and input construction without changing its unit IDs or goals. | Original AI-versus-human replay in Conquest, Classic and Assault with all payments, steps, queues, limits and denials; engine-AI operation commands. No canonical actuator or room admission proof is added here. |
| AI-25 | Reconnection commands pass original news filtering and formation merging with complete command arrays and ordered IDs. | Redundant moves, attacks and retreats and all merging cases in the whole input-log acceptance. |
| AI-26 | Original inputsFor and native inputs_for emit identical camera, select, key and click sequences, final cursor and terminal raw command. | Full human-level screen placement and all other command families through actual scheduling/admission. |
| AI-31 | Original quiet point choice and aim memory match with a single allied cut point and no neutral sentinel. | The complete loss/respect/point-failure memory check and enemy/wait/Assault branches. |
| AI-39 | All three original level assertions reconnect both real spawned rifles by attack-move at tick zero, despite no neutral point. Compare full planner output, aim and draw consumption. | Capturing/holding, physical reconnection and the remaining original row behavior. The supplied cut flag is the original test overlay; this is not a supply-network producer proof. |
| AI-47 | Full original raw command sequence is retained through the adapter, including the cut-point attack-move. | Retreat and operation command ordering and complete decision/operation state and whole input logs. |
| AI-66 | Native component consumes exactly the recorded original seed-37 waypoint fractions. Production uses its existing per-seat MatchRandom stream. | Complete planner and commander draw schedules in AI-65. Component fraction replay is not global-stream or full-match equivalence. |

Existing consumers: commander_ai produces the typed command; HumanPolicy invokes
it through HumanState; raw_command feeds original-shaped values to the existing
human input helpers. Source search finds no room-runtime or Match caller of
HumanPolicy. Missing consumers remain host AI scheduling, all-seat delivered
observation ownership, issuer allocation and canonical physical admission.
The existing R1 room cannot host AI without a new owner decision.

## Planned verification and delivery

No Source recorder, Cargo, rustc, native binary or formatter execution is
authorized until Root grants a slot. Prepare the recorder and native comparison
first. The new test loads the recorder output at runtime, so source preparation
can compile without a fabricated JSON fixture. It deliberately fails if that
real fixture has not been generated; do not ignore it or substitute values.

After a grant, run serially from this worktree:

```sh
WW2_REFERENCE=/home/judiazm/Projects/ww2-rts-native/reference/ww2-rts /tmp/cmb-s1-5fcd7159/toolchain/node-v24.21.0-linux-x64/bin/node harness/ai-cut-reconnection-fixture.mjs
WW2_REFERENCE=/home/judiazm/Projects/ww2-rts-native/reference/ww2-rts /tmp/cmb-s1-5fcd7159/toolchain/node-v24.21.0-linux-x64/bin/node harness/ai-cut-reconnection-fixture.mjs --check
cargo +1.99.0 test --manifest-path rust/Cargo.toml --locked --profile ci-test -p ww2-core --lib human_commander_policy::tests::original_cut_reconnection_check_and_human_inputs_match -j4 -- --exact --nocapture --test-threads=1
cargo +1.99.0 test --manifest-path rust/Cargo.toml --locked --profile ci-test -p ww2-core --lib human_commander_policy::tests -j4 -- --nocapture --test-threads=1
cargo +1.99.0 test --manifest-path rust/Cargo.toml --locked --profile ci-test -p ww2-core --test it commander_ai:: -j4 -- --nocapture --test-threads=1
cargo +1.99.0 fmt --manifest-path rust/Cargo.toml --all -- --check
```

The discovered Node candidate is in the active CMB toolchain directory. Root must
confirm it remains available or substitute another exact x64 Node 24.21.0 path;
the recorder checks the actual version before importing Source. `/usr/bin/node24.21.0`
does not exist here. No Node executable was run during preparation.

Use Root's CPU, memory, process-scope and target-directory lease for every granted
job. Stop on the first real failure and retain the command/log. Freeze a successor
SHA containing the actual fixture only after source generation succeeds. A
separate reviewer and Root verification are required before any push or PR.
Before a future shared parity-file change or PR, integrate current main and
preserve both owners' row edits. This preparation does not change parity files.
