#!/usr/bin/env bash
# Usage: ww2-merge-when-green.sh <pr> <reviewed-head-sha>
# Waits for every check on the PR to finish. Merges with a merge commit only if
# all checks pass and the head is still the reviewed SHA. Prints one result line.
set -u
pr=$1; head=$2; repo=axelmanosalvasmd/ww2-rts-native
# CI skips docs-only changes, so a PR touching only docs/ and changelog.d/ has no checks.
# Retry until the file list is non-empty: an empty answer (API error) must never count as docs-only.
until files=$(gh pr diff "$pr" --repo "$repo" --name-only 2>/dev/null) && [ -n "$files" ]; do sleep 300; done
docs_only=$(printf '%s\n' "$files" | grep -vcE '^(docs/|changelog\.d/|design\.d/|[^/]+\.md$)')
while true; do
  states=$(gh pr checks "$pr" --repo "$repo" 2>/dev/null | awk -F'\t' '{print $2}')
  if [ "$docs_only" = "0" ] && [ -z "$states" ]; then states=pass; break; fi
  # One API call per poll; an empty answer is a transient API error (e.g. rate limit), not a moved head.
  info=$(gh pr view "$pr" --repo "$repo" --json mergeable,headRefOid --jq '"\(.mergeable) \(.headRefOid)"' 2>/dev/null)
  if [ -z "$info" ]; then sleep 300; continue; fi
  if [ "${info%% *}" = "CONFLICTING" ]; then echo "PR #$pr CONFLICTING with main; not merged"; exit 1; fi
  cur=${info#* }
  if [ "$cur" != "$head" ]; then echo "PR #$pr HEAD MOVED to ${cur:0:8} (reviewed ${head:0:8}); not merged"; exit 1; fi
  if [ -n "$states" ] && ! echo "$states" | grep -q pending; then break; fi
  sleep 150
done
if echo "$states" | grep -qv '^pass$'; then
  echo "PR #$pr CHECKS FAILED: $(gh pr checks "$pr" --repo "$repo" | awk -F'\t' '$2!="pass"{print $1}' | tr '\n' ',')"; exit 1
fi
if gh pr merge "$pr" --repo "$repo" --merge --match-head-commit "$head" >/dev/null 2>&1; then
  echo "PR #$pr MERGED $(gh pr view "$pr" --repo "$repo" --json mergeCommit --jq .mergeCommit.oid | cut -c1-8)"
else
  echo "PR #$pr MERGE FAILED (conflict or protection): $(gh pr view "$pr" --repo "$repo" --json mergeable --jq .mergeable)"; exit 1
fi
