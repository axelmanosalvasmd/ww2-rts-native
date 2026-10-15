# Unapplied internal paid-bridge provider proposal

Status: UNAPPLIED, uncompiled and unexecuted. This document and its code artifacts do not enable bridge commands. Base reader is 65066904873d94122077b8c10a4c35aea1ba4d91. Its runtime tree remains unchanged. The proposal patch targets that exact base; it is not a grant to apply it or run Source, native, Cargo, formatting or Clippy.

## Ownership declared before edits

MOV_AUDIT owns MOV11/24/27/28/29/33. ENG_AUDIT owns ENG04/13/14/48/66 in the restored roster, and audits the shared FieldWorks provider. The engineering delivery checkout was read at c96143de3c46a8b8c7db8c7a0ebfb50b7e3760bf. Root received the overlap declaration before these proposal files were written. That notice is receipt, not ENG consent or permission to adopt runtime work.

The proposed paths are `core/src/field_works.rs`, `match_engineering.rs`, `world.rs` and a new World child `bridge_field_work.rs`. Their owners cover placement, canonical wallet/job/control state, work timing, materials, terrain and section graphs. `field_work_snapshot.rs`, `field_project_tick.rs`, `field_projects.rs`, command parsing and recipient routing are affected consumers even where the patch proposes no textual edit. Bridge also intersects engineering rows ENG07/15/16/56/57. It remains excluded from `line_fort`, so this is not mass-project support. Existing queued-Dig and non-Conquest admission boundaries stay in place.

No external API, FFI/header, wire schema, recipient privacy policy, World/Classical delivery gate or terrain admission change is adopted here. Any future expansion into those boundaries requires the corresponding owner coordination before adoption. Root must assign the internal provider edit lease and coordinate ENG_AUDIT before applying this proposal.

## Current boundary and pinned transaction

The actual blocker is `field_works.rs:65`: bridge returns UnsupportedKind. `match_commands.rs:524-528` turns that into Command::Unsupported. Simply adding an enum variant does not implement the original transaction.

Pinned Source is e4e579a838aabae3b1d9dadbc17a0a87147bc52d. The complete paths read for this proposal are `shared/sim.js:431`, `:454-469`, `:2533-2564`, `:3185-3202`, `:3260-3277`, `:3635-3651`, `:5386-5413`, `:2650-2662` and `:2680-2744`.

1. Placement checks sight on every in-map cell of the five-cell stamp before reading buildability. It filters to W and then scans the pending cells in their original insertion order, repeatedly admitting cells touching a bank, supported deck or a cell already admitted in this plan. A floating parallel plan can therefore be empty and blocked.
2. An ordinary unqueued Dig selects the first actual eligible builder, checks its wallet, computes the along-approach direction (no trench quarter turn), and prepares a cost-80 job with reach 9. No terrain is built at admission. The existing PreparedStart wallet borrow and Match preparation remain the single payment/control/job/story transaction. A refused ordinary dig retains the existing Source follow-clearing behavior. That does not mean errors permit partial wallet publication.
3. A distant paid crew uses the existing cached approach at reach minus 2 and the existing RequestKind::Dig scheduler. It never receives a synthetic route or teleport. In range, one cell is due every original three seconds, with the existing engineer pace factor. At its due time, a W cell must still touch a bank or supported deck. A changed or unsupported cell is consumed without refund. Finishing the last planned cell clears the real job/activity and tallies built even if that final cell was skipped.
4. A built cell goes through canonical World mutation, then Source registerBridgeSection semantics. Reuse its existing bridge section when present; otherwise consume the canonical Match next ID before searching neighbors, even when the generated ID is unused because the section joins another bridge. Search neighbors up, down, left, right and join the first bridge with fewer than 128 sections, or create one. Set compiled bridge HP/maxHP, wood material, intact state, actual bank anchor and ordered supports; add missing reciprocal supports. Existing section rubble/failure fields survive the Source reset of its selected fields. New sections start with empty rubble. No unit is created.

`bridge_field_work.rs.unapplied` is the proposed World-owned preparation/publication implementation. `provider.patch` connects its supported-plan filter, due-cell commit and canonical ID allocator to FieldWorks and Match. Neither artifact is registered or compiled. The standalone FieldWorks helper deliberately refuses bridge stepping because it has no canonical Match ID owner; it does not invent a local counter. The proposed real Match path passes its existing next_unit_id by exclusive borrow.

## Atomicity and routing effects

Admission retains the existing prepare_start, staged garrison/control/firing/ability cancellation, prepared mpSpent story update and final infallible publication. No second wallet, copied payment, manually installed job or actor exists.

For a due paid bridge cell, the proposal prepares cell-row shape, mutation capacity, journal capacity, section/structure identity, compiled HP and ID capacity before any write. The prepared object exclusively borrows World and the canonical next ID. A failure before commit leaves terrain, IDs, jobs and built story unpublished. The local cloned job may compute its next elapsed/cell state during preparation, but its canonical record is published only after the cell operation completes. The existing Match built-story preparation remains outside and before the work publication. Bridge jobs cannot belong to a mass project.

The commit uses World::set_cell rather than a flags after-image. It preserves the actual flags allocation and existing layered-ground/wear/toughness/mine/fire/soak effects. World mutation alone does not register a bridge section; the following prepared graph publication supplies that missing owner operation. Its final log_cell reproduces Source's second terrain row and terrainVersion advance, without an extra geometry bump. A rebuilt plank uses toughness 1 through the canonical set_cell rule. Shoals and initialTerrain remain the original constructor values.

The real object/flag transitions advance obstruction, infantry, vehicle and naval versions only through the existing World mutation rule. Existing owner observations derive their own remembered changes when refreshed. No Source flags, change lists or versions enter native state. An authoritative query with absent direct/history change inputs must not gain an invented incremental history; its actual version change still invalidates the existing typed cache. The reader's persistent public Pathfinder and actual flags storage remain intact. Historical direct/history cache and fresh-road checks remain separate evidence.

## Review and future proof

Text-only validation may establish patch shape and runtime-tree equality. It cannot establish compilation, atomicity, engine equality or bridge support. No Source or native result is attached to this proposal.

Before runtime adoption, ENG_AUDIT should review the actual ownership/borrow boundary, graph replacement and historical section handling, exact allocation/log order, no-refund skipping and the standalone helper boundary. Root must separately approve the internal runtime scope and exact base. Any external API/wire/privacy consequence needs coordination before adoption.

Future proof must use complete authentic Source before the exact MOV28 consumer, preserve the first genuine failure, and retain actual payment/job/section/next-ID/version/row prefixes. Required provider witnesses include unsupported floating plans, survival of a supported far deck, repair attachment and plank replacement, changed-ground/support skipping with no refund, original three strip payments and all fixed work windows. They must start from genuine constructors and ordinary inputs or the explicitly disclosed original fixture boundary. No direct bridge installation, actor priming or expected-state imports may make the witness pass. These engineering witnesses are a proposed coordination list, not new ENG ownership or an added whole-match gate for MOV28.

MOV28 remains unproved. MOV11 2a13, MOV33 c0, MOV24/27 a545 and reader650 remain frozen. c875's actual 3,400 Source ticks / 3,402 frames / 342 N10 checkpoints remain a historical failed prefix with no engine comparison.
