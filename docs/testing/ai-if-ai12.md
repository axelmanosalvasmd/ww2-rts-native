# AI-12 assessment, 2026-10-16

AI-12 remains unproven and is dropped from this batch. The inspected native base is `b6d05874078f183fc95421cd25e13762b253c57c`, with this stream's comparator and server edits in progress. This assessment adds only this document, with zero Cargo executions and no test selectors for AI-12.

The reference is `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`. In `shared/ai.js:260-287`, each look clones its observation and mirrors spending on buy, support, build, tech and dig. It also mirrors ability targets/cooldowns/munitions/suppression, build and assist work, garrison entry, fire-at, dig, entrench and stance. `shared/ai-human.js:168` collects orders without returning a refusal, so every queued order takes that local accepted path. The holder branch at `shared/ai.js:738-739` then skips further work after an assumed accepted garrison, before authoritative execution can refuse it.

The native policy has partial local accounting, not that complete mirror:

- `rust/crates/core/src/commander_tactics.rs:470-478` subtracts research and forward-HQ spending and removes the chosen builder from later movement choices.
- `rust/crates/core/src/commander_tactics.rs:670-688` passes shopping's remaining MP into a separate support budget. `SupportBudget::call` at lines 1028-1051 subtracts one support cost and sets a per-look spent flag.
- `rust/crates/core/src/commander_tactics.rs:708,787,802` tracks units assigned to defense with a separate busy set.
- The holder branch at `rust/crates/core/src/commander_tactics.rs:830-854` emits only cover. No current runtime policy branch emits garrison, ability, fire-at, dig, entrench or assist. The quiet policy explicitly rejects a holder with an available house at `rust/crates/core/src/commander_ai.rs:547-561`.
- `rust/crates/core/src/commander_ai.rs:77-97` exposes boolean work flags but no ability cooldown or hold-fire/hold-position fields. `commander_tactics::UnitFacts` at lines 21-29 supplies auto-retreat but no remaining stance or ability state. Complete later-decision consumers for these mirrored effects are absent.

The delayed motor and refusal paths do not fill the policy gap. `rust/crates/core/src/human_commander_policy.rs:79-86` calls the actual tactics planner. `human_commander_state.rs:727,754-760` obtains its completed order list and only then turns it into delayed inputs. Adding a mirror after that callback would be too late for decisions in the look. `human_commander_inputs.rs:376-399` can retain a raw garrison request on a delayed key/click input, but `Command::from_json` in `match_commands.rs:383-689` has no garrison variant and falls through to `Command::Unsupported`. `match_commands.rs:841-842` returns `Denial::Unsupported`. A synthetic queued request could demonstrate that unsupported-command refusal, but would not demonstrate the actual planner seeing its own garrison as accepted.

Completing the row requires the missing policy branches and delivered local ability/stance state, a mirror used between actual decisions, and a planner-generated deferred refused-garrison witness. That exceeds this bounded batch. No unused mirror helper was added. Public R1 ingress remains restricted to move, attack-move, stop and stance by `rust/crates/server/src/commander_ingress.rs:17-33`; World admission, logistics and recipient guards were not changed.
