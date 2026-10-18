# Additive C queries, ABI 2.2

Status: implementation candidate under the [owner decision on #5](https://github.com/axelmanosalvasmd/ww2-rts-native/issues/5#issuecomment-6224738109), message `julio-373-merged-decisions-20261016`. Exact signatures require runtime review before Unreal adoption. Reference: `ww2-rts` at `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`. ABI 2.1 comes from #418, which stacks on #389 for detached geography validation. IF-11 stacks on #418 and adds ABI 2.2 presentation queries under the user's explicit additive-function authorization.

## Compatibility and memory

The existing crate had one integer version, `WW2_ABI_VERSION = 2`, with no minor component. That constant and `ww2_abi_version()` remain 2 so an existing ABI 2 consumer's equality check continues to work. ABI 2.1 introduced `WW2_ABI_MINOR_VERSION` and `ww2_abi_minor_version()`. IF-11 raises the minor constant to 2. New consumers first check major 2, then require minor at least 1 for placement/authoring or at least 2 for motion/flight. An older archive has no minor symbol and cannot satisfy the new link. Existing functions, structs, enums, field order and layouts are unchanged. The formation rules hash is unchanged.

Every query in this document returns the existing `int32_t` status codes and runs inside the panic boundary. Results use caller-owned buffers, explicit byte lengths and UTF-8 JSON without a terminating zero. Input is limited to 16 MiB before reading or parsing. A null input is allowed only with zero length, which then fails JSON parsing. Input, output, count storage and any view handle must be disjoint. No returned allocation needs freeing.

A writable `out_len` is required. Buffer validation and parsing set it to zero; a completed query sets it to the required byte count. Capacity zero with null output asks for that count and returns `WW2_ERR_CAPACITY`. An undersized buffer changes no output bytes. A null output with sufficient capacity returns `WW2_ERR_NULL_POINTER`. Other invalid inputs return `WW2_ERR_BAD_ARGUMENT`; invalid UTF-8/JSON returns `WW2_ERR_PARSE`. `ww2_last_error` supplies the diagnostic. Capacity discovery repeats the deterministic query, including bounded simulation for `play_match`.

Placement follows the existing view rules: one exclusive caller per handle, no mutation on ordinary query success or failure, and poison after a caught panic. An empty handle returns `WW2_ERR_WRONG_STATE`; a poisoned handle accepts only free. Authoring queries create no persistent handles or live match state.

## New signatures

```c
uint32_t ww2_abi_minor_version(void);
int32_t ww2_view_placement_check(struct ww2_view *view, const char *json,
    size_t len, char *out, size_t capacity, size_t *out_len);
int32_t ww2_authoring_validate_map(const char *json, size_t len,
    char *out, size_t capacity, size_t *out_len);
int32_t ww2_authoring_migrate_scenario(const char *json, size_t len,
    char *out, size_t capacity, size_t *out_len);
int32_t ww2_authoring_migrate_map(const char *json, size_t len,
    char *out, size_t capacity, size_t *out_len);
int32_t ww2_authoring_spawns_for(const char *json, size_t len,
    char *out, size_t capacity, size_t *out_len);
int32_t ww2_authoring_create_preview(const char *json, size_t len,
    char *out, size_t capacity, size_t *out_len);
int32_t ww2_authoring_find_path(const char *json, size_t len,
    char *out, size_t capacity, size_t *out_len);
int32_t ww2_authoring_house_kinds(const char *json, size_t len,
    char *out, size_t capacity, size_t *out_len);
int32_t ww2_authoring_play_match(const char *json, size_t len,
    char *out, size_t capacity, size_t *out_len);
```

## Motion and flight queries added in ABI 2.2

Each of these five additions has the same signature and caller-owned buffer rules as the ABI 2.1 stateless queries:

```c
int32_t ww2_movement_profile(const char *json, size_t len,
    char *out, size_t capacity, size_t *out_len);
int32_t ww2_is_ground_vehicle(const char *json, size_t len,
    char *out, size_t capacity, size_t *out_len);
int32_t ww2_body_radius(const char *json, size_t len,
    char *out, size_t capacity, size_t *out_len);
int32_t ww2_flight_at(const char *json, size_t len,
    char *out, size_t capacity, size_t *out_len);
int32_t ww2_velocity_at(const char *json, size_t len,
    char *out, size_t capacity, size_t *out_len);
```

| Function | JSON input | JSON result | Prototype source at `e4e579a` |
| --- | --- | --- | --- |
| `ww2_movement_profile` | `{type}` | `{acceleration,reverseAcceleration,braking,reverseSpeed,hullTurn,tracked}` or null | `shared/vehicle-motion.js:1-16`; suspension caller `client/engine-presentation.js:138-141` |
| `ww2_is_ground_vehicle` | `{type}` | Boolean | `shared/vehicle-motion.js:15`; caller `client/engine-presentation.js:138` |
| `ww2_body_radius` | `{type}` | Number, hull half-length in metres | `shared/vehicle-motion.js:17`; caller `client/engine-presentation.js:141` |
| `ww2_flight_at` | `{flight:{x,y,z,vx,vy,vz,gravity,stamp},time}` | `{x,y,z}` position in metres | `shared/projectiles.js:22-25`; client copy `client/projectiles.js:35-36` |
| `ww2_velocity_at` | Same flight/time input | `{x,y,z}` velocity in metres per second | `shared/projectiles.js:26-28`; client copy `client/projectiles.js:35` |

`type` must name one of the 47 compiled unit definitions. Unknown or missing types return `WW2_ERR_BAD_ARGUMENT`. These are catalog queries, so they do not accept a caller-supplied definition. Non-ground units return a null movement profile; the source fallback is retained for ground units without a named profile (including train, lorry and kaiju). All movement profile fields and radii retain the pinned source values.

All eight flight fields and `time` are required finite JSON numbers. `stamp` and `time` use simulation seconds; elapsed time is `max(0,time-stamp)`. Negative times/stamps and zero or negative gravity are accepted when the calculation remains finite. Missing, null or nonnumeric fields and unrepresentable intermediate/results return `WW2_ERR_BAD_ARGUMENT`. Non-JSON numeric tokens return `WW2_ERR_PARSE`. No output component is silently changed to null. JSON consumers may normalize the sign of zero; the source comparison treats signed zeros as equal. No renderer coordinates are applied; Unreal retains its existing metres-to-centimetres and axis conversion.

The flight calls evaluate caller-supplied presentation data. They have no authoritative match or recipient handle and add no simulation state, message, admission rule, extrapolation window or projectile lifetime policy. The source adapter for received flight rows maps `v[0..2]` to `vx/vy/vz` and `time` to `stamp`. The comparison harness exercises the actual client trail calculation at the same elapsed time; it does not claim the client's expiry or extrapolation behavior.

IF-11 qualification calls the exported functions for all 47 definitions, all 10 existing source-recorded flight/velocity cases and generated flight samples. `harness/if11-source-check.mjs` imports the pinned shared helpers and the actual client projectile view, then compares them with the linked C driver. The new signatures remain candidates for Axel's review before Unreal adoption. Earlier three-platform ABI 2.1 receipts below do not qualify ABI 2.2.

## Placement input and result

`ww2_view_placement_check` takes `{kind,x,z,dir?,team?}` and returns `{ok,reason?,cells?,x,z,c?,node?}`. Building footprints are cell indices; fort footprints are ordered `[cell,character]` pairs. Undefined object properties are omitted; undefined fort characters become JSON null, as in JavaScript arrays. Depot results include the selected node object. Coordinates use metres, x east and z south; direction uses radians.

The `reason` string vocabulary is closed for ABI 2.1. Unreal can map these exact, case-sensitive values to UI text:

| Value | Meaning |
| --- | --- |
| `blocked` | The kind, footprint, occupancy or other placement constraint prevents placement. |
| `notVisible` | The required placement position is outside delivered allied sight. |
| `territory` | The footprint is outside owned territory (World-only kernel branch, unreachable through this recipient query). |
| `rail` | The footprint overlaps a known rail yard (the recipient query supplies no rail-yard set, so this branch is unreachable). |
| `coast` | A coastal building on a naval map has no coast within the required footprint neighborhood. |

Successful results omit `reason`. The current recipient query can emit only `blocked`, `notVisible` and `coast`; the exhaustive building serializer also defines `territory` and `rail`. Fort results use only `blocked` and `notVisible`. No diagnostic from `ww2_last_error` becomes a `reason` value. Source: `rust/crates/core/src/recipient_placement.rs:15-22,208-238,293-296,374-421`. Any addition to this closed set must be documented and reviewed before consumer adoption.

Behavior comes from `reference/ww2-rts/shared/sim.js:3261-3305`, with the recipient adapter at `client/availability.js:169-198`. It uses delivered map rows/layers/heights, accumulated cell updates, held units, smoke, teams, nodes, mode and logistics fields. It cannot access an authoritative match. Sight follows the browser's placement adapter, including the fields that adapter omits. Building and fort queries preserve reason precedence, snapped coordinates, node reservations, coastal checks and partial fort plans.

Running World rooms remain closed. World placement returns an explicit bad-argument error because the unchanged view does not retain the prototype adapter's initial-terrain and railway discovery history. Entrenchment/work previews omit `team` (`client/main.js:1208,1238`); direct `footAt` previews pass it (`client/main.js:1297`). The browser recipient game has no `mines`/`mineSeen` tables, so its team-qualified demine path throws when it encounters a mine (`shared/sim.js:456,466`). This C query returns a diagnostic for that case instead of inventing knowledge. These limitations are not successful placement results.

## Authoring input and result

These calls operate on the author's own document. They must not receive an authoritative live map through a recipient workflow.

| Function | JSON input | JSON result | Pinned behavior |
| --- | --- | --- | --- |
| `ww2_authoring_validate_map` | Raw map | null or first diagnostic string | `shared/sim.js:574-620`; geography `1139-1162`; editor `client/editor.js:159-168` |
| `ww2_authoring_migrate_scenario` | Raw map | Scenario object or null | `shared/scenarios.js:7-23`; used at `client/editor.js:70` |
| `ww2_authoring_migrate_map` | Raw map | Cloned v2 map with separate ground/object/mine layers; author metadata retained | `shared/world-layers.js:19-27`; `client/editor.js:71` |
| `ww2_authoring_spawns_for` | `{map,mode}` | Usable source spawn indices | `shared/sim.js:626`; `client/editor.js:119` |
| `ww2_authoring_create_preview` | `{map,options?,seed?}` | JSON setup description: terrain, players/spawns, units, points, winVp and mode setup | `shared/sim.js:682`; `client/editor.js:122-155` |
| `ww2_authoring_find_path` | `{map,from:{x,z,type?},to:{x,z}}` | Waypoint array of `{x,z}` | `shared/sim.js:3020-3033,3068-3145`; editor reachability `client/editor.js:162-168` |
| `ww2_authoring_house_kinds` | `{}` for metadata, or raw map for labels | House metadata; with map: `{metadata,kinds,blocks}` | `shared/sim.js:141-143,2274-2295` |
| `ww2_authoring_play_match` | `{map,options?,seed,max_ticks}` | `{winner,winning_team,ticks,player_vp,timedOut,seed,players,horde}` | Intent of `shared/ai-schedule.js:57-86`; fairness aggregation `client/fairness.js:5-25` |

`validate_map` uses raw authoring validation separately from typed runtime materialization. A valid raw document can still be refused by preview/path/simulation if its values cannot fit the existing native representation. Validation does not open World admission. Migration preserves author-only fields and does not synthesize material tables absent from the prototype migration.

`create_preview` returns a JSON setup description in the caller's output buffer. It constructs a temporary Rust match, copies the setup into a JSON value, and drops that match before the FFI serializes and copies the result. It creates no `ww2_view` handle, persistent match, retained allocation or lifetime that Unreal must release. No free call is needed; Unreal owns its input/output buffers. The header's phrase "creates a detached setup view" describes this JSON result, not a handle. Source: `rust/crates/core/src/authoring_queries.rs:301-344`; `rust/crates/ffi/src/queries.rs:24-66,178-195`.

Preview defaults to seed 0 and unshuffled setup. Match options include mode, player count, names, teams, factions, army, weather, weather seed, living weather, tech, supply, logistics, sandbox, shuffle and Assault defender team. Unsupported or malformed options return a diagnostic. Supported preview constructors are Default/Conquest, Classic, Annihilation, Assault and Horde, within each native constructor's admission rules. No fallback silently converts an unsupported mode to Conquest.

`play_match` requires an unsigned 64-bit seed and an integer `max_ticks` from 0 through 36,000 ticks inclusive. This is a validation ceiling, not a clamp: a larger value, a negative value, a non-integer or a missing limit returns `WW2_ERR_BAD_ARGUMENT`. Zero performs setup without advancing a tick. The call blocks the calling thread until the match finishes, reaches the tick limit or returns an error. It creates no worker thread and has no wall-clock timeout or asynchronous cancellation API. Editor callers should run it on a worker thread to keep the UI responsive. A capacity-only query performs the simulation too; retrying with the requested buffer capacity runs it again. Source: `rust/crates/core/src/authoring_queries.rs:448-478`; `rust/crates/ffi/src/queries.rs:49-59,256-273`.

It uses the existing native ordinary tick runner and delivered-view commander, runs only Default/Conquest without logistics, and refuses unsupported active phases, snapshot producers or commander policies. The existing tick report also lists dormant construction/logistics/rail phases and the already-connected ground-contact phase; the query accepts only those six known labels while checking that production queues, rail actors/runs, reserves and convoys remain absent. Snapshots are checked for every seat on delivery beats. `winner` is the first player index on the winning team, or null; `winning_team` retains the native team identifier, including a draw identifier. Each player result includes its actual spawn and VP, so callers can credit shuffled starts. A limit reached without a finish is a timeout.

`play_match` requires an unsigned 64-bit seed and an integer limit from 0 through 36,000 ticks. It is synchronous and blocks its calling thread until simulation and serialization finish. A capacity-discovery call also runs the simulation; retrying with sufficient output capacity runs it again. It uses the canonical tick runner for Default/Conquest, Classic, Annihilation, Assault and Horde. Mode switches filter logistics, supply and tech through their existing owners. Every seat receives a complete initial recipient frame, then retains delivered terrain changes independently of adaptive brain demand. Human packets come only from the detached SeatMemory/CommanderMemory adapter. The internal Horde seat receives its scripted policy packet. After every simulation tick, all requested packets refresh before any brain acts. Camera attention, delayed inputs, tactical memory and owner hurt alerts remain with SeatCommander/HumanPolicy. Commands enqueue for the next authoritative boundary. Event ordinals persist until the delivery beat clears the batch and advances its generation. Snapshots are checked for every seat on delivery beats and on finish. `winner` is the first player index on the winning team, or null; `winning_team` retains the native team identifier, including a draw identifier. Each player result includes its actual spawn and VP, so callers can credit shuffled starts. A limit reached without a finish is a timeout. Additive `horde` progress is null outside Horde or `{wave,active,timeLeft}` from the existing mode owner, so authoring clients can inspect the played wave without reading private director state.

Explicit scenarios use canonical startup, authored groups, triggers and recipient filtering. Null and other falsey scenario values remain absent, using the same selection as migration (`shared/scenarios.js:7-14`). Horde accepts defender-authored groups and reinforcements, but refuses either for the internal Horde slot, matching `shared/sim.js:818-823,4884-4897`. One through five defenders and all four army sizes are supported by detached authoring; six requested defenders select the existing Conquest fallback. The editor selector does not offer Tutorial (`client/editor.js:43`), so that mode remains outside these calls. Localized announcements prefer English, then JavaScript's numeric-index key order, then author insertion order (`shared/scenarios.js:160`). JSON object order is retained for this fallback without changing the pinned serde_json version.
The runner is deterministic. The C-entry tests compare a 60-seed ordinary batch with the pinned worker's actual-spawn attribution, rounded percentages and verdict precedence (`client/fairness.js:11-19`, `client/editor.js:392-399`). Completed mirrored matches and a map requiring movement provide separate finite-outcome checks. Independent native random streams can change finish ticks and second-place VP; these tests assert outcome intent, not exact whole-match state.

World authoring runtime stays deferred. The raw validation, migration and spawn-query calls remain available for authored World documents; preview, paths requiring native World context and running World fairness remain outside runtime admission. Unreal consumption and packaged-client testing are separate.

## Verification

The `ww2-ffi` tests call these public C entry points directly. Existing header generation, ABI/error tests and C replay checks remain active. New compatibility checks pin all ABI 2 field offsets and function types and retain legacy declaration order. The C replay program links every new symbol and checks the JSON size/error path. Test mapping and qualification commands are in `docs/testing/abi-queries.md`; the PR records final committed-head results.

### Hosted static-library builds

The [Rust workflow](../../.github/workflows/rust.yml) builds `ww2-ffi` with `cargo build --locked -p ww2-ffi --release` in three independent `Release library` jobs: `ubuntu-24.04`, `windows-2025`, and `macos-15`. Confirm the requested Linux x86_64, Win64 MSVC and macOS arm64 targets against each job's reported Rust host at the reviewed head. The crate declares `staticlib` in `rust/crates/ffi/Cargo.toml:12`. Linux also links and runs the C replay against `libww2_ffi.a`. This checks static-library builds, not Unreal adoption or packaged-client behavior.

Confirmed at #418 head `581962e768b68b36253c3d5faa4eec4c12fde2c4`, Rust tree `558fc2f5291637c8694a9455dac3ee8d578aeb5d`, by hosted run `38987359931` (attempt 2):

| Platform | Hosted Release library job | Result |
| --- | --- | --- |
| Linux x86_64 (`x86_64-unknown-linux-gnu`) | [ubuntu-24.04](https://github.com/axelmanosalvasmd/ww2-rts-native/actions/runs/38987359931/job/117263341483) | Success, including C link/replay |
| Win64 MSVC (`x86_64-pc-windows-msvc`) | [windows-2025](https://github.com/axelmanosalvasmd/ww2-rts-native/actions/runs/38987359931/job/117263306658) | Success |
| macOS arm64 (`aarch64-apple-darwin`) | [macos-15](https://github.com/axelmanosalvasmd/ww2-rts-native/actions/runs/38987359931/job/117263305234) | Success |

The logs confirm these target triples. An external cancellation interrupted earlier attempts; `gh run rerun --failed` preserved the Linux/Windows successes and completed macOS. These receipts cover the earlier head only. Concurrent branch integration changed the Rust tree to `737551dc33e15eedfb045847e33ada9964b36027`; the final pushed-head Release library results are recorded separately in the ABI-DOCS-418 final handoff and can be found in the [branch's hosted runs](https://github.com/axelmanosalvasmd/ww2-rts-native/actions/workflows/rust.yml?query=branch%3Aruntime%2Fabi-queries-20261016). Results from an older head or a cancelled job do not replace that final-head check. The workflow permits `workflow_dispatch` if a documentation push does not start a run.

Resolver qualification on Vostro Linux x86_64 passed for Rust tree `737551dc33e15eedfb045847e33ada9964b36027`: all 36 FFI tests, the added Classic startup projection check, formatting, strict workspace/all-target Clippy, the release static library, and the matching-header C replay. The replay passed ABI 2.1 queries and all 32 formation cases (215 spots). The concurrent consumer-documentation merge changes no Rust bytes. Hosted release checks at the final pushed head remain the platform evidence; this Linux receipt does not confirm Windows or macOS execution. ABI 2.1 adoption and any Unreal repin remain Axel's decision.
