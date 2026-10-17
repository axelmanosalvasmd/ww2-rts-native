# MODE-HOSTING ordinary room delivery

Base: #403 at `84bdc0df70136c8500e54c16230bd2dc213429c1` (#395), with
#390 at `32dd5931785f598472b79350d4d1e77f1a013176` merged for Classic snapshots.
Merges after #373. Prototype pin: `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`.
Owner decision M and Axel's confirmation: #5 comments 6217590741 and 6218552386.
This stream acknowledges those decisions locally; it posts no issue reply.

The R1 private two-human opposing-team profile stays in force. The four public
ordinary names now select their canonical constructors and explicit startup
profiles. Default remains the no-mode Conquest alias. No mode guard is removed:
startup, snapshot, recipient-list, result and save/load guards name the supported
ordinary cases. Tick execution and elimination already handle these exact modes;
the lifecycle tests execute those existing branches. Inherited aircraft,
logistics, field-work and transport capability restrictions remain bounded.

## Each-row literal test mapping

| Row | Tests and scope |
| --- | --- |
| MODE-01 | `mode01_mode04_host_conquest_saves_and_result`; `mode01_mode04_host_classic_sudden_death_saves_and_result`; `mode01_mode04_host_annihilation_elimination_saves_and_result`; `mode01_mode04_host_assault_attacker_saves_and_result`; `mode01_mode04_host_assault_defender_timeout_saves_and_result`; `mode01_mode04_host_default_alias_restores_ticks_and_result`; `mode01_mode04_binary_ordinary_modes_start_snapshot_and_restart`. These complete ordinary R1 room admission beyond #403's component projection tests. |
| MODE-04 | The same tests host two human seats, refuse guest Start, publish each seat's Start and snapshot, save/load, advance through a mode result and the complete ending hold. `mode04_map06_non_world_capacity_and_start_readiness_follow_authored_spawns` and `mode04_trusted_start_checks_capacity_and_assault_sides_before_r1_policy` retain #403's all-map capacity and Assault side checks. |

Each in-process lifecycle sends public Start, runs six ordinary ticks, collects
every seat's snapshots and saves through the host command. A bounded canonical
setup then reaches the real mode victory
branch: Conquest VP, Classic sudden death and HQ loss, Annihilation bunker loss,
or Assault bunker loss / defender timeout. Every recipient receives at least
60 terminal snapshots and its result lobby. The host then loads the earlier save,
receives every seat's fresh Start envelope, resumes and runs six more ticks with
every seat's snapshots. Authored house records must remain identical across restore. No fault is accepted. The Default
witness restores the internal alias through the same startup and delivery gates.
`mode01_host_mode_guards_refuse_corrupt_pairs_and_missing_mode_owners` also
requires tick, snapshot, list, save and RoomRuntime admission to refuse
inconsistent mode pairs or missing canonical mode owners. Binary coverage uses the real R1 server and both WebSocket clients for all four
ordinary names, then restarts each and receives both new Start envelopes and
snapshots. Existing negative binary cases retain closed modes and roster limits.

World clauses remain dropped for R1. Tutorial/Sandbox and hosted Horde lifecycle
are owned by separate streams. These tests claim the ordinary hosting contribution
to MODE-01/04, not those streams' evidence or unrestricted room capabilities.
No parity status document is edited.

## Merge record

Merge `34929386` has parents #403 and #390. The sole nonautomatic hunk was
`rust/crates/core/src/recipient_snapshot/public_state.rs`: preserve #403's
`mode_row` with omitted Conquest mode and #390's Classic nodes and initial empty
ghosts. Classic per-team remembered ghosts remain intact immediately below it.
Every other path merged automatically. No inherited test registration was removed.

The default map exposed a shared save decoder omission: its authored structures
use the existing `house` kind (`structures.rs:24-26`), which was absent from the
fixed save vocabulary and the compiled catalog. Add that one canonical value;
unknown save strings remain refused. The hosted tests require nonempty house
records and compare every restored record with the saved records.

## Validation

Execution receipts belong to the producing head recorded in the PR body. Required
checks are focused core/server nextest selections, the real-server ordinary-mode
binary test, `cargo fmt --all --check` and strict workspace all-target Clippy.
Use the shared slot's matching target with MemoryMax=6G, MemorySwapMax=0 and
four jobs. Refresh both crate entry points under the lock before compilation so
another worktree's newer relative-path Cargo dep-info cannot qualify this tree.
