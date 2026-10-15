# Ten accepted combat components: private integration gates

This is a source-only integration of preserved nine-row candidate `a788b9d6dafdddedac0236e913147f570f180292`, whole CMB-61 status child `89de3410afdb68023d5cce2716400d75cded5b5c`, and fixed prospective #261 integration `5a8ce0e22c5771f3619f7f52fb0705c9afe2fe8a`. First merge a788 with 89de at `f2bf7663`, then normally merge fixed 5a8. The frozen integration commit containing this file has f2bf as first parent and 5a8 as second parent. Actual main at preparation is `54413f87a92408ff1bd269293835b533a606b8fa`; 5a8 is private prospective evidence, not main.

Full independent reports were read: `/tmp/roster16-reviewer2-cmb7c017-final/REVIEW.md` accepts the nine component rows, and `/tmp/review-cmb61-cfc826-whole-final/REVIEW.md` accepts the complete CMB-61 check. Their results belong to their named producing heads. The a788-versus-9e integration history and complete changed paths were inspected. This successor needs its own independent review and fresh hosted CI before Root publication. No public action occurs here.

## Source and row preservation

Both merges conflict only on additive CHANGELOG/DESIGN blocks. Resolve each by retaining both blocks, with no rewritten historical assertion or execution note. Source and test bodies merge without a choice. The final simtest registry is the complete incoming 5a8 registry plus exactly `#[path = "../match_firing_flame.rs"]` and `mod match_firing_flame;`. All incoming movement modules and MODE-15 raw-path assertions remain intact.

Compare every Git leaf entry in the complete three-parent union, including mode, object type and OID. Preserve incoming 5a8 entries except the accepted component deltas, the mechanical registry union, combined tracking documents and this gate documentation. Compare accepted deltas against a788 and 89de without excluding a Rust, harness or fixture path. The aligned cfg(test) hooks, driver, fields, constructors, comparison-only default mapping and consumed-draw handling remain exact incoming objects. PARITY.md, including the complete owner quotation, is the exact incoming object. No shipping provider, runtime, guard, terrain, ABI, input or oracle changes.

All 160 combat acceptance cells remain exact incoming text. Only Rust and Status cells on CMB-34/35/36/37/61/81/105/118/119/120 differ from incoming; their accepted evidence text is inherited unchanged. CMB-105 is selected, CMB-113 is unchanged. The canonical inventory, excluding the separate TOOL proposals and later historical duplicate recaps, is prospective 222/939 plus these ten, or private 232/939. All other 929 canonical rows remain exact. The four vision rows, six NET rows and MODE-15 stay as incoming. No MODE-21 promotion follows from the replay.

The ability and recovery recorders, consumers and authentic 45/24-case captures remain exact a788 objects. Their older 36/23-case objects and serialized prefixes remain exact fixed `396f6d8a` content. The flame recorder, authored inputs, authentic supplemental capture, complete consumer, older ordinary firing proof and unapplied CMB-69 proposal remain unchanged. The five-row source checkpoint `492e8afdf6abee5589f81d9e6524bbfe609009e9` and unexecuted CMB-128 checkpoint `b5db5a0aa7b24df4489f35d637d90d37a2675078` remain separate and absent from this integration.

The static Git comparison covers 1,031 leaf mode/type/OID tuples: 1,010 exact incoming entries, nine exact accepted nine-component entries, six exact accepted flame entries, four merged tracking/row/registry entries and two new gate-documentation entries. Modes are 1,029 regular `100644 blob` entries, one executable `100755 blob` and the unchanged `160000 commit` reference pin. Including 69 directory trees, the recursive inventory has 1,100 entries. No source entry is omitted from the union comparison. Both complete CHANGELOG/DESIGN parent histories remain ordered subsequences; the only newly written design text describes this integration and its pending gates. These are source comparisons, not build or test results.

## Exact pending instances

Names and registration were read from source, not discovered by executing a test binary. There are exactly 12 planned unique instances in three binaries. The same registry function name denotes two distinct instances, one per package. The standalone `r1-differential` binary's copy of its comparator is not selected again.

| Package | Binary / target | Full test name |
| --- | --- | --- |
| ww2-core | it | economy::compiled_catalog_keeps_complete_post_initialization_tables |
| ww2-core | it | startup_tables::complete_node_exports_keep_every_field_number_and_exact_bytes |
| ww2-core | ww2_core / libtest | match_abilities::tests::conquest_abilities_match_original_source_phases |
| ww2-core | ww2_core / libtest | match_recovery::tests::represented_recovery_phases_match_original_source |
| ww2-core | it | every_test_file_is_compiled |
| ww2-simtest | it | match_firing_flame::cmb61_flame_launch_accuracy_ignores_trench_and_garrison |
| ww2-simtest | it | match_firing_flame::cmb61_flame_target_value_has_decisive_one_point_five_preference |
| ww2-simtest | it | every_test_file_is_compiled |
| ww2-core | ww2_core / libtest | aligned_replay::tape_rejects_malformed_inputs_and_records_every_consumption |
| ww2-core | ww2_core / libtest | aligned_replay::original::tests::exact_comparator_retains_first_unit_field_and_signed_zero |
| ww2-core | ww2_core / libtest | aligned_replay::canonical::currency_defaults_are_symmetric_and_preserve_real_differences |
| ww2-core | ww2_core / libtest | aligned_replay::original_r1_aligned_61_checkpoints |

`ww2-core` and `ww2-simtest` both declare `autotests = false` and an explicit `it` target. Core declares library `ww2_core`; the private aligned module is cfg(test) in that library. Flame is registered by the two additive lines in simtest's `it` target. The original four aligned tests are exactly the two top-level aligned functions, the included original comparator regression and the canonical-currency regression.

## Commands proposed for a future Root grant

Run from `rust/`, only after Root assigns the current slot, target and resource scope and the guard confirms the 20 GiB floor before each Cargo invocation. Freeze the clean actual head and tree first. These are pending commands, not a grant or an execution record. A genuine compile or behavior failure stops the batch. Do not regenerate Source, run discovery/list/full/release, or repeat successful instances.

```sh
cargo +1.99.0 fmt --all --check

cargo +1.99.0 nextest run --locked --cargo-profile ci-test \
  -p ww2-core --lib --test it --build-jobs 4 --test-threads 1 \
  -E 'test(=economy::compiled_catalog_keeps_complete_post_initialization_tables) | test(=startup_tables::complete_node_exports_keep_every_field_number_and_exact_bytes) | test(=match_abilities::tests::conquest_abilities_match_original_source_phases) | test(=match_recovery::tests::represented_recovery_phases_match_original_source) | test(=every_test_file_is_compiled)'

cargo +1.99.0 nextest run --locked --cargo-profile ci-test \
  -p ww2-simtest --test it --build-jobs 4 --test-threads 1 \
  -E 'test(=match_firing_flame::cmb61_flame_launch_accuracy_ignores_trench_and_garrison) | test(=match_firing_flame::cmb61_flame_target_value_has_decisive_one_point_five_preference) | test(=every_test_file_is_compiled)'

WW2_ALIGNED_TRACE=/tmp/aligned-replay-d6308bcc/source-trace.json \
WW2_ALIGNED_NATIVE_FRAMES="${CMB_FUTURE_REPORT_DIR:?Root must provide a new private output directory}/native-frames.json" \
WW2_ALIGNED_REPORT="${CMB_FUTURE_REPORT_DIR:?}/native-report.json" \
cargo +1.99.0 test --locked --profile ci-test -p ww2-core --lib \
  --jobs 4 aligned_replay:: -- --include-ignored --nocapture --test-threads=1

cargo +1.99.0 clippy --locked --workspace --all-targets --jobs 4 -- -D warnings
```

The first test command selects five instances, the second three, and the explicit libtest invocation four. Its `--include-ignored` is required for `aligned_replay::original_r1_aligned_61_checkpoints`. Do not reuse S1 from the historical ignore message: Root must grant the actual current slot. The Source trace is read-only; both output paths must be fresh and must never point at an original artifact. Fresh lint provenance must show the owned checkout's packages checked; reused output alone is not a fresh lint result.

The private replay's libtest success does not assert canonical equivalence. Read its actual new report: keep raw MISMATCH 0/61, first tick 0 Fuel absence versus positive zero, separately from canonical MATCH 61/61 with null first divergence and all 600 stepped ticks equal. Verify the complete 1,321 consumed input rows, startup receipt and 15,063 unused values against the unchanged original Source receipt. Keep the four exact comparison-only paths and their mapping counts separate from engine inputs. Confirm original Source inputs/artifacts remain unchanged and preserve raw evidence on the actual producing head. None of those future results is established by this preparation.

## Preparation result and held gates

Current integration execution count is zero: Source/Node, Cargo/rustc, native tests, rustfmt/fmt, Clippy, discovery/list/full/release and benchmarks. The earlier nine-component five passes belong to df39366e; the flame three belong to 25c76d6c; the aligned four and later integration results retain their original producing heads. Do not transfer those passes to this frozen union.

Local ACK of issue #5 through Hermes 162/comment 6196168806: retain exact published-head review and fresh hosted gates, the zero-row replay eligibility list, separate MODE-21 work and unchanged contracts. No public acknowledgment is sent. Root owns the slot grant, fresh independent integration review, hosted CI and publication. No combat PR while another open PR edits combat.md. No slot is acquired or inferred here.
