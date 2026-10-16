# Design fragments

Each change records new decisions and measurements here, in a new file,
instead of appending to `DESIGN.md`. Separate files keep parallel pull
requests from conflicting on new sections. Editing existing `DESIGN.md`
text stays allowed.

- Name the file `YYYY-MM-DD-short-topic.md`, for example
  `2026-10-16-design-fragments.md`. The date keeps entries in order; the
  topic keeps names unique.
- Write the section text exactly as it should appear in `DESIGN.md`,
  including its heading, decision, reasons and any measurements.
- Add the fragment in the same commit as the change. Update the same file
  if a later commit in the same pull request changes the decision.
- Merging main into a branch is not a change. It needs no entry.

`scripts/collect-design.sh` appends every fragment to `DESIGN.md` in
ascending filename order and deletes the fragment files. Fragments with
the same date sort by topic filename, not by creation time. Run it in its
own commit when the folder grows long. This file stays.

Each invocation requires exclusive access to the worktree, fragment paths
and its Git index. Wait for the collector to finish before other Git or
worktree writes, including editing files, staging or committing. The checks
and actual `git rm` are separate operations, not one atomic transaction.
Concurrent changes after validation are unsupported.

Before replacing `DESIGN.md`, the script saves its original and assembled
contents plus each fragment's path, contents and original Git index entry in
a private per-worktree journal. If interrupted, or if Git cannot delete the
fragments, fix the Git failure and rerun the script. It finishes that
collection before collecting any new fragments. A retry refuses unrelated
design or fragment edits and changed staged blobs or file modes present
when validated, including changes made between retry invocations and entries
with missing worktree files. It preserves the journal until you restore the
recorded contents and index entries. It never removes Git's `index.lock`.
Commit the resulting design text and staged fragment deletions together.
