# Protocol recorder and browser oracle

This harness verifies the internal Rust recipient decoder against the pinned reference protocol. PR #16 wraps the same decoder with six ABI 2 C calls, approved by both owners for staged adoption. PR merge remains pending. Axel reported separate Linux Editor build, guard, replay and live shadow checks on `03a49ac`; see `docs/contracts/rust-unreal-abi.md` for that evidence and its limits. This harness alone does not establish Unreal integration or packaged-client completion.

The baseline is reference commit `e4e579a838aabae3b1d9dadbc17a0a87147bc52d` and x64 Node 24.21.0. `WW2_REFERENCE` may point at another initialized checkout of that pin. Install its existing Node dependencies before running; the harness resolves `three` and `ws` from the reference checkout.

```sh
node harness/protocol-record.mjs --scenario conquest --ticks 1000 --seed 0x3c1055 --map six-fronts --out harness/out/conquest
node harness/protocol-record.mjs --scenario world --ticks 200 --seed 0x3c1055 --out harness/out/world
node harness/view-check.mjs harness/out/conquest/protocol-conquest-p0.jsonl.gz
node harness/view-check.mjs harness/out/world/protocol-world-p0.jsonl.gz
```

The recorder imports `server.js` unchanged and connects two named human clients through `test-socket.js`'s `memoryConnect`. Each match has two Normal AI seats. Human orders move the starting infantry and request another rifle squad; player 0 also sends a refused command and a `resync` halfway through the run. Every server message to each recipient is recorded in order as one raw JSON object per line. The clients use public fixture tokens from the start; there are no real session tokens to sanitize. Input actions go in a separate file under the output directory.

Only external test seams change: listening and automatic interval ticks are disabled, maps use the same file contents through synchronous cached reads, Math.random uses mulberry32, and Date.now uses a clock advancing 50 ms per tick. Profiling hrtime advances one nanosecond per call, so machine load cannot alter the server's snapshot beat and thereby its AI schedule. World generation uses the same explicit seed. The real server still steps the game, runs AI, filters recipients, and produces every recorded message. No map, unit or privacy rule is patched. Neither a network port nor a background server remains afterward. This is deterministic fixture scheduling, not a performance measurement.

Each output has both `.jsonl` and deterministic `.jsonl.gz` forms. The committed files use gzip because complete World starts and longer Conquest streams exceed 500 KB uncompressed. They retain every raw message, including the full start. Nothing is cut from the map or snapshot payload. `view-check.mjs` also accepts multiple files in sequence for one recipient; use a separate invocation for a different recipient.

The oracle directly imports the browser's `createWorldState` and `createFog`. It runs the unmodified `main.js` cell-application functions with rendering dependencies stubbed and checks the browser's applied character, state, height, layer and discovery fields for received rows. The cross-language parity report compares the separate raw cell-row cache; it does not compare the complete composed terrain. A start resets the browser view even when the match id repeats. A full snapshot replaces the held unit set; it does not reset fog or terrain. Every input line, including ignored lobby/deny/pause messages, produces one report with keys in this order:

`ids, units, seen, explored, cells, terrain, resync`

`ids` are ascending. `units` hashes complete raw held rows in ascending id order. `seen` and `explored` hash the real fog mask bytes. `cells` counts explicitly received cell indices, and `terrain` hashes their latest raw rows in ascending cell-index order. The terrain digest is raw cell-row cache parity, not a hash of the browser's merged map, heights, materials or all placement-query results. `resync` is the held-set mismatch result, independent of browser network-request throttling.

Hashes are FNV-1a 64, lowercase hexadecimal padded to 16 digits. Canonical values encode null as `n`, booleans as `t`/`f`, numbers as `d` plus little-endian float64 (negative zero becomes positive zero), strings as `s` plus UTF-8 plus NUL, arrays as `a` plus children plus `]`, and objects as `o` plus sorted canonical key/value pairs plus `}`. Keys use JavaScript's UTF-16 sort. Mask hashes consume bytes directly.

Each committed stream also has a matching `.expected.jsonl.gz` file. It contains the direct browser oracle's report after every message, so Rust regression tests can compare the full sequence without Node. Regenerate its uncompressed contents with `node harness/view-check.mjs <capture.jsonl.gz>` and gzip with the timestamp omitted.

The committed Conquest streams cover 500 snapshots per recipient, held checks, full resync, fog deltas, changed terrain and natural unit removals. The World streams cover 100 snapshots per recipient, fog changes and new terrain discoveries. These captures do not cover reconnects, every refusal reason, operative messages, save/load, transport compression or close codes. Those remain separate protocol work.
