# Versioned public startup tables: source comparison and schema proposal

Status: **unagreed proposal for consumer review**. This is a data/schema proposal, prepared from main `d1f2fb5d31efefb4902576142b135ddab9078920` and the clean read-only reference `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`. It changes no runtime source, header, FFI call or websocket message. `schemaVersion: 1` below is a proposed delivery schema, not a claim that a table interface exists. Axel's actual [034 reply](https://github.com/axelmanosalvasmd/ww2-rts-native/issues/5#issuecomment-6155467855) requires separate table-schema agreement and consumer evidence. The current Unreal consumer source is unpublished; this artifact does not prove adoption.

## Review files

| File | Content |
| --- | --- |
| [`rust/crates/core/data/rules.json`](../../../rust/crates/core/data/rules.json) | Existing embedded catalog at the recorded main baseline; its exact hash is checked rather than duplicating it here |
| `proposed-content.json` | Complete proposed public static data, with all existing 47 tables and 17 source-derived additions |
| `proposed-content.schema.json` | JSON Schema 2020-12 for this pinned proposal: exact table and unit keys, per-unit field presence, source-ordered name arrays, reserved number tags |
| `comparison-inventory.json` | Catalog coverage, missing constants and source locations, exact JSON pointer/type inventory, and unit/weapon/ability field presence by unit |
| `manifest.json` | Exact source pins, file hashes, measured counts and decisions requiring agreement |
| `generate.py` | Local source extraction and artifact generation; it does not run the game or alter repository files |

The content has 64 tables, 47 unit definitions, 8 supports, 12 forts, 6 entrenchments and 5 weather kinds. Its 2,807 JSON paths include containers and array entries. Existing compiled data is 28,972 bytes and SHA256 `96ddf08289375e2338d8efa5c9f8994a9213834691e516e023212bb3d27133df`. The complete proposed content is 30,618 bytes, SHA256 `5de90a7990ec5e8a2903436ac4809302464be29b546d79591a2adc08c68a2efd`. These hashes identify these local review artifacts only.

## Exact proposed JSON shape

```json
{
  "schemaVersion": 1,
  "reference": "e4e579a838aabae3b1d9dadbc17a0a87147bc52d",
  "formation": {
    "hash": "ed0ee23d32a621d1",
    "scope": "formation-only",
    "shapes": ["line", "block", "column", "wedge"],
    "numbers": {
      "infantrySlot": 3,
      "vehicleSlot": 5,
      "slotMargin": 1.5,
      "rankGap": 1.5,
      "lineRank": 10,
      "maxStretch": 3,
      "depthSteps": 100
    }
  },
  "tables": {}
}
```

The empty `tables` above is an abbreviated illustration. The supplied content and schema define all 64 entries and their full nested fields. No table may be synthesized from live Match state. A static definition for an unsupported mode or unit does not enable that behavior.

## Source coverage and required fields

`docs/parity/client-imports.md` inventories 86 distinct imports across 40 browser files, plus the `GUN_SLOTS` re-export. Its `C` entries identify data, while functions and live-state queries remain separate work. `docs/parity/interface.md` IF-01 still marks C table export and consumption pending. The merged six-call ABI 2 scope in `docs/contracts/README.md` does not approve new table calls.

The embedded catalog is complete for its captured exports. `harness/rules-fixture.mjs:49-67` collects 47 tables after module initialization and records every field, including resolved movement-profile fallbacks. `Catalog` in `catalog.rs:76-124` retains the entire recursive object, uses ordered keys, and exposes table/unit/support accessors. There is no public JSON table producer or delivery API. `Rules::from_catalog` in `rules_catalog.rs:77-167` builds a reduced movement/vision/fog projection. In particular, its unit `weapon` member is a truthy flag derived from `w`, and its ability field retains only speed. Exporting typed `Rules` would lose the weapon, ability, HUD, economy, support and presentation data required below.

| Data family | Exact source fields and treatment |
| --- | --- |
| Units | Preserve the complete `UNITS` object for every name in `UNIT_TYPES`, with source order in that array. Field union: `aa, ab, air, ammo, boss, buildTime, building, camo, carries, classic, classicCost, coast, cost, crushes, deep, faction, frontArmor, fuel, garrisons, gate, hpPer, hull, infantry, logisticsCache, logisticsTruck, lookout, makes, max, medic, models, name, naval, needs, pop, produces, radius, rail, reinforces, riders, role, size, slit, speed, structure, train, vision, w`. These are a union, not fields required on every unit. Per-unit presence and null values are retained in the supplied schema and inventory. |
| Weapons | Keep `UNITS.<type>.w`, not a renamed `weapon` field. Field union across 34 object definitions: `accInf, accVeh, antiGarrison, area, beam, blast, every, flame, flight, inf, interval, minRange, moveFire, perModel, range, rockets, salvo, setup, shellTerrain, spread, spreadFar, supp, terrain, veh`. Preserve absent or null weapons. |
| Abilities | Keep `UNITS.<type>.ab` exactly. Field union: `cd, dur, fuse, id, inf, mult, mun, name, naval, radius, range, shells, speed, supp, terrain, veh`. Preserve each object's source fields and missing/null distinctions. |
| Formation and squads | `SHAPES`, all seven formation numbers in the displayed fixed order, `SLOTS`, `SLOT_SCALE`, `GUN_SLOTS`. Formation tables do not replace the approved deterministic formation calls. |
| Rules and economy | Entire `CFG`, `CELL`, `TICK`, terrain masks, `TERRAIN`, `SUPPORT`/`SUPPORT_TYPES`, `FORTS`, `ENTRENCH`/`ENTRENCH_TYPES`, `TECH`, `TIER`/`TIER_NAMES`, build lists, `AB_MUN`, `DOCTRINE`, `WORLD_TUNING`, weather tables, traffic and resolved movement profiles. Nested fields and ordered arrays are fully specified in the supplied data and pointer inventory. Do not infer current player resources or availability from static data. |
| Snapshot columns | Exact `UNIT_ROW` columns 0 through 15, `GATE_OPEN_FLAG`, `AUTO_FLAG`, `OPERATIVE_FLAG`, `RIDING_FLAG`, `CARGO_FLAG`, `CARGO_SHIFT`. Defining bits does not approve any additional recipient visibility or operative starts. Existing ClientView remains the decoder owner. |
| Public presentation | `RAIL_DIRS`, `DEBRIS_LIMITS`, `SECTION_MASS`, map-format `WORLD_VERSION`, `BASE_SURFACES`, `OBJECT_TYPES`, `BIOMES`, `MATERIALS`. These are static definitions, never a particular map's mines, triggers, scenario or discovered regions. |
| Authoring metadata | `MAX_PLAYERS`, `POINT_KINDS`, `STRUCTURE_LIMITS` and the editor use of surfaces/materials are included as public constants for schema review. This does not approve an Unreal editor or runtime structure capability. |

The browser uses only selected `CFG` fields directly, including `levelHeight`, `pointRadius`, `reinforceRadius`, `fortBuilders`, `houses`, `paraRange`, `rail`, `catchupMax`, `catchupPer`, `armies`, `terrainHp`, `stoneBridge`, `aid.radius` and `mine.detect`. Keeping the full source object avoids a second incomplete rules projection and permits exact comparison to the core's catalog. The proposed profile includes public static constants only.

The compiled catalog lacks 17 proposed additions: `AUTO_FLAG`, `BASE_SURFACES`, `BIOMES`, `CARGO_FLAG`, `CARGO_SHIFT`, `DEBRIS_LIMITS`, `GATE_OPEN_FLAG`, `MATERIALS`, `OBJECT_TYPES`, `OPERATIVE_FLAG`, `POINT_KINDS`, `RAIL_DIRS`, `RIDING_FLAG`, `SECTION_MASS`, `STRUCTURE_LIMITS`, `UNIT_ROW`, `WORLD_VERSION`. Exact values and source locations are in the inventory. `WORLD_VERSION`, `OBJECT_TYPES` and `BIOMES` are supporting map-format metadata beyond the direct constant imports. `OPERATIVE` and `OPERATIVE_WEAPONS` remain excluded because FPS acceptance is deferred. `BUILDABLE` remains in the original 47-table catalog for source fidelity, although its browser imports are dead.

## Version, hash and numbers

Actual source has catalog `format: 1`, reference map `WORLD_VERSION = 2` and ABI version 2. Those identifiers describe different formats. None establishes a delivered startup-table schema today. Proposed `schemaVersion: 1` identifies the exact JSON shape in this artifact. Both owners must agree which incompatible schema changes require a new version and whether compatible table-content changes retain that version. A content change must produce a different content hash even when the schema version stays the same.

The existing `ww2_rules_hash()` returns `ed0ee23d32a621d1`. `rules.rs:13-40` explicitly limits its FNV-1a64 input to each formation shape UTF-8 name plus NUL, then the seven formation numbers' IEEE754 binary64 little-endian bytes in `formation.rs:35-44` order. This review independently recomputed that exact value in Python from the opened source. It does not hash the catalog and must not be presented as full-table compatibility proof. Use a lowercase, zero-padded 16-character hexadecimal string for this value in the proposed data. Sending it as a JSON number could lose integer precision in JavaScript.

Proposed full-content identity is lowercase 64-character SHA256 of the exact received UTF-8 content bytes. Keep that hash in a separate manifest or transport metadata so it does not hash itself. The review fixture generator sorts object keys, preserves every source array order, uses compact JSON and includes one final LF. A future Rust producer must freeze its own deterministic encoding and compare it against a source recorder before claiming compatibility. Consumers hash received bytes before parsing; reserializing floats in C++, JavaScript or Rust is not a reliable byte comparison. A semantic comparison must inspect every decoded field and IEEE754 value separately.

The existing recorder reserves `{"$number":"NaN"}`, `{"$number":"Infinity"}`, `{"$number":"-Infinity"}` and `{"$number":"-0"}`. Ordinary numbers remain finite binary64 JSON numbers. This proposal preserves that convention. The current catalog has three `Infinity` tags at `CFG.houses[3..5].min`; other tags are admitted by the schema because the existing recorder/parser support them, not because they occur in this content. Reject mixed or malformed reserved-tag objects. Preserve missing fields separately from null, false and zero. Do not fill absent fields with native struct defaults.

## Producer ownership and agreement needed

Rust core should own one static content producer from `Catalog` plus the source-verified constants missing from it. Extending the recorder and compiled catalog would make those constants available without copies on the Unreal side. Server transport and any client-facing adapter should consume those exact producer bytes. The Unreal adapter should validate schema version and content hash, decode number tags, retain source name-array order and read the resulting immutable cache. It must not derive tables from a live Match or recreate rules in Blueprint.

This artifact proposes content, not an endpoint, websocket envelope, startup order or new C call. Both owners still need to agree the delivery mechanism, memory ownership, errors, recovery and compatibility transition. The existing six ABI 2 calls and current websocket startup repair need no table transport change.

Concrete consumer review can now compare `proposed-content.json` against the full field inventory and confirm whether the Unreal RTS renderer/HUD needs any additional public constant. The accepted implementation must then prove exact Node-export versus Rust-producer field/value equality, byte hash reproducibility on Linux/Windows/macOS, rejection of incompatible schema or malformed tags, and actual Unreal parser/cache consumption at the same published source head. A decoder-only test cannot prove table adoption.

## Remaining decoder and mirrored-rule dependencies

The static-import closure is not the complete browser rule/query interface. The client inventory also records literal decoders and copied rules. Exact cell state at `sim.js:2352` uses wear mask 3, burnt mask 4, damage stage `(state >> 3) & 3`, and shelling stage `(state >> 5) & 3`. Literal unit masks used by the browser include retreating 1, garrisoned 32 with house type `(flags >> 20) & 7`, hidden/camouflaged 256, grounded aircraft 512 and auto-retreat 8192. `stanceBits:3334` adds hold-fire 2048, hold-position 4096 and auto-retreat 8192. Source `playerRow:5900-5904` filters non-owner flags with `2047 | 32768 | (7 << 20) | RIDING_FLAG`; this is existing behavior, not approval of a new privacy rule.

Those packed-field definitions are source evidence for consumer review, not additional named table exports in the proposed payload. If the renderer needs named metadata for these fields, the schema review must select that metadata or agreed canonical view queries before implementation. Static tables also do not implement formation/entrenchment arithmetic, affordability, placement, weather presentation functions or owner-state queries. The proposal must not be used to claim those interface rows complete.

## Reproduce the artifacts

With Python 3.10+ and the reference submodule initialized at its recorded pin, run from the repository root:

```sh
python3 docs/contracts/startup-tables/generate.py --check
```

`--check` reads pinned Git blobs and compares generated bytes without writing. If this worktree has no initialized submodule, pass `--reference PATH` for another read-only checkout at the exact pin. Run without `--check` to regenerate only this proposal's four JSON files after an intentional proposal edit. The standard-library checker validates every structural constraint emitted by the generator. The schema also passed the existing local Python `jsonschema` package's full Draft 2020-12 meta-schema and content validation when prepared. No dependency was added to the repository.

The generator reads the embedded catalog and formation constants at the recorded native baseline, and additional public constants at the reference pin. It checks the unchanged catalog byte hash, all 47 original table values, 17 non-overlapping additions, source-order name arrays and the formation-only hash. A different baseline or changed source requires an explicit proposal update and another consumer review. This generation step does not execute Node, build Rust or prove a future runtime producer.

## Decisions requested from the consumer owner

Please review the full data and schema and decide: the required RTS public fields, inclusion of authoring/map metadata and deferred constants; the proposed field names and revision policy; separate formation-only and full-content hash representations; strict number-tag and absent/null handling; and which packed-field metadata or canonical view queries the renderer needs.

For transport, choose the mechanism and define memory lifetime, startup ordering, error/recovery behavior and compatibility transition before implementation. This proposal specifies no endpoint, websocket packet or additional FFI call. Confirm the Unreal parser/cache owner and the exact-head consumption evidence required to agree that later implementation. Schema agreement, implemented delivery and verified adoption are separate outcomes.
