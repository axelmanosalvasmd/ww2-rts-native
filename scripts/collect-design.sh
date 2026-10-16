#!/usr/bin/env bash
# Append design fragments from design.d/ to DESIGN.md.
#
# Append in ascending filename order, independent of the caller's locale.
# A private per-worktree journal supports retries after process interruption
# or Git deletion failure. Each invocation requires exclusive access to the
# worktree, fragment paths and Git index. Wait for it to finish before other
# Git or worktree writes. Validation and git rm are separate operations, not
# one atomic transaction. Checks reject changes present when validated or
# made between retries; concurrent writes after validation are unsupported.
# Run from anywhere inside the repository.
set -euo pipefail
export LC_ALL=C

root=$(git rev-parse --show-toplevel)
design="$root/DESIGN.md"
dir="$root/design.d"
journal=$(git -C "$root" rev-parse --path-format=absolute --git-path design-collection)
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
    echo "No design fragments to collect."
    exit 0
  fi

  # Refuse dirty or untracked fragments before creating a journal or changing
  # DESIGN. A later Git failure keeps the journal available for recovery.
  git --literal-pathspecs -C "$root" rm -q --dry-run -- "${fragments[@]}" > /dev/null

  work=$(mktemp -d "${journal}.tmp.XXXXXX")
  cp -p -- "$design" "$work/before"
  cp -p -- "$design" "$work/after"
  : > "$work/fragments"
  for ((i = 0; i < ${#fragments[@]}; i++)); do
    printf '%s\0' "${fragments[i]#"$root/"}" >> "$work/fragments"
    cp -p -- "${fragments[i]}" "$work/fragment-$i"
    git --literal-pathspecs -C "$root" ls-files --stage -z -- "${fragments[i]}" > "$work/index-$i"
  done

  # Keep existing text and separate appended sections with a blank line.
  sed -e '$a\' "$work/before" > "$work/after"
  echo >> "$work/after"
  for ((i = 0; i < ${#fragments[@]}; i++)); do
    sed -e '$a\' "$work/fragment-$i" >> "$work/after"
    echo >> "$work/after"
  done

  # Publish complete recovery data before replacing DESIGN. Concurrent
  # collectors are unsupported; never overwrite an existing journal.
  [[ ! -e "$journal" ]] || refuse "Another design collection has started."
  mv -- "$work" "$journal"
  work=""
fi

fragments=()
while IFS= read -r -d '' f; do
  fragments+=("$f")
done < "$journal/fragments"

if cmp -s -- "$design" "$journal/before"; then
  replace=true
elif cmp -s -- "$design" "$journal/after"; then
  replace=false
else
  refuse "DESIGN.md changed during collection; restore the journal's before or after copy before retrying: $journal"
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
  # Use DESIGN's filesystem so replacement is a single rename. The
  # journal remains intact if replacement or subsequent deletion fails.
  out=$(mktemp "$root/.design-collection.XXXXXX")
  cp -p -- "$journal/after" "$out"
  mv -- "$out" "$design"
  out=""
fi

if ((${#remaining[@]} > 0)); then
  git --literal-pathspecs -C "$root" rm -q -- "${remaining[@]}"
fi
rm -rf -- "$journal"
echo "Collected ${#fragments[@]} fragment(s) into DESIGN.md."
