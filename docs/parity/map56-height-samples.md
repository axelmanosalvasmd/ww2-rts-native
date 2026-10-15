# MAP56 height query draft

This source-only draft selects exactly 1,000 query rows from the maintained original-JavaScript terrain captures at reference `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`. The branch starts at fixed native main `446671b977abfa6bf689fcb8853833d36519faab`. MAP-56 retains its incoming status; all acceptance text and status cells remain unchanged. The initial preparation preceded execution; historical results and this coverage correction are attributed below.

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
| Fractional interior samples whose recorded height differs from step height | 13 |

The category `fractional-interior-distinct-from-step` describes the existing predicate: fractional coordinates inside the outer cell centres and a recorded height different from the step height. It does not test geometric slope. Of these 13 samples, authored corner heights vary along both axes in 2, along one axis in 7, and along neither axis in 4. The two-axis samples are crater-field:19 and three-islands:19. The four flat patches retain valid weighted-arithmetic differences, including hill-112:18, whose four levels are [4,4,4,4] and whose interpolated result is one ulp below its step height. No expected value or sample changes.

The remaining 60 queries use the two original maps without a height layer. Their interpolated-height result follows the existing guarded gameplay caller branch, zero. This does not invoke the private Source `heightAt` helper without its required height array.

Source `shared/sim.js:2257-2260` supplies `cellOf`; `2829-2837` supplies `level`, `levelAt` and private `heightAt`. The original pre-structure terrain setup ends at `777`, before the `778` capture marker. The recorder uses the maintained terrain-height recorder's exact early-return hook and exposes the unchanged private height query. It never replaces a query formula. It uses the same constant initialization random input, checks the original draw count, and refuses query randomness.

Native `Game::level_at` in `rust/crates/core/src/game.rs:418` and `terrain::height_at` in `rust/crates/core/src/terrain.rs:211` are the actual providers. The existing `simtest::terrain_fixtures::produce` loads the complete authoritative map, initializes terrain, obtains its query Game, and calls those providers with compiled rules. The new focused test passes only the raw map and decoded coordinate bits to it. It compares every expected column by bits and verifies its original captured row, complete input map, category counts and unique sample identity. The original all-map centre/1,000-point test remains unchanged.

All selected finite negative and outside coordinates are accepted by the existing producer. The step query returns level zero outside the map; interpolation clamps to its existing edge centres. These are original query behaviors, not new admission rules. The producer separately refuses non-finite x or z before calling either query. Six NaN/infinity placements assert its existing exact error, `terrain fixture point must be finite`. They are native harness refusal witnesses, not a cross-language equality claim for non-finite coordinates. Empty or malformed maps, direct raw queries on invalid dimensions, unbounded numeric magnitudes, generated World startup, dynamic terrain, rendered meshes, transport and Unreal consumption are outside this finite selected check.

The initial fixture was selected from existing Source evidence without running Source. Source `--check` later ran once at `8187a261ffd17582a086dd3f7e27a82d6ecdd02b`, exit 0, byte-matching all 1,000 query records. The formatting child `da1d151ad7ea1c3e021a0eb68ac47cf959730d09` passed fmt and the exact three native instances, 3 passed, 0 failed, 170 skipped, in run `76f6c07c-2b02-4800-84ee-ac0aa6d6f1a7`.

The independent review of da1d151a returned REVISE for the overstated two-axis coverage label while accepting the literal 1,000-point comparison. This correction renames only that metadata category and corrects the prose; all predicates, numeric counts, selections, coordinates, maps, input metadata, provenance and 5,000 expected bits are unchanged. The sealed earlier reports, including their original coverage claim, remain historical evidence. No Source, Cargo, formatter or native command ran for this correction, and no execution is attributed to its child head. MAP56 remains unpromoted; separate review and Root publication authorization remain required.
