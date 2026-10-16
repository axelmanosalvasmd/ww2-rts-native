# Native match save format, version 1

This implements owner group I as recorded in [issue #5, comment 6217590741](https://github.com/axelmanosalvasmd/ww2-rts-native/issues/5#issuecomment-6217590741). The compatibility source is `reference/ww2-rts` at `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`. Hermes' exact-head implementation acknowledgement remains a merge requirement. This document does not mark another draft contract agreed.

The envelope carries the prototype's game, local defender, raw map, seats, settings and planner-memory content. JSON with a version field replaces Node's V8 binary serialization. Existing Node `.save` files are not imported. Version 1 binds this native state schema and its compiled rules; incompatible schema or rules changes require a format-version increment. Files and credentials remain server-private. The deterministic core encodes and decodes in-memory state and performs no file I/O.

## Files and summaries

`SAVES_DIR` selects the directory in the server executable; the default is `saves/` beneath its working directory. Library hosts set `Config.saves_dir`. Workers share that directory and each room belongs to one worker. Filenames are `<room>-<13-digit Unix milliseconds>-<manual|auto>.save`, matching `server/saves.js:11-14,29-35`. The body is gzipped UTF-8 JSON. A write creates `.tmp`, completes it, then renames it to `.save`. A separate `.json` file holds the summary. Existing IDs are preserved by advancing the filename timestamp on a same-millisecond collision.

Only one command executes against a room at a time. File operations currently run synchronously under the host's room hub lock. This preserves capture and lobby ordering; moving file operations to a worker is a performance follow-up. No performance claim is made.

The newest ten manual files and three auto files are retained separately per room, deleting both body and summary (`server/saves.js:33-35,53-56`). The storage component accepts both kinds. World admission remains closed, so this batch adds no automatic-save producer.

Summary keys follow `server/saves.js:33` and `server.js:428-429`:

| Key | Value |
| --- | --- |
| `id` | Filename without extension. |
| `kind` | `manual` or `auto`. |
| `savedAt` | Unix milliseconds; also orders the list, newest first. |
| `bytes` | Compressed body length; private, absent from lobby messages. |
| `mode` | Room mode string. |
| `minutes` | Whole simulation minutes, `floor(tick / 1200)` at 20 Hz. |
| `names` | Saved seat names in roster order. |
| `world` | Optional `{total, owned}`: total regions and region counts in distinct team order. Stored summaries can retain this shape, but running World save/load is refused. |

A lobby exposes exactly `id, kind, savedAt, mode, minutes, names`, plus `world` when present (`server.js:340`). It never includes `bytes`, tokens or the save body. Listing is synchronous, room-scoped and newest first, preserving `server/saves.js:37-48`. A missing directory or malformed summary contributes no row.

## Body keys

The root has these seven required keys. The six content owners correspond to `server.js:425-426` and `server/saves.js:16-26`; `version` is the owner-approved native format addition.

| Key | Value and source |
| --- | --- |
| `version` | Integer `1`. Any other value is refused before native state restoration. |
| `g` | Lossless native `Match` checkpoint, described below. It owns every implemented authoritative ordinary-match component. The prototype stores the whole `g` at `server/saves.js:18`. |
| `guard` | Separate local-defender player, corresponding to the non-enumerable `g.players[-1]` at `server/saves.js:15-26`. Currently `null`; a non-null guard is refused by room admission while World is closed. |
| `map` | Full raw authored map, including private layers and triggers. It is never replaced with `mapForClient`. The prototype uses `null` in World (`server.js:426`); native World restoration is refused. |
| `seats` | Saved roster in simulation slot order, with the keys below (`server.js:425`). |
| `settings` | Exactly the nine setting keys below (`server.js:419,426`). |
| `ai` | Planner memories, separately owned from `g` (`shared/ai.js:159-165`). Currently `null`; non-null memories and AI seats are refused until hosted planner restoration exists. Commander state is not added to this envelope. |

Each saved seat has `name`, `token`, `ai`, `level`, `team`, `faction`. `name` removes a trailing ` (AI)`; `ai` is boolean; `level` is the difficulty string, or `null` for a human without an AI level; `team` and `faction` are the original numeric values (`server.js:425`). A valid UTF-16 token is saved as a JSON string. A token ending in an unpaired surrogate uses `{utf16: [code units]}` so the native server's existing truncated token identity survives JSON without replacement characters. This is a private format representation, not a socket field.

The setting keys and values are:

| Key | Value |
| --- | --- |
| `mode` | Room mode, currently admitted as `conquest`. |
| `worldSize` | World-size setting string. |
| `mapName` | Authored map key. |
| `mapSpawns` | Raw authored spawn objects, preserving `x`, `y` and optional `assault`. |
| `army` | Army-size string. |
| `weather` | Weather choice string. |
| `defenderTeam` | Defender team number. |
| `logistics` | Match logistics switch. |
| `tech` | Match technology switch. |

The list is identical to `server.js:419`. In particular, `sandbox` is not a setting key. Sandbox restoration remains refused. Ordinary logistics checkpoints preserve reserves, convoy stores, cargo, routes and supply state through the existing versioned match checkpoint.

## Game checkpoint encoding

`g` has exactly `state` (the Match checkpoint), `pendingEvents` (unsent native tick events, preserving the next recipient frame), and `held` (the ending counter from 0 to 119, corresponding to the prototype's `g.held` at `server.js:743-753`). The first two values use a tagged JSON value model to preserve the native owners without flattening maps, sets, tuples or IEEE-754 sentinels into ambiguous JSON. Structs are maps from tagged field-name strings to tagged values. Maps are ordered arrays of `[key, value]` pairs, so structured keys such as a queued order's tick, issuer and sequence remain intact. Sequence order is preserved. Enum variants use the codec's normal tagged Serde variant representation. The complete list of tags, their payloads and enum representation is defined here; malformed tags and payloads are refused.

| Tag | Payload |
| --- | --- |
| `bool`, `i`, `u` | Boolean, signed 64-bit integer or unsigned 64-bit integer. |
| `i128`, `u128` | Decimal string holding a 128-bit integer. |
| `f32`, `f64` | Unsigned integer holding the exact IEEE-754 bits, including NaN and negative zero. |
| `char`, `string` | Unicode scalar or string. |
| `bytes` | Array of byte values. |
| `unit`, `none` | No payload. |
| `some`, `newtype` | One recursively tagged value. |
| `seq` | Array of tagged values, also used for tuples. |
| `map` | Array of tagged key/value pairs, also used for structs. |

Each node is `[tag, payload]`, except `["unit"]` and `["none"]`. A unit enum variant is a tagged string containing its variant name. Other enum variants are single-entry tagged maps from their variant name to the tagged newtype, sequence or field-map payload.

All fields in `Match` and its nested mutable owners participate in the checkpoint. This includes `world`, `start` (economy, points, weather and RNG streams), `options`, commands and pending orders, production, rallies, field work, combat, firing, abilities, recovery, environment, logistics, supply, unit allocation, story, fallen locations, finish state and timeline. Recipient routing knowledge, fog, remembered buildings, path requests, retries and route caches also survive. Immutable compiled rules are reconstructed from the binary. World runtime and geography are excluded behind an explicit save/load refusal.

Native cache identities that refer to vector allocations must be rebound to restored storage, including convoy handoff paths, per-owner ordinary and danger paths, and their owned danger and vehicle buffers. They are not dereferenced from saved addresses. This is an internal native persistence choice under IF-27, recorded in `design.d/2026-10-16-save-load.md`: the prototype loses WeakMap path/cache state (`shared/sim.js:2865,3035`), while native checkpoints retain state required for deterministic continuation. It does not claim JavaScript tick equality or import compatibility.

The `state` struct field keys are listed below. These are native component boundaries for the prototype's whole-game payload (`shared/sim.js:709-728`, terrain and optional arrays at `:735-766`), rather than additional socket keys. Nested records use the same tagged struct/map/sequence rules and the typed fields of the named canonical owner.

| Native field keys | Saved content |
| --- | --- |
| `world` | Live game, mutable terrain, immutable initial terrain, structures, private map layers, observation journals and versions. |
| `start`, `options` | Original setup, current wallets and objectives, seeds and RNG streams, weather and supported match switches. |
| `aviation`, `support`, `firing`, `combat` | Units in flight, support schedules, projectiles, salvos, combat metadata and producer event state. |
| `field_works`, `abilities`, `recovery` | Paid engineering work, project membership, ability timers and recovery metadata. |
| `environment`, `logistics`, `convoys`, `supply_net`, `supply_routes` | Environmental effects, reserves, convoy state and supply ownership, within current admission. |
| `commands` | Unit controls, waiting orders, the recorded tick queue, path requests, retries and path caches. |
| `grid` | Spatial records, bucket order, owner membership and allocator order. Hash-container keys serialize in sorted order; each bucket keeps its original unit order. |
| `recipients`, `routing_fog`, `classic_known_buildings` | Recipient terrain and object memories, routing knowledge, explored fog and shared building memories. |
| `building_cells`, `production`, `building_rallies`, `player_rallies`, `recruit_metadata`, `produced_placer` | Building identity, production jobs, rally points, completed recruit metadata and placement state. |
| `capture_ticks`, `tick_state`, `ordinary_snapshot_only` | Capture clocks, tick phase state and the existing delivery capability restriction. |
| `story`, `fallen`, `finish`, `timeline`, `next_unit_id` | Match statistics, fall locations, terminal decision, result samples and the canonical actor allocator. |
| `assault`, `horde`, `tutorial`, `scenario`, `companions`, `rail_runs` | Existing mode and operative owners. Inactive records are retained; their presence does not grant room admission. |
| `elimination` | Optional Annihilation/Classic owner: `teams`, `bunkers`, `time_left`, `sudden_death`, `nodes`. Each resource node retains `cell`, `at` (`x`, `z`), `depot`, `rate`, `fuel`. |
| `rules` | Omitted and reconstructed from compiled rules. Saving modified rules is refused. |
| `world_runtime` | Omitted; saving or loading a World runtime is refused. |

`elimination` uses the existing tagged codec: `none` when inactive, otherwise `some` containing a struct map. Its timer, node positions and rates retain exact `f64` bits. These owners correspond to the prototype's `g.mode.teams`, `g.mode.bunkers`, `g.mode.timeLeft`, `g.mode.suddenDeath` and `g.nodes` (`shared/sim.js:792-795,1679,1705-1721,5760-5761`). At pin `e4e579a`, `server.js:425-426` passes the complete game to `snapshotState`, which serializes `g` at `server/saves.js:18-20`.

Version remains 1. The hotfix completes serialization of the existing `Match.elimination` field and documents its nested keys; it adds no envelope or socket keys. Main `57ebeb8e`, which combined elimination modes and versioned saves, could not compile and therefore could not write a v1 checkpoint. A checkpoint from another build that omits `elimination` still deserializes that optional owner as `None`, so otherwise compatible non-elimination v1 saves remain loadable. Missing state is not reconstructed for Annihilation or Classic: their validators reject it. A present elimination record must contain its required typed fields; incomplete records are refused.

Annihilation and Classic remain excluded by ordinary recipient snapshot admission, including the room save/load path. Their full Match checkpoint codec is tested independently, including Classic sudden death, resource nodes and next-tick continuation. Serializing their owners does not open room admission.

## Admission and delivery

`save` requires the current host and an attached, non-faulted match. Success sends exactly `{t:"saved", id}` followed by a lobby update. Failure sends `{t:"deny", cmd:"save", reason:"save"}`. These message fields match `server.js:690-693`.

`loadSave` requires the current host in a lobby and an ID belonging to that exact room (`server.js:694-695`). File decoding, native validation, settings, raw-map binding, roster binding and all recipient start snapshots must succeed before changing the room, epoch or delivery accounts. Failure sends `{t:"deny", cmd:"loadSave", reason:"save"}`.

Human seat assignment follows the prototype's two passes (`server.js:439-445`). First every saved human matches an unused present identity by nonempty token, including offline identities. Then remaining seats take unused connected humans in present order. Current names and tokens survive a match; saved teams and factions replace current choices. Unmatched humans wait offline with their saved identities. This batch refuses AI seats, planner memories and surplus connected humans that would need running spectator delivery. Those cases remain incomplete under NET-92 and AI-63.

A successful load clears the previous result and opens an indefinite host pause with exactly `{t:"pause", paused:true, by:<current host name>, reason:"host", left:0}` (`server.js:393-412,470-485`). It resets drop-pause allowances. The playing lobby precedes every start, and every connected seat receives its start and pause before a tick can run. The loaded room ignores gameplay while paused. Its current host may send `resume`, which uses the existing `{t:"pause", paused:false, by:"", reason:"host", left:0}` shape. Guests remain refused. The host pause has no reconnect or time expiry.

## Batch scope

Intent tests cover the save portions of NET-28 and NET-80, plus NET-89, NET-91 and NET-93. They also compare a running saved match's continuation with uninterrupted native execution, using held fire, movement and queued AutoRetreat stance changes, collecting both seats' complete recipient snapshots on every tick through a VP victory and subsequent ending ticks. Unsupported-version refusal must preserve the lobby and epoch.

NET-12 (editor writes), MODE-55 (Horde records), NET-90 (World autosave), the full NET-71 callback ordering, full NET-92 seat/AI/spectator restoration and AI-63 planner memory are not claimed by ordinary match persistence.
