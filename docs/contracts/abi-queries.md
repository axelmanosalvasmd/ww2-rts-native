# Additive C queries, ABI 2.1

Status: implementation candidate under the [owner decision on #5](https://github.com/axelmanosalvasmd/ww2-rts-native/issues/5#issuecomment-6224738109), message `julio-373-merged-decisions-20261016`. Exact signatures require runtime review before Unreal adoption. Reference: `ww2-rts` at `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`. This stream stacks on #389 for detached geography validation.

## Compatibility and memory

The existing crate had one integer version, `WW2_ABI_VERSION = 2`, with no minor component. That constant and `ww2_abi_version()` remain 2 so an existing ABI 2 consumer's equality check continues to work. This addition introduces `WW2_ABI_MINOR_VERSION = 1` and `ww2_abi_minor_version()`. New consumers first check major 2, then require minor at least 1. An older archive has no minor symbol and cannot satisfy the new link. Existing functions, structs, enums, field order and layouts are unchanged. The formation rules hash is unchanged.

Every new query returns the existing `int32_t` status codes and runs inside the panic boundary. Results use caller-owned buffers, explicit byte lengths and UTF-8 JSON without a terminating zero. Input is limited to 16 MiB before reading or parsing. A null input is allowed only with zero length, which then fails JSON parsing. Input, output, count storage and any view handle must be disjoint. No returned allocation needs freeing.

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

## Placement input and result

`ww2_view_placement_check` takes `{kind,x,z,dir?,team?}` and returns `{ok,reason?,cells?,x,z,c?,node?}`. Building footprints are cell indices; fort footprints are ordered `[cell,character]` pairs. Undefined object properties are omitted; undefined fort characters become JSON null, as in JavaScript arrays. Depot results include the selected node object. Coordinates use metres, x east and z south; direction uses radians.

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
| `ww2_authoring_create_preview` | `{map,options?,seed?}` | Detached setup terrain, players/spawns, units, points, winVp and mode setup | `shared/sim.js:682`; `client/editor.js:122-155` |
| `ww2_authoring_find_path` | `{map,from:{x,z,type?},to:{x,z}}` | Waypoint array of `{x,z}` | `shared/sim.js:3020-3033,3068-3145`; editor reachability `client/editor.js:162-168` |
| `ww2_authoring_house_kinds` | `{}` for metadata, or raw map for labels | House metadata; with map: `{metadata,kinds,blocks}` | `shared/sim.js:141-143,2274-2295` |
| `ww2_authoring_play_match` | `{map,options?,seed,max_ticks}` | `{winner,winning_team,ticks,player_vp,timedOut,seed,players}` | Intent of `shared/ai-schedule.js:57-86`; fairness aggregation `client/fairness.js:5-25` |

`validate_map` uses raw authoring validation separately from typed runtime materialization. A valid raw document can still be refused by preview/path/simulation if its values cannot fit the existing native representation. Validation does not open World admission. Migration preserves author-only fields and does not synthesize material tables absent from the prototype migration.

Preview defaults to seed 0 and unshuffled setup. Match options include mode, player count, names, teams, factions, army, weather, weather seed, living weather, tech, supply, logistics, sandbox, shuffle and Assault defender team. Unsupported or malformed options return a diagnostic. Supported preview constructors are Default/Conquest, Classic, Annihilation, Assault and Horde, within each native constructor's admission rules. No fallback silently converts an unsupported mode to Conquest.

`play_match` requires an unsigned 64-bit seed and an integer limit from 0 through 36,000 ticks. It uses the existing native ordinary tick runner and delivered-view commander, runs only Default/Conquest without logistics, and refuses unsupported active phases, snapshot producers or commander policies. The existing tick report also lists dormant construction/logistics/rail phases and the already-connected ground-contact phase; the query accepts only those six known labels while checking that production queues, rail actors/runs, reserves and convoys remain absent. Snapshots are checked for every seat on delivery beats. `winner` is the first player index on the winning team, or null; `winning_team` retains the native team identifier, including a draw identifier. Each player result includes its actual spawn and VP, so callers can credit shuffled starts. A limit reached without a finish is a timeout.

The bounded native runner is deterministic. It does not establish equal fairness verdicts with the prototype's full AI schedule, which observes at tick zero and supports broader policies/modes. IF-12 remains incomplete until that outcome comparison and the complete editor surface are proven. Unreal consumption and packaged-client testing are separate.

## Verification

The `ww2-ffi` tests call these public C entry points directly. Existing header generation, ABI/error tests and C replay checks remain active. New compatibility checks pin all ABI 2 field offsets and function types and retain legacy declaration order. The C replay program links every new symbol and checks the JSON size/error path. Test mapping and qualification commands are in `docs/testing/abi-queries.md`; the PR records final committed-head results.
