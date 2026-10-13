# Gameplay terrain fixtures

These maintained fixtures answer `axel-terrain-fixtures-026` on issue #5. They cover gameplay queries and delivered recipient terrain, not rendered ground or an agreed transport. The terrain contract remains a v0 draft. The approved six-call ABI 2 is unchanged.

## Authored maps

`source-authored/` has one JSON file for each of the 33 maps at reference `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`. The original JavaScript captures contain every cell centre, in row-major order, and 1,000 deterministic points per map. This is 661,799 centres and 33,000 points in total.

Each file declares its columns and units:

- `centres`: `[cell, x, z, rawLevel, levelMetres, heightAtMetres]`.
- `points`: `[x, z, rawLevel, levelMetres, heightAtMetres]`.
- `rawLevel` is the integer returned by Source `levelAt` and native `Game::level_at`. `levelMetres` is that value multiplied by 2.5. `heightAtMetres` is bilinear gameplay height. Source gameplay callers use zero on maps without a height layer.

Coordinates are simulation metres: east is x, south is z, and a cell is 2 metres square. The sampler includes 16 explicit edge, outside-map and centre probes, then 984 point pairs from the documented 32-bit LCG. It does not draw from or reset a match random stream. Two maps have no height layer; 1,442 sampled points distinguish the step height from interpolation, and 2,064 have a negative integer level.

The `map` field is the complete authoritative fixture input, including hidden authoring data. It is test input and must never be used as a client-safe `start.map`. Source files identify their original producer and reference pin. They are not native-produced results.

## World scouting history

`source-world-scout.json` records actual World memory and terrain collectors over a generated scenery map. It preserves all 398 startup actors and records 32 scene/collector actions with 17 read receipts. The history includes shared team knowledge with independent player cursors, leaving a known area, unseen terrain changes, rescouting, mine discovery and end reveal. It has 273,678 delivered rows, including a final full read of all 262,144 cells.

Rows retain the original `[cell, character, integerLevel, packedState, data]` shape and optional-key omissions. Data includes initial terrain, materials, sections, biomes, landmarks, rails and stations when Source supplies them. The fixture interns repeated data objects and stores region labels as runs only to bound file size; its recorder asserts that expanding each receipt exactly reproduces the original complete row list. This encoding is a test artifact, not a wire format.

The scouting replay uses the existing canonical World memory and recipient cursors. It does not implement or claim a whole World tick, physical walking or a new observation owner. Expected delivered rows never initialize the native scene.

## Reproduction and native export

Both recorders require x64 Node `v24.21.0` and a clean tracked reference checkout at the exact pin. Set `WW2_REFERENCE` when the worktree does not initialize its reference submodule:

```sh
node harness/terrain-height-fixture.mjs --check
node harness/terrain-world-scout-fixture.mjs --check
```

The maintained `ww2-simtest` producers call existing core queries and collectors. The authored test compares every centre and point by IEEE number bits; the scouting test compares complete ordered rows, memory versions and omitted keys. Native compilation and execution are pending for this source checkpoint.

After an assigned build lease validates the producing head, build `terrain-contract-export` in release mode and run that exact executable:

```sh
terrain-contract-export --out NEW_DIRECTORY --producing-commit EXACT_COMPILED_SHA --input-root REPOSITORY
```

The exporter embeds its build checkout identity and clean status, then verifies both against the supplied clean producing checkout. An older copied executable cannot label output with a newer commit. It refuses an existing output directory, computes every result before writing, and emits 33 native height JSON files, full World scouting receipts and a manifest. It reads raw map, point and scene inputs; it never copies expected query heights or delivered rows into its native results. Files identify the actual producing commit and reference pin. Source reproduction, native equality, export read-back and host or Unreal adoption are separate evidence gates.
