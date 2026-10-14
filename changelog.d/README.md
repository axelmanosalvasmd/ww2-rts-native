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

`scripts/collect-changelog.sh` moves every fragment, oldest first, to the top
of the **Unreleased** section of `CHANGELOG.md` and deletes the fragment
files. Run it in its own commit when cutting a release or when the folder
grows long. This file stays.
