# IF-11 C presentation queries

IF-11 stacks on #418. ABI 2.2 adds only five functions with the ABI 2.1 JSON-buffer convention. The contract in `docs/contracts/abi-queries.md` lists every signature, input, output, error, unit and pinned source. The major number, formation hash and existing struct layouts/signatures stay unchanged. Axel reviews signatures before Unreal adopts them.

## eachrowliteraltestmapping

IF-11: `if11_all_unit_motion_queries_match_recorded_source_tables`; `if11_recorded_flights_match_source_bits_through_c_entry_points`; `if11_invalid_queries_and_capacity_failures_leave_output_untouched`; `harness/if11-source-check.mjs` through `motion_check.c`.

The first test checks all 47 compiled definitions against the existing pinned Node table fixture, including source fallback profiles for train, lorry and kaiju, null profiles for non-ground units, classification and exact body radii. The second reuses all 10 existing binary64-recorded flight/velocity cases, comparing every returned component's bits after C JSON serialization and parsing. Times include before launch, launch, after launch and late flight. No behavior recording or fixture is added.

The error test covers unknown/malformed types, missing/null flight fields, overflowing calculations, capacity discovery, undersized/invalid output and recovery after rejected queries. The existing ABI test now checks minor 2, retains every layout assertion and checks declaration order through ABI 2.1. Its error loop includes all five additions. The hosted C replay links the five new symbols and checks their parse failures.

The standalone source harness loads the clean reference at `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`. It compares all three motion exports for 47 definitions, 180 generated states for both flight exports and the 10 recorded cases: 511 linked C queries. Generated states vary launch time, query time, gravity and all velocity components. It also exercises the actual client projectile view's trail callback for all 190 generated/recorded entries at the same elapsed time. This checks the duplicated client calculation directly. Timestamp translation isolates the formula; client extrapolation/expiry and Unreal adoption are not claimed. JSON compares signed zeros as equal; every other returned numeric value compares exactly.

These calls are stateless presentation queries. The shared compiled-rule cache is immutable. No match fields, commands, recipient fields or wire shapes are added, so no new lifecycle state requires a full-match witness.

## Qualification

Run Cargo on the Vostro with Rust 1.99, a locked S1/S2/S3 slot and its matching target, `MemoryMax=6G`, `MemorySwapMax=0`, and `-j4`. Release the slot as soon as the checks finish. Require at least 20 GiB free before beginning.

From `rust/`, inside that scope:

```sh
WW2_WRITE_HEADER=1 cargo test --locked -j4 -p ww2-ffi --profile ci-test --test header
cargo fmt --all --check
cargo nextest run --locked -j4 -p ww2-ffi --cargo-profile ci-test
cargo clippy --locked -j4 --workspace --all-targets -- -D warnings
cargo build --locked -j4 -p ww2-ffi --release
```

With the same slot still held, from the repository root, link against the matching release archive and generated header:

```sh
cc -std=c11 -Wall -Wextra -Werror -pedantic \
  -I rust/crates/ffi/include rust/crates/ffi/examples/link_check.c \
  "$CARGO_TARGET_DIR/release/libww2_ffi.a" \
  -lgcc_s -lutil -lrt -lpthread -lm -ldl -lc -o /tmp/if11-link-check
/tmp/if11-link-check rust/crates/core/tests/fixtures/formation.txt
cc -std=c11 -Wall -Wextra -Werror -pedantic \
  -I rust/crates/ffi/include rust/crates/ffi/examples/motion_check.c \
  "$CARGO_TARGET_DIR/release/libww2_ffi.a" \
  -lgcc_s -lutil -lrt -lpthread -lm -ldl -lc -o /tmp/if11-motion-check
node harness/if11-source-check.mjs /tmp/if11-motion-check
```

The PR records executed final-head results. Historical ABI 2.1 platform receipts do not qualify ABI 2.2; hosted checks and signature review remain separate from local Linux qualification.
