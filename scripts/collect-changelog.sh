#!/usr/bin/env bash
# Move changelog fragments from changelog.d/ into CHANGELOG.md.
#
# Fragments are inserted, oldest first by file name, directly under the
# "## Unreleased" heading, then deleted. changelog.d/README.md is kept.
# Run from anywhere inside the repository.
set -euo pipefail

root=$(git rev-parse --show-toplevel)
changelog="$root/CHANGELOG.md"
dir="$root/changelog.d"

shopt -s nullglob
fragments=()
for f in "$dir"/*.md; do
  [[ $(basename "$f") == README.md ]] && continue
  fragments+=("$f")
done

if ((${#fragments[@]} == 0)); then
  echo "No changelog fragments to collect."
  exit 0
fi

grep -qx '## Unreleased' "$changelog" || {
  echo "CHANGELOG.md has no '## Unreleased' heading." >&2
  exit 1
}

# Check that git will remove every fragment before touching CHANGELOG.md, so a
# refusal (for example a fragment with uncommitted edits) leaves nothing half
# collected and a retry cannot add the entries twice.
git -C "$root" rm -q --dry-run -- "${fragments[@]}" > /dev/null

# Newest entries go on top, matching the existing order of CHANGELOG.md.
block=$(mktemp)
trap 'rm -f "$block"' EXIT
for ((i = ${#fragments[@]} - 1; i >= 0; i--)); do
  sed -e '$a\' "${fragments[i]}" >> "$block"
  echo >> "$block"
done

# Print the heading, a blank line, then the fragments (each already ends with
# a blank line). The blank line that followed the heading is dropped so the
# spacing stays single.
out=$(mktemp)
awk -v block="$block" '
  drop_blank && $0 == "" { drop_blank = 0; next }
  { drop_blank = 0; print }
  !done && $0 == "## Unreleased" {
    print ""
    while ((getline line < block) > 0) print line
    done = 1
    drop_blank = 1
  }
' "$changelog" > "$out"
mv "$out" "$changelog"

git -C "$root" rm -q -- "${fragments[@]}"
echo "Collected ${#fragments[@]} fragment(s) into CHANGELOG.md."
