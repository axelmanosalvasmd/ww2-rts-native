#!/usr/bin/env bash
# Move changelog fragments from changelog.d/ into CHANGELOG.md.
#
# Final order is newest on top, with same-date topic filenames descending.
# A private per-worktree journal supports retries after process interruption
# or Git deletion failure. Each invocation requires exclusive access to the
# worktree, fragment paths and Git index. Wait for it to finish before other
# Git or worktree writes. Validation and git rm are separate operations, not
# one atomic transaction. Checks reject changes present when validated or
# made between retries; concurrent writes after validation are unsupported.
# Run from anywhere inside the repository.
set -euo pipefail

root=$(git rev-parse --show-toplevel)
changelog="$root/CHANGELOG.md"
dir="$root/changelog.d"
journal=$(git -C "$root" rev-parse --path-format=absolute --git-path changelog-collection)
work=""
out=""
cleanup() {
  [[ -z "$work" ]] || rm -rf -- "$work"
  [[ -z "$out" ]] || rm -f -- "$out"
  return 0
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

refuse() {
  echo "$*" >&2
  exit 1
}

if [[ ! -d "$journal" ]]; then
  shopt -s nullglob
  fragments=()
  for f in "$dir"/*.md; do
    [[ ${f##*/} == README.md ]] && continue
    fragments+=("$f")
  done

  if ((${#fragments[@]} == 0)); then
    echo "No changelog fragments to collect."
    exit 0
  fi

  grep -qx '## Unreleased' "$changelog" ||
    refuse "CHANGELOG.md has no '## Unreleased' heading."

  # Refuse dirty or untracked fragments before creating a journal or changing
  # CHANGELOG. A later Git failure keeps the journal available for recovery.
  git --literal-pathspecs -C "$root" rm -q --dry-run -- "${fragments[@]}" > /dev/null

  work=$(mktemp -d "${journal}.tmp.XXXXXX")
  cp -p -- "$changelog" "$work/before"
  cp -p -- "$changelog" "$work/after"
  : > "$work/fragments"
  : > "$work/block"
  for ((i = 0; i < ${#fragments[@]}; i++)); do
    printf '%s\0' "${fragments[i]#"$root/"}" >> "$work/fragments"
    cp -p -- "${fragments[i]}" "$work/fragment-$i"
    git --literal-pathspecs -C "$root" ls-files --stage -z -- "${fragments[i]}" > "$work/index-$i"
  done
  for ((i = ${#fragments[@]} - 1; i >= 0; i--)); do
    sed -e '$a\' "$work/fragment-$i" >> "$work/block"
    echo >> "$work/block"
  done

  # Preserve the existing insertion and spacing rules.
  awk '
    BEGIN { block = ARGV[2]; ARGV[2] = "" }
    drop_blank && $0 == "" { drop_blank = 0; next }
    { drop_blank = 0; print }
    !done && $0 == "## Unreleased" {
      print ""
      while ((getline line < block) > 0) print line
      done = 1
      drop_blank = 1
    }
  ' "$work/before" "$work/block" > "$work/after"

  # Publish complete recovery data before replacing CHANGELOG. Concurrent
  # collectors are unsupported; never overwrite an existing journal.
  [[ ! -e "$journal" ]] || refuse "Another changelog collection has started."
  mv -- "$work" "$journal"
  work=""
fi

fragments=()
while IFS= read -r -d '' f; do
  fragments+=("$f")
done < "$journal/fragments"

if cmp -s -- "$changelog" "$journal/before"; then
  replace=true
elif cmp -s -- "$changelog" "$journal/after"; then
  replace=false
else
  refuse "CHANGELOG.md changed during collection; restore the journal's before or after copy before retrying: $journal"
fi

remaining=()
for ((i = 0; i < ${#fragments[@]}; i++)); do
  f="${fragments[i]}"
  [[ -s "$journal/index-$i" ]] || refuse "Journal has no recorded index entry for fragment: $f"
  if [[ -e "$root/$f" || -L "$root/$f" ]]; then
    cmp -s -- "$root/$f" "$journal/fragment-$i" ||
      refuse "Fragment changed during collection; restore it before retrying: $f"
  elif $replace; then
    refuse "Fragment missing before collection; restore it before retrying: $f"
  fi
  if git --literal-pathspecs -C "$root" ls-files --error-unmatch -- "$f" > /dev/null 2>&1; then
    # A missing worktree file does not make a changed staged entry ours.
    git --literal-pathspecs -C "$root" ls-files --stage -z -- "$f" |
      cmp -s -- "$journal/index-$i" - ||
      refuse "Fragment index entry changed during collection; restore its recorded blob and mode before retrying: $f"
    remaining+=("$f")
  elif [[ -e "$root/$f" || -L "$root/$f" ]]; then
    refuse "Fragment is no longer tracked; restore its index entry before retrying: $f"
  fi
done

if ((${#remaining[@]} > 0)); then
  git --literal-pathspecs -C "$root" rm -q --dry-run -- "${remaining[@]}" > /dev/null
fi

if $replace; then
  # Use CHANGELOG's filesystem so replacement is a single rename. The
  # journal remains intact if replacement or subsequent deletion fails.
  out=$(mktemp "$root/.changelog-collection.XXXXXX")
  cp -p -- "$journal/after" "$out"
  mv -- "$out" "$changelog"
  out=""
fi

if ((${#remaining[@]} > 0)); then
  git --literal-pathspecs -C "$root" rm -q -- "${remaining[@]}"
fi
rm -rf -- "$journal"
echo "Collected ${#fragments[@]} fragment(s) into CHANGELOG.md."
