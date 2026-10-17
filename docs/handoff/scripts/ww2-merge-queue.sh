#!/bin/bash
# Merge queue (Julio, 2026-10-17: merge without waiting for Hermes).
# Every 5 minutes: merge each open PR whose checks all passed, that GitHub reports MERGEABLE,
# and whose head does not contain another open PR's head (so stacked PRs merge base-first).
# Pauses while the latest completed main CI run failed. One log line per action.
R=axelmanosalvasmd/ww2-rts-native
G=/home/judiazm/.t3/worktrees/ww2-rts-native/t3-29229094
SKIP=" 199 201 "
while true; do
  git -C $G fetch -q origin 2>/dev/null
  last=$(gh run list --repo $R --branch main --status completed --limit 1 --json conclusion,headSha --jq '.[0]|"\(.conclusion) \(.headSha[0:8])"' 2>/dev/null)
  if [ "${last%% *}" = "failure" ]; then [ "$paused" = "${last#* }" ] || echo "$(date -u +%H:%M) PAUSED: main CI failed at ${last#* }"; paused=${last#* }; sleep 300; continue; fi; paused=
  prs=$(gh pr list --repo $R --limit 100 --json number,headRefOid,mergeable --jq '.[]|"\(.number) \(.headRefOid) \(.mergeable)"' 2>/dev/null)
  [ -z "$prs" ] && { sleep 300; continue; }
  # Free CI runners: cancel runs for heads GitHub reports CONFLICTING (they rerun after a resolver push).
  conf=$(echo "$prs" | awk '$3=="CONFLICTING"{print $2}')
  for row in $(gh run list --repo $R --limit 300 --json databaseId,headSha,status --jq '.[]|select(.status=="queued" or .status=="in_progress")|"\(.databaseId):\(.headSha)"' 2>/dev/null); do echo "$conf" | grep -qx "${row#*:}" && gh run cancel ${row%%:*} --repo $R >/dev/null 2>&1; done
  heads=$(echo "$prs" | awk '{print $1" "$2}')
  merged_any=0
  while read -r n h m; do
    case "$SKIP" in *" $n "*) continue;; esac
    [ "$m" = "MERGEABLE" ] || continue
    git -C $G cat-file -e "$h" 2>/dev/null || git -C $G fetch -q origin "$h" 2>/dev/null
    stacked=0
    while read -r on oh; do
      [ "$on" = "$n" ] && continue
      case "$SKIP" in *" $on "*) continue;; esac
      if git -C $G merge-base --is-ancestor "$oh" "$h" 2>/dev/null; then stacked=1; break; fi
    done <<< "$heads"
    [ $stacked = 1 ] && continue
    ok=$(gh pr view $n --repo $R --json statusCheckRollup --jq '[.statusCheckRollup[]|(.conclusion // .state)] as $c | ($c|length) > 0 and ($c|all(. == "SUCCESS" or . == "SKIPPED" or . == "NEUTRAL"))' 2>/dev/null)
    [ "$ok" = "true" ] || continue
    if gh pr merge $n --repo $R --merge --match-head-commit "$h" >/dev/null 2>&1; then
      echo "$(date -u +%H:%M) MERGED #$n ${h:0:8}"; merged_any=1
    else
      echo "$(date -u +%H:%M) merge failed #$n ${h:0:8}"
    fi
    sleep 20
  done <<< "$prs"
  sleep 300
done
