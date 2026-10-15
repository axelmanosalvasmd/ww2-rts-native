# MAP56 height query draft

This source-only draft selects exactly 1,000 query rows from the maintained original-JavaScript terrain captures at reference `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`. The branch starts at fixed native main `446671b977abfa6bf689fcb8853833d36519faab`. MAP-56 retains its incoming status; all acceptance text and status cells remain unchanged. No Source, Cargo or native execution has qualified this draft.

The finite sample matrix contains 33 complete original map inputs and 5,000 expected binary64 columns: x, z, integer level, step height in metres and interpolated height in metres. Coordinates and expected values are 16-digit hexadecimal IEEE754 bits. Each map retains its complete pinned original JSON text and its SHA256, plus the exact existing Source-capture path, hash and producer identity. Expected outputs do not initialize native terrain.

Selection is fixed: Source capture filenames in ascending order, all first 16 existing boundary points per map, then existing point indices from 16 upward across maps in round-robin order until the total is 1,000. The first ten maps contribute 31 points each; the other 23 contribute 30. No random stream is consumed by selection. Coverage counts overlap:

| Predicate | Queries |
| --- | --- |
| Cell centres | 67 |
| Cell edges | 453 |
| Exact map boundary | 132 |
| Outside the half-open map | 296 |
| Negative coordinates | 66 |
| Negative integer level | 40 |
| Interpolated height differs from step height | 97 |
| Explicit height layer | 940 |
| Interior fractional samples with distinct bilinear height on both axes | 13 |

The remaining 60 queries use the two original maps without a height layer. Their interpolated-height result follows the existing guarded gameplay caller branch, zero. This does not invoke the private Source `heightAt` helper without its required height array.

Source `shared/sim.js:2257-2260` supplies `cellOf`; `2829-2837` supplies `level`, `levelAt` and private `heightAt`. The original pre-structure terrain setup ends at `777`, before the `778` capture marker. The recorder uses the maintained terrain-height recorder's exact early-return hook and exposes the unchanged private height query. It never replaces a query formula. It uses the same constant initialization random input, checks the original draw count, and refuses query randomness.

Native `Game::level_at` in `rust/crates/core/src/game.rs:418` and `terrain::height_at` in `rust/crates/core/src/terrain.rs:211` are the actual providers. The existing `simtest::terrain_fixtures::produce` loads the complete authoritative map, initializes terrain, obtains its query Game, and calls those providers with compiled rules. The new focused test passes only the raw map and decoded coordinate bits to it. It compares every expected column by bits and verifies its original captured row, complete input map, category counts and unique sample identity. The original all-map centre/1,000-point test remains unchanged.

All selected finite negative and outside coordinates are accepted by the existing producer. The step query returns level zero outside the map; interpolation clamps to its existing edge centres. These are original query behaviors, not new admission rules. The producer separately refuses non-finite x or z before calling either query. Six NaN/infinity placements assert its existing exact error, `terrain fixture point must be finite`. They are native harness refusal witnesses, not a cross-language equality claim for non-finite coordinates. Empty or malformed maps, direct raw queries on invalid dimensions, unbounded numeric magnitudes, generated World startup, dynamic terrain, rendered meshes, transport and Unreal consumption are outside this finite selected check.

The initial fixture was selected from existing Source evidence without running Source. A future authorized x64 Node v24.21.0 recorder run must independently recompute these bits from pinned Source and check the complete fixture bytes. The new test and existing terrain-contract test then need a focused run on the actual frozen head, with the registration guard. A separate actual-head review precedes any PR or status proposal. No previous pass is attributed to this draft, and no slot or publication is authorized here.
