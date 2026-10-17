# Save chain integration, 2026-10-17

Base: `61947c5b8a8438eb6ccbefa02e7d269c4e032472`. Branch: `integrate/save-20261017`.

Ordinary saves remain v1 and authored saves remain v2 under `docs/contracts/save-format.md`. World admission remains refused. The branch preserves each PR head with ordinary merge commits; no PR is opened.

## Manual resolutions

- #376: retain main's protocol approval text and append the editor contract link. Retain main's Classic partial-delivery checks and evidence. Compose editor routes with static-file fallbacks, editor state with worker fronts and test rigs, and editor configuration with static-root and room-capacity settings. Load the selected edited or embedded map before deriving its logistics option.
- #383: accept equivalent formatted routing and map-loading composition. Keep one copy of each static/editor configuration field and worker initializer.
- #386: retain full scenario condition/action/arrival execution with strict checkpoint field decoding, legacy-state refusal and the required execution clock. Retain canonical timed-state validation where that evaluator supports the definition; use the full engine's contextual consistency checks for other definitions. Keep authored and legacy execution mutually exclusive. Update the earlier group/reinforcement refusal tests to the engine behavior; preserve their other admission assertions. Keep the v1/v2 format contract.

## Rewritten tests

- `authored_scenarios_validate_ordinary_sides_and_localization_before_startup`: groups and reinforcement are admitted by the full engine; invalid sides, localization and World targets remain refused.
- `map15_map_loading_uses_the_existing_complete_scenario_validator`: the validated authored group now publishes and survives ordinary ticks and both recipient snapshots instead of being refused by the timed subset.

## Verification

Pending integration checkpoints and final-head checks.
