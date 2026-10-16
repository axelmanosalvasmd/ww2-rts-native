# Classic planner and commander randomness

Keep the native planner's existing per-seat `Ai(slot)` random stream and the human commander's private timing stream. The prototype's global planner RNG is not restored. Internal independent streams remain the approved native design under PARITY.md's randomness rule.

Ordinary Classic non-depot construction now consumes one planner fraction for every eligible engineer before site visibility and wallet checks, matching the intent of the eagerly evaluated flak angle at prototype `shared/ai.js:1034`. Fixed-angle buildings discard that draw. Repairs, depot planning, busy or retreating engineers and an absent next building consume none. Proposals use delivered terrain and the existing site search, then enter canonical command admission through normal delayed human input. No runtime state or command kind is added. World keeps its existing detached reserve-only construction provider.

The human commander's existing seat-mixed mulberry32 initialization and two-draw Box-Muller samples are unchanged. Focused tests cover both planner destination branches and actual ordinary Conquest and Classic seat lifecycles, with recipient snapshots for every seat. Execution results belong in the AI-66 testing note after the coordinated run.
