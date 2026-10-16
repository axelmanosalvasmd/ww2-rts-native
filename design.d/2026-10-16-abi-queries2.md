## Authoring fairness on the human commander schedule (2026-10-16)

The authoring C runner uses the existing SeatCommander and HumanPolicy rather than the quiet OpeningCommander. Every seat observes at creation; each tick refreshes all requested views before any brain acts. Human timing, camera attention and private tactical memory remain with their existing owners. Attempts enter the ordinary command queue for the next authoritative tick. Damage events retain their original ordinals until the delivery beat clears them and advances the generation. The fallible two-pass loop is local to this cold query because the generic Schedule callbacks are infallible.

Supply and living weather default to enabled, matching pinned createGame. Horde previews without defender spawns or with six players use the prototype's Conquest fallback, including disabled logistics. Detached preview JSON exposes Classic node positions/fuel and Horde gate/break metadata. No C function or layout is added or changed, so ABI remains 2.1. The caller can aggregate actual winning starts exactly as the fairness worker does.

World authoring runtime remains deferred. Complete non-World authored scenarios and all-mode fairness still depend on their canonical runtime and recipient owners. Bounded ordinary proofs do not establish these remaining clauses.
