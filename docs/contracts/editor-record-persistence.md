# Editor maps and Horde records

Owner decisions I and K were recorded in #5 comment 6217590741 and confirmed by Axel in comment 6218552386. Compatibility source: prototype `e4e579a`. Hermes' exact-head runtime acknowledgement remains required. This extends server persistence without changing the C interface or headers. Match saves retain v1 with optional elimination state, as documented in [save-format.md](save-format.md).

## Editor HTTP

The executable opens writable `maps/` overrides over the embedded authored catalogue. Library hosts opt in with `Config.map_editor`. `EDIT_PASSWORD` takes precedence; otherwise `.edit-password` is read and trimmed or created with six random bytes encoded as twelve hexadecimal characters (`server.js:89-93`). The native implementation hashes both passwords with SHA-256 and compares the fixed-size digests in constant time, matching `server.js:92-93`. Passwords remain private to the server.

Paths, names and responses follow `server.js:95-129,146-164`:

| Request | Response |
| --- | --- |
| `/maps`, any method | JSON array of sorted map keys. |
| Invalid name | 400, text `bad map name`. Names use 1 to 32 lowercase ASCII letters, digits or hyphens; `.json` is optional. |
| `POST /maps/<name>`, missing or incorrect `x-edit-password` | 403, JSON `{ "error": "wrong password" }`. Authentication precedes body parsing. |
| Authenticated body above 8 MiB | 413, JSON `{ "error": "map too large" }`. Incoming bytes count before parsing or projection. |
| Malformed JSON | 400, JSON `{ "error": "bad JSON" }`. |
| Invalid map | 400, JSON `{ "error": <validation error> }`, using the prototype's validation messages. |
| Valid authenticated POST | Writes validated fields to `<name>.json`, refreshes the shared catalogue, then returns 200, JSON `{ "ok": true }`. |
| GET without the header | Public `mapForClient` projection, 200, JSON and `Cache-Control: no-store`. Private mines, triggers and scenarios are omitted. |
| GET with a correct header | Exact raw file bytes, 200 and `Cache-Control: no-store`. |
| GET with an incorrect header | 403, text `wrong password`. |
| Missing map | 404, text `no such map`. |
| Other methods for a named map | 405, text `method not allowed`. |

The field whitelist is `core::map_save::prepare_map_save`, already checked against every authored prototype map. It retains exactly the fields selected by `server.js:104-126`, including nested layer materials, structures, optional scenario data, buildings and timed triggers. Unknown fields are dropped. Runtime map admission remains separate from editor validation. A valid authored scenario can be persisted even while its hosted execution is guarded.

The front and game workers share one locked catalogue. A successful write publishes a new revision only after disk I/O succeeds. Before the next message or maintenance callback, each worker refreshes its spawn counts and Horde map list. This replaces the prototype's private `maps` process notification (`server.js:127`), with the same effect on subsequent lobby messages. Existing running matches retain their original map. A saved match retains that raw map even if an editor later replaces the current file.

Editor World validation follows `shared/sim.js:1139-1164`: it checks region counts, cell membership, integer unique IDs, open objective positions, four-element bounds, unique valid homes and connected ground routes. It does not require numeric ordered bounds, typed optional metadata or signed 32-bit region IDs. Runtime construction retains those stronger typed checks. This editor path constructs no live World state and opens no World rooms.

The prototype's save whitelist omits `world`, even when that field selected World validation. Native POST preserves that exact omission (`server.js:106-126`). A validated World document with zero capture points can therefore be accepted and saved, then fail subsequent map validation after losing its geography. The server retains that file for authenticated raw GET, uses default-map catalogue metadata and refuses strict runtime construction. This is the prototype's editor behavior, not a new World save format. Ordinary shipped maps still validate after a save round trip.

Library hosts can select a persistence directory. A malformed or unreadable optional override no longer stops startup. The server retains readable file bytes for authenticated raw GET. Unreadable files return 404; public GET also returns 404 for malformed JSON. Public projection parses terrain independently of runtime map admission, so a readable World document or zero-point editor result can still be delivered without constructing a match. Invalid map catalogue metadata falls back to the frozen embedded default, following non-strict `loadMap` (`server.js:53-63`); strict native match loading still rejects the invalid override. Saving a replacement refreshes both the raw document and catalogue revision. Embedded default and tutorial assets remain compiled and validated independently. An I/O failure during POST returns 500 with `{ "error": "map not saved" }` and publishes no catalogue revision. That is recovery behavior outside the prototype's listed successful-write and validation responses.

## Horde record file

`Config.horde_records` selects the private store; the executable defaults to `horde-records.json`. Its object keys and values match `server.js:66-86`:

```json
{
  "bastogne|2|standard": {
    "wave": 7,
    "kills": 42,
    "time": 310,
    "names": ["Host", "Friend"]
  }
}
```

The key is `<map>|<defender count>|<army>`. A better run has a higher wave, or the same wave and strictly longer time. Kills do not break ties. A record result contains exactly `wave, kills, time, names, record, best`, with the previous record in `best`, or null. Sandbox runs produce no record, matching `server.js:753`. Missing or malformed files start empty. A write failure retains the in-memory best and reports the failure, matching `server.js:75-77`.

Workers clone one store, so compare-and-write operations share a lock and cannot overwrite another worker's better run or unrelated key. A fresh store restores the exact persisted values. The lobby's existing `hordeBest` field contains the current key's record only in Horde mode; otherwise it is null (`server.js:344`).

Only MODE-55's persistence and lobby lookup are claimed. Hosted Horde remains guarded. The finish hook derives wave, defender kills, rounded simulation seconds and current defender names from existing canonical state, but a hosted Horde completion is not proven by this batch.
