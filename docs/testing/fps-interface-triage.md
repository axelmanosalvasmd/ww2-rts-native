# FPS and interface ownership triage

Baseline: native `40fe9e296b46060241693c6c627ecd615d4f8ea4`, prototype `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`. Paths below are relative to `reference/ww2-rts`. This covers all 40 FPS rows and the 28 todo/in-progress IF rows. IF-15 and IF-18 are already ported; IF-17 is already differs and is outside this batch.

RUST means core/server behavior; CLIENT means client input or presentation; BOTH means Rust supplies rules/data and the client consumes or presents them. Classification considers the behavior and the Unreal column. CLIENT is a proposed ownership routing, not owner approval to move a parity row.

Axel's `axel-parity-native-intent-yes-20261016` decision on issue #5 approves native behavior tests plus independent prototype-intent review. Exact comparison remains for tables and pure functions. This test PR changes no parity status, runtime, wire, ABI, contracts or privacy rules. Helpers that lack their room integration do not prove room behavior.

| Row | Class | Reason and prototype source |
| --- | --- | --- |
| FPS-01 | BOTH | Server validates operative admission and refuses invalid joins; client shows refusal (`server.js:604-607`). |
| FPS-02 | RUST | Room membership, presence, expiry and host eligibility are server rules (`server.js:205-206,607,723`). |
| FPS-03 | BOTH | Server sends operative roster/role fields for the client lobby (`server.js:342`; `client/main.js:370-373`). |
| FPS-04 | CLIENT | Lobby picker, direct URL, separate token identity and new-tab entry are client input/navigation (`client/main.js:137-151,370-373`). |
| FPS-05 | BOTH | Server resets and sends operative start state; client enters first person (`server.js:307-319,411,614`). |
| FPS-06 | RUST | Match creates one free controlled rifleman, preserves identity and rejects ineligible owners (`shared/sim.js:2008-2017,2172-2179`). |
| FPS-07 | BOTH | Server enforces operative permissions and client gates outgoing actions (`server.js:621-637`; `client/main.js:173-177`). |
| FPS-08 | RUST | Command ownership excludes the operative soldier except destroy (`shared/sim.js:3419,3763`). |
| FPS-09 | BOTH | Core validates sequences/numbers/booleans; client supplies bounded frames (`shared/sim.js:2021-2037`). |
| FPS-10 | BOTH | Server throttles inputs and gates pause/play; client has its own send cadence (`server.js:631-633`; `client/operative-view.js:128-133`). |
| FPS-11 | BOTH | Core detects counted edges and reconnect baselines; client maintains counts (`shared/sim.js:2018-2037`; `server.js:312`). |
| FPS-12 | RUST | Core expires stale/offline input and stops movement/fire (`shared/sim.js:2110-2112,2137`). |
| FPS-13 | BOTH | Core integrates stance/jump movement and client predicts it (`shared/sim.js:2116-2130`; `client/operative-controls.js:15-29`). |
| FPS-14 | BOTH | Core enforces map/body collision; client reconciles wall corrections (`shared/sim.js:2107-2108,2131-2136`). |
| FPS-15 | BOTH | Weapon definitions drive authoritative fire and client loadout/HUD (`shared/sim.js:2044-2047`). |
| FPS-16 | BOTH | Core gates firing, ammo and counters; client predicts shot timing (`shared/sim.js:2049-2051,2146`). |
| FPS-17 | BOTH | Core computes/rolls spread and client draws its crosshair from the formula (`shared/sim.js:2053-2056`; `client/operative-view.js:375-378`). |
| FPS-18 | RUST | Core resolves the aimed sampled ray and obstruction without a client target id (`shared/sim.js:2052,2075-2087`). |
| FPS-19 | RUST | Core filters visible targets and computes operative, squad and heavy body geometry (`shared/sim.js:2048,2061-2074`). |
| FPS-20 | RUST | Core applies friendly blocking, enemy damage, source metadata and counters (`shared/sim.js:2088-2093`). |
| FPS-21 | BOTH | Core publishes shot/man identity for client casualty/effect rendering (`shared/sim.js:2094-2096`; `client/main.js:931-958`). |
| FPS-22 | BOTH | Core owns reload/swap rules and client predicts their timers (`shared/sim.js:2138-2142`). |
| FPS-23 | BOTH | Core launches and resolves grenade damage; client animates and sounds the throw (`shared/sim.js:2098-2106,2145`). |
| FPS-24 | BOTH | Core selects/leads/releases squads; client shows the squad and release hint (`shared/sim.js:2148-2170,3419`). |
| FPS-25 | BOTH | Core retires the soldier and schedules respawn; client shows death/countdown (`shared/sim.js:5672-5678`). |
| FPS-26 | BOTH | Core chooses squad/HQ respawn and pays its cost; client explains the source/wait (`shared/sim.js:5311-5316,2171-2177`). |
| FPS-27 | RUST | Tick dispatch and recovery exclude operative soldiers from autonomous squad behavior (`shared/sim.js:5344,5659`). |
| FPS-28 | BOTH | Server projects the commander's view into an independent operative stream; client applies it (`server.js:285-293,320-329,507`; `server/room-tick.js:28,41`). |
| FPS-29 | BOTH | Server supplies rounded status/loadout/hit direction used by client prediction and HUD (`server.js:294-306`). |
| FPS-30 | BOTH | Server disconnect/reconnect preserves soldier identity and resets sequence; client reconnects (`server.js:596-603,718-725,312`). |
| FPS-31 | CLIENT | Pointer lock, bindings, input cadence and shortcut swallowing are client input (`client/operative-view.js:67-133`; `client/operative-controls.js:10-13`). |
| FPS-32 | CLIENT | Local prediction history, latency reconciliation and blending are client behavior (`client/operative-controls.js:14-29`; `client/operative-view.js:354-367,421-433`). |
| FPS-33 | CLIENT | Local ammo, cooldown, reload and guessed tracers are presentation/prediction (`client/operative-view.js:368-410,434-437`). |
| FPS-34 | CLIENT | HUD, damage indicators, capture control and leave link present supplied state (`client/operative-view.js:22-31,333-353,508-537`). |
| FPS-35 | CLIENT | Camera, recoil, FOV and hidden RTS overlays are client presentation (`client/operative-view.js:320-326,438-455,501-505`). |
| FPS-36 | CLIENT | Weapon art, animations, tracers, casings, sky and sound belong to rendering/audio (`client/operative-view.js:38-65,135-317,457-499`). |
| FPS-37 | CLIENT | Generic effects suppress shots already rendered/sounded locally (`client/operative-view.js:538-539`; `client/main.js:1003`). |
| FPS-38 | CLIENT | Client removes the named drawn man and places the operative model (`client/main.js:931-958`). |
| FPS-39 | BOTH | Room retains operative role across matches; client returns to lobby then first person (`server.js:411`; `test-operative-server.mjs:41-42`). |
| FPS-40 | BOTH | Server resync/rename updates the operative stream/lobby; client performs the held self-check (`server.js:634-635`). |
| IF-01 | BOTH | Rust exports tables/constants for client rule consumption (`shared/sim.js:26-28,372-473`; `shared/squad-men.js:8-26`). |
| IF-02 | BOTH | Rust supplies snapshot helpers and client decodes unit/fog/cell state (`shared/sim.js:5878-5887,5951`; `client/fog.js:29-30,72`). |
| IF-03 | BOTH | Rust supplies availability/prices/population for client HUD enable state (`shared/sim.js:378-394,520-549`; `client/availability.js:46-164`). |
| IF-04 | BOTH | Rust placement queries use a client-built recipient view for preview (`shared/sim.js:3261-3305`; `client/availability.js:187-198`). |
| IF-05 | BOTH | Rust provides plans/prices while client chooses digger/facing and draws ghosts (`shared/sim.js:477-512`; `client/main.js:1204-1215,1328`). |
| IF-06 | BOTH | Rust computes formation spots; client supplies selection positions and final order mapping (`shared/formation.js:29-53`; `client/main.js:1411-1441`). |
| IF-07 | BOTH | Directional cover provider/mirror choice feeds client cover rings (`shared/sim.js:2262-2339`; `client/cover-preview.js:25-53`). |
| IF-08 | BOTH | Rust map/weather helpers feed terrain, house, rail and border rendering (`shared/world-layers.js:16,53`; `shared/weather.js:37-122`). |
| IF-09 | BOTH | Shared slot/count/hit geometry determines client drawn men (`shared/squad-men.js:8-34`; `client/unit-models.js:484-497`). |
| IF-10 | BOTH | Shared movement prediction provider feeds client reconciliation (`client/operative-controls.js:15-29`; `client/operative-view.js:425`). |
| IF-11 | BOTH | Vehicle/profile/projectile helpers feed client pose/flight rendering (`shared/vehicle-motion.js:15-17`; `shared/projectiles.js:22-26`). |
| IF-12 | BOTH | Optional authoring core calls support an Unreal editor if adopted (`shared/sim.js:574-628,682,3068`; `client/editor.js:60-168`). |
| IF-13 | CLIENT | Command mapping, local preferences and recipient selection are client input rules (`client/orders.js:9-66`; `client/autocast.js:24-30`). |
| IF-14 | BOTH | Recipient-only query inputs and client linkage jointly prevent hidden-state access (`client/availability.js:187-198`; `shared/world-layers.js:68-80`). |
| IF-16 | BOTH | Rust math functions serve shared rules and client-facing queries (`shared/formation.js:30`; `shared/sim.js:478-509`). |
| IF-19 | RUST | Numeric conversions, signed zero and remainder semantics govern deterministic rules (`shared/sim.js:2345,869-873`; `shared/grid.js`). |
| IF-20 | RUST | Recorded per-purpose match RNG replaces the global-stream prototype accident under PARITY.md (`shared/ai-schedule.js:37-45`; `shared/ai.js:1034`). |
| IF-21 | RUST | Match construction/restarts must use recorded deterministic seeds (`shared/ai-schedule.js:52-72`; `server.js:381`). |
| IF-22 | RUST | Collection and live unit iteration order are core scheduling rules (`shared/sim.js:5342-5352,4826-4832`). |
| IF-23 | RUST | Grid encounter order and A* heap/storage affect authoritative routes (`shared/grid.js:12-129`; `shared/sim.js:3037-3064,3160-3161`). |
| IF-24 | RUST | Typed storage rounds authoritative cell/path state after f64 decisions (`shared/sim.js:4772,4783,5039-5076`). |
| IF-25 | BOTH | Stable comparators govern core choices and client formation queries (`shared/formation.js:35-40`; `shared/supply-convoys.js:92`). |
| IF-26 | RUST | Float-derived grouping and JSON equality govern cache/command identity (`shared/sim.js:1906,3216`; `shared/ai.js:187`). |
| IF-27 | RUST | Save/load must account for module-owned state and cache reset policy (`server/saves.js:18-27`; `server.js:447-448`). |
| IF-28 | BOTH | Server logs tick/order inputs while client sends gameplay actions (`server.js:631-633,713,784,791-800`). |
| IF-29 | RUST | AI observation cadence is an explicit deterministic scheduling input (`server/room-tick.js:14,21`; `shared/ai-schedule.js:28-33`). |
| IF-30 | RUST | Whole-match diagnostics serialize/hash authoritative state (`tools/bench.mjs:107-114`). |
| IF-31 | RUST | World generation needs deterministic streams, math and cost storage (`shared/world-conquest.js:24-38`; `shared/world-landforms.js:7-29`). |

CLIENT ROWS: FPS-04 FPS-31 FPS-32 FPS-33 FPS-34 FPS-35 FPS-36 FPS-37 FPS-38 IF-13

## Test scope

The FPS suite uses real public Match providers, canonical units/controls/combat state, and real tick dispatch for death and autonomy. It does not simulate a server room. The interface suite checks exported tables against their consumers and reuses existing exact component fixtures. No new Source recorder, behavior fixture or provenance bundle is added.

Only the Rust half of BOTH rows is claimed. ABI exports/adoption, Unreal input and presentation, live sockets and privacy acceptance remain separate. In particular, FPS-21 checks internal shot events, not wire encoding. FPS-11 checks the core reconnect baseline, not a room reconnect. FPS-14 checks authoritative collision, not client reconciliation.

IF-01 combines the existing exact startup-table comparison with `fps_runtime::operative_tables_match_the_pinned_source`. The agreed startup payload explicitly excludes operative constants; that second existing fixture checks their separate typed Rust definitions. Neither this test batch nor the triage adds them to the payload.

IF-31 uses the existing base-geography, dressed-geography and determinism tests. They compare the ten reference seed/size combinations, twenty full-map cases with scenery off and on, loaded terrain state and ordered rail paths. The separate 80-case sweep compares complete generated worlds across sizes and seat/team layouts. These are generator checks, not whole-match World gameplay evidence.

IF-05 is withheld: the native helper rejects non-trench arc/ring/strongpoint patterns that the prototype helper accepts. The added linear-fort consistency test and existing exact valid-domain fixture are useful partial evidence, but this PR neither approves the restriction nor claims complete IF-05 coverage. This is a differs candidate for owner review.

The complete passing-test mapping and dropped rows are recorded in the PR body after execution. A dropped row stays in its current parity status; it is outside this test batch, not removed from the product inventory.

Validation on Vostro Linux x86_64: all 36 selected checks passed (22 new tests and 14 existing checks, 377 unselected tests skipped). `cargo fmt --all --check` and `cargo clippy --workspace --all-targets -j4 -- -D warnings` passed. The final run rebuilt core/simtest artifacts in locked slot S1 under `MemoryMax=6G` and `MemorySwapMax=0`; no full workspace test run was performed. These results cover 25 Rust rows, leave 33 dropped rows explicit, and route 10 client-only rows for owner consideration. Unreal and live operative sockets were not exercised.
