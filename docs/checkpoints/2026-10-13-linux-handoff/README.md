# Linux development handoff, 2026-10-13

Continue from `codex/linux-handoff-2026-10-13` in
`https://github.com/axelmanosalvasmd/ww2-rts-native`.
This branch contains the consolidated input/table/terrain candidate plus this
handoff. Its runtime source baseline is
`0ee71badc23df7c74c34de968540c631c0cbb0ef`. It is not merged or fully validated.
Julio requested this checkpoint and local cleanup so development can continue
on Linux. Local Mac test jobs are stopped. Do not restart them on the Mac.

## Start here

```sh
git clone --recurse-submodules --branch codex/linux-handoff-2026-10-13 \
  https://github.com/axelmanosalvasmd/ww2-rts-native.git
cd ww2-rts-native
git fetch origin
git status --short --branch
git rev-parse HEAD
git submodule status
```

Read `AGENTS.md`, `README.md`, `DESIGN.md`, `docs/agent-coordination.md`,
`docs/contracts/README.md` and issue #5, including replies after comment
`6157766804`. Read the relevant contract PR replies before interface work.
Every change needs its own CHANGELOG entry in the same commit. Preserve the
read-only reference pin and deterministic core. There is one canonical Match,
command queue and ordered event owner. Use separate worktrees and one owner
for shared tick/interface edits.

Use the Linux machine's configured toolchain. The workspace pins Rust 1.99.0
with rustfmt/Clippy in `rust/rust-toolchain.toml`. Source fixtures were produced
with x64 Node 24.21.0; check architecture/version before regenerating them.
For new-machine or toolchain repair, follow the fleet COMPUTERS.md and
TOOLCHAIN.md instructions available to that machine. No credentials were copied.
The old Mac `/private/tmp/ww2-native-cargo-*` wrappers and lease files are not
Linux dependencies. Run normal Cargo commands from `rust/`.

## Exact source checkpoints

All these branches are pushed. Keep their identities separate from test claims.

| Work | Remote branch | Exact source head | Status at handoff |
| --- | --- | --- | --- |
| Landed baseline | `main` | `92fb7fe66eb7f788c83449ae67223984b969f177` | PR #196 merged; 401 tests passed on all three CI platforms |
| Consolidated input, tables and terrain | `codex/linux-handoff-2026-10-13` | Runtime baseline `0ee71badc23df7c74c34de968540c631c0cbb0ef`; documentation successor contains this file | Independently source-reviewed; expected 410 tests, validation pending; PR #198 is updated to this reviewed checkpoint under Julio's explicit retry approval |
| Ground commands, construction and moved-vehicle contact | `codex/linux-handoff-ground` | `b6cbf406b629e33ea402c4621f43b4b12ac55581` | Fmt/strict Clippy passed; full suite stopped by user after 289 passing execution lines, zero failures so far; suite/FFI incomplete |
| Ordinary research, legacy triggers, Say/falling projection and socket startup | `codex/linux-handoff-legacy` | `158fadbe568a44eda8ebc9584cf5a94a8639e7c4` | Compiles; first research target has 0 passes and 2 failures; later gates unrun |
| World/Ground source union | `codex/linux-handoff-world` | `fca71f0b3bf49828a87874ef2ac2a9896e3f5d3e` | Source-only normal union of `7f9c886` and Ground `b6cbf406`; moved World actors still refused by contact admission |
| Internal table producer, PR #197 | `codex/startup-tables-producer-publication` | `1748508fb342963e60d4979d111de21bfe7ebf83` | Reviewed LF checkout repair; fresh 404-test platform cycle requested |
| Terrain source, PR #200 | `codex/terrain-contract-fixtures-publication` | `88d4005fe21f165076dc62d13d2c15153dd23224` | Producer/fixtures match verified `a81b4f2`; combined 403-test platform proof pending |
| Table transport proposal, draft PR #199 | `codex/startup-tables-transport-publication` | `b35bb4097e6e9166cf5b42f45cacbbd237e62644` | Documentation-only, unagreed and unimplemented |

The original 58-PR queue is fully incorporated: 21 GitHub merges and 37
superseded entries closed only after their exact heads were verified as
ancestors of main. This does not mean the game or all remaining Rust gameplay
is complete. Do not reopen or individually remerge that historical queue.

## Next work

1. Validate the consolidated Linux handoff candidate and inspect PR #198's
   fresh final-head Linux, Windows and macOS checks. Its earlier `abcaef0`
   Windows failure is recorded below. Julio explicitly approved retrying the
   publication; the reviewed integration checkpoint now updates that PR
   branch with an accurate title/body. The consolidated tree preserves all
   providers through normal ancestry and adds
   only reviewed input, table and terrain work to main. Expected inventory is
   410, not a recorded pass. Fix actual failures, then merge under the existing
   project authorization once all gates and review conditions are satisfied.
   Verify #197/#200 incorporation and their exact ancestry after integration;
   do not report a separate GitHub merge if an entry was only closed as included.
2. On the Legacy branch, fix the real research-scene admission failure described
   below, plus the unused `cell` warning. Preserve the footprint guard and real
   Source construction semantics. Then run the research/scenario/recipient/
   socket targets and full gates. The shared Scenario event consumers and
   real El Alamein startup tests are already composed, not missing interfaces.
3. Finish Ground's full optimized workspace and release FFI on Linux. Its
   earlier exact `b45ed53` passed all six focused Ground/construction/contact
   tests. The only b6 source change is module declaration ordering, but that
   does not turn its interrupted whole-suite result into a complete pass.
   Normally integrate with the consolidated current-main baseline afterward.
4. Continue World contact/actor integration on the World branch. The true union
   preserves both parent histories; it has not run tests. Moved World actors
   still meet the ordinary-mode contact refusal. Implement a genuine canonical
   World path and original Source witnesses before removing that guard. The
   frozen World recorder has eight original traces and a ninth authored-wind
   trace, 118 steps and 1,538 phase entries. Constructor RNG correspondence is
   still a gap; never overwrite native wind to manufacture a match.
5. Use issue #1 and PARITY for further unblocked core/server work. Full Human
   behavior/scheduling, broader command families and additional mode/actor
   admission are not complete just because their components exist. Read the
   parity judgment rules before marking rows done.

Research repair, Ground validation and World Source work can proceed in
parallel. Serialize edits and integration at shared Match/tick interfaces.
Keep workers on useful independent tasks and consolidate validation batches.

## Actual failures and incomplete checks

### Research/scenario branch

Exact `4438f9e` initially failed compilation with E0597 in
`legacy_scenario_runtime.rs:150`: per-cell `contact_id` did not outlive the
reused DamageContext. Exact `158fadbe` gives those identical execution/cell
strings stable backing in the original y-then-x order. Independent source
review passed, and subsequent compilation succeeds.

The actual `ordinary_production_tick` run then fails both tests:

- `original_ordinary_research_and_immediate_buy_ticks`, at the first boundary
  of `tier-pauses-without-access-out-but-away-and-postwinner-run`.
- `restored_ordinary_training_queue_is_refused_before_the_clock`, tick zero.

Both report `aircraft admission: Environment(InvalidInput("building footprint"))`.
No later focused test, full suite, strict Clippy or release FFI ran in that cycle.
There is also an unused `cell` warning in `legacy_scenario_runtime.rs:159`.
Inspect actual recorded actors and canonical structure metadata before deciding
the cause. Ground's previous invalid HQ scenes were repaired with genuine
Source `placeBuilding` and matching canonical metadata, not fake footprints.
That is precedent, not proof that the research failure has the same cause.

The planned focused targets are `ordinary_production_tick` (2),
`legacy_scenario_tick` (4), `recipient_legacy_say` (1), core falling (2), and
the actual server startup socket test (1). Enumerate current names in source.
Great Bridge has 84 actual failed sections at zero HP; its selected Source
snapshots have zero visible falling rows for both seats. Separate falling
tests cover visible rows. Preserve that distinction.

### Windows repairs in the consolidated branch

PR #197's old `8bf067d` passes the Rust fixture comparison but fails the second
exact-byte assertion against accepted documentation JSON. Isolated Git with
`core.autocrlf=true` reproduces 30,618 bytes becoming 30,619 through final LF to
CRLF conversion. Exact `1748508` adds a scoped LF attribute for canonical
startup-table documentation JSON. It preserves the accepted SHA256
`5de90a7990ec5e8a2903436ac4809302464be29b546d79591a2adc08c68a2efd` and
does not weaken the test or change the producer.

PR #198's old `abcaef0` has a Windows OS10053 ConnectionAborted failure in the
strict socket reader while the takeover test waits for the old socket. The
log does not prove the receive stage or a production runtime defect. Exact
`22eae725`, included in the consolidated tree, reads and asserts the actual
`replaced` notification before attempting stale bytes, then never reads that
retired socket again. Shared Receiver and direct-Hub stale-authority guards
are unchanged. Fresh platform success remains required.

### Ground stop

Exact `b6cbf406` passed formatting and strict all-target Clippy. Julio's handoff
request interrupted its full optimized run with exit 143 after 289 passing
execution lines and no failure lines. This is STOPPED_BY_USER, not a test
failure or whole-suite pass. Its release FFI build did not run. Both local Mac
lanes were released and the Ground process group was verified gone.

## Axel and contracts

Latest consumed reply is
[axel-root-integration-review-044](https://github.com/axelmanosalvasmd/ww2-rts-native/issues/5#issuecomment-6157766804).
It accepts the internal producer and conditionally has no objection to host
input once checks pass and three questions are answered. DESIGN and the
coordination response explain them:

- Attached player Vec indices remain stable through reconnect/takeover;
  compaction happens after native detachment.
- `held >= 120` is terminal post-win completion, not resumable drop pause.
  New input already refuses after finish, before reservation.
- Mixed eligible unsupported selections cause a temporary whole-command
  service refusal. This is not Source selection semantics. Foreign/missing/
  source-ineligible rows are skipped first; garrison/build restrictions differ
  by command. Do not describe all such stance commands as blanket-refused.

Still pending: NET-58 privacy, spectator Say/starts, operative starts,
owner-only flag-bit visibility, public start/restart, broader Human behavior,
unsupported roles, and modes.md question 7. Publication permission is not
agreement to a new wire contract. Draft PR #199 proposes `tablesVersion:1`
and one `tables` envelope; do not implement/adopt it before explicit agreement.
Axel's Unreal work and deferred FPS/editor scope remain separate.

Reference pin: `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`.
Authoritative six-call ABI 2 came through PR #16, merge `d88ef6d`.
Approved header SHA256:
`bf203f16eca88562cfc6e53efccc48bf260d3a44750d7300f00694723ec69499`.
No Unreal consumer source or fresh live adoption is claimed in this handoff.

## Linux validation

From each exact candidate's `rust/` directory:

```sh
cargo fmt --all --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace --profile ci-test
cargo build --locked -p ww2-ffi --release
cc -std=c11 -Wall -Wextra -Werror -pedantic \
  -I crates/ffi/include crates/ffi/examples/link_check.c \
  target/release/libww2_ffi.a \
  -lgcc_s -lutil -lrt -lpthread -lm -ldl -lc -o target/link_check
./target/link_check crates/core/tests/fixtures/formation.txt
```

Run the meaningful focused failing target first when fixing a failure.
Preserve the actual failure before making a successor. Do not just retry
flaky behavior, waive a failed platform, change `ci-test` optimization/assertions
or add duplicate tests. Fresh final-head CI on all three OS platforms is a
merge requirement. Local Mac wrappers, copied executables and caches can be
deleted after handoff; compile native Linux artifacts from pushed source.

## Durable artifacts and local cleanup

Automatic approval review initially refused updating PR #198 and publishing
diagnostic experiments. Julio then explicitly approved a retry. The reviewed
consolidated checkpoint is published on both the dedicated handoff branch and
PR #198's branch; fresh platform proof remains required. The optional diagnostic
archive is published separately from native source and test evidence. It is
not integrated gameplay code.

[Terrain dataset prerelease](https://github.com/axelmanosalvasmd/ww2-rts-native/releases/tag/terrain-fixtures-a81b4f2)
contains 35 JSON files in a 4,308,253-byte archive. SHA256:
`c1c54b5cdc4f39f49e781778abb712b4d45e810ed749a55f59322cb27ec0f6ca`.
Producing commit stays `a81b4f2b03bfbbe0a19094bfd6e3efa00bf80f1b`.
It covers 33 maps, 661,799 centres, 33,000 points and 273,678 recorded World
cell rows. It proves gameplay queries/collectors, not a whole World tick,
physical walking, rendered terrain or live Unreal adoption.

[Linux handoff prerelease](https://github.com/axelmanosalvasmd/ww2-rts-native/releases/tag/linux-handoff-2026-10-13)
stores a Git bundle of retained Codex branch histories and a compact evidence
archive with logs/manifests, excluding Mac executables and build caches.
The small `evidence/` files beside this document contain the latest failures,
the user-stopped run and source correspondence. The main Mac checkout and
this file are retained. Temporary task worktrees/caches are cleaned only
after remote commits and these archives are verified. Never clean unrelated
projects, shared services or another agent's uncommitted work.

## Vostro destination

The requested checkout is `/projects/ww2-rts-native`; `/projects` aliases the
existing `/home/judiazm/Projects` root. T3 Code project ID is
`8f5da126-1c26-4dae-a35e-10fceadaf6aa`, title `ww2-rts-native`. Supported CLI
registration and the exact active project row were read back. Node 24.21.0 x64
and pinned Rust 1.99.0 are available from a fresh minimal service PATH. The
existing T3 service was preserved and no AI development thread was started.
Verify the current branch, reference submodule and handoff before continuing.
