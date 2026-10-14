# Changelog fragments

Each change records its changelog entry here, in a new file, instead of
editing `CHANGELOG.md`. Separate files mean parallel pull requests never
conflict on the changelog.

- Name the file `YYYY-MM-DD-short-topic.md`, for example
  `2026-10-14-ci-parallel-jobs.md`. The date keeps entries in order; the
  topic keeps names unique.
- Write the bullets exactly as you would have written them under
  **Unreleased**: what changed, in plain words, with numbers when you
  measured something.
- Add new bullets to the same file if a later commit in the same pull request
  changes the entry.
- Merging main into a branch is not a change. It needs no entry.

`scripts/collect-changelog.sh` moves every fragment to the top of the
**Unreleased** section of `CHANGELOG.md` and deletes the fragment files.
The final order is newest on top. Fragments with the same date sort by topic
filename in descending order, not by creation time. Run it in its own commit
when cutting a release or when the folder grows long. This file stays.

Each invocation requires exclusive access to the worktree, fragment paths
and its Git index. Wait for the collector to finish before other Git or
worktree writes, including editing files, staging or committing. The checks
and actual `git rm` are separate operations, not one atomic transaction.
Concurrent changes after validation are unsupported.

Before replacing `CHANGELOG.md`, the script saves its original and assembled
contents plus each fragment's path, contents and original Git index entry in
a private per-worktree journal. If interrupted, or if Git cannot delete the
fragments, fix the Git failure and rerun the script. It finishes that
collection before collecting any new fragments. A retry refuses unrelated
changelog or fragment edits and changed staged blobs or file modes present
when validated, including changes made between retry invocations and entries
with missing worktree files. It preserves the journal until you restore the
recorded contents and index entries. It never removes Git's `index.lock`.
Commit the resulting changelog and staged fragment deletions together.

For local Rust checks after the test-binary consolidation, prefer
`cargo nextest run --locked --workspace --cargo-profile ci-test`
from `rust/`. Plain `cargo test` runs formerly separate test files in one
process; nextest keeps each test in its own process, as CI does.
