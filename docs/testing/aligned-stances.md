# Targeted CMB-83 stance diagnostic

Status at this source freeze: execution pending; DIAGNOSTIC_PROPOSED_DEFAULTS, ownerApproval=PENDING, parityEligible=false. Root owns the one general-rule request6196930542. Hermes1656196947312 is a technical acknowledgment, not owner approval. No row status or public action occurs.

Literal CMB-83: stance accepts holdFire/holdPos/autoRetreat with boolean on, otherwise blocked; own units only, otherwise needs; private bits2048/4096/8192/16384 reach only the owner. Source Stances check is test.js1227-1246. Actual Source anchors: declarations1786-1797; stance reads3331-3334; selection3419; dispatch3574-3579; explicit attack3460 onward; reactive Hold Position4090; fire gate5569; owner projection5903 and snapshotFor6011 onward. Native anchors: Command::from_json stance decoder, Match::execute_command own-unit dispatch, existing recipient_snapshot::SnapshotCache/recipient. No new shipping interface is needed.

The complete authored script is [aligned-stances.json](../../harness/scenarios/aligned-stances.json), with a single1,200-tick case and121 N10 checkpoints. It keeps the accepted profile, explicit layout[0,1], finite16,384-draw shared tape and the same genuine constructor. The authored flat map adds one sandbag cell(10,11), next to ordinary actor1 at(21,21). Ordinary Move stages foe4 at(43,21); ordinary Stop ends staging. Neither actors, health, pose, flags, queues nor ownership are primed or overwritten.

At tick1, each owner sets its three troops Hold Position/Hold Fire on, autoRetreat and autocast off. Tick2 moves the foe. Ticks10-18 submit unknown key, numeric/null/string/missing on, foreign selections both directions and legal Hold Position off/on. Ticks20/21 toggle actual autoRetreat true/false. Tick500 stops both actors;502/503 toggle autocast and504/505 autoRetreat while visible. Tick520 enables foe fire while actor holds both stances;600 stops foe fire. Tick620 orders actual Attack despite Hold Fire,701 stops that attack,702 switches Hold Fire off,780 on again,820 switches Hold Position off and821 resumes foe fire. All input objects, exact IDs, sequence/order and expected disposition labels are committed in the scene.

Every command records its exact input, issuer, sequence, label and actual denial/success, plus before/after recipient flags. Source calls original command. Native calls actual Command::from_json then Match::execute_command at the pre-tick boundary. A malformed decoder result is the real Denial; no malformed typed command or synthetic blocked result is invented. Both advance the ordinary tick runner. This check does not claim network queue or server delivery coverage.

The compared fields retain the complete old raw projection: tick/nextId/births/deaths; unit id/owner/kind/x/z/hp/supp; full path/rot/aim/moveSpeed/vx/vz/travelDir/reverse; repath/stuck/still/orders; targetId/attackId/amove/face; holdPos/holdFire/autoRetreat/retreating/cooldown/retarget; player slot/team/mp/mun/fuel/vp/inc, score and points; actual canShootFoe, branch events, moving observations, ordinary retirement and drawCount. New fields are commands with actual before/after recipient flags, and direct/cached recipient views, each with slot and full id/owner/flags rows for every provider-visible actor. Source uses snapshotFor both uncached and snapshotCache paths; native uses fresh-per-recipient and shared SnapshotCache providers. No private flags are reconstructed from raw unit state. Missing snapshot capabilities are not claimed as full recipient-view or network parity.

Required actual branches on each engine: all authored dispositions; five malformed calls blocked; two foreign calls needs; all three legal boolean switches on/off; actual shootable foe while Hold Fire prevents damage; natural incoming damage while Hold Position keeps the actor stationary; explicit Attack damages the foe while Hold Fire remains on; switching Hold Fire off resumes damage; releasing Hold Position under renewed natural fire leads to actual movement. Recipient nonvacuity requires all four private bits observed on owner rows, visible foreign rows whose owner's flags are set and zero leaked private bits, using both Source paths. Every paired raw frame and consumed consumer/index/value must match in diagnostic mode before branch completion counts.

No registry extension is proposed. Existing holdFire/holdPos missing-or-exact-absence-to-false comparison copies remain diagnostic, with exact actor/path counts, Source sim.js3334 expressions and retained present values. Four approved currency mappings remain separate. Unknown fields, present-value/type/zero-bit/nonfinite/order differences fail. No runtime/Source/input/scene repair follows a failure. The first real failure is retained and ends the run.

Freeze and announce the clean source head, then run pinned x64 Node24.21.0 generation once and authentic check once under S1. Freeze new private capture/binding read-only before Cargo. New artifacts are `/tmp/aligned-stances-d6308bcc/`; all old paths stay intact. Currentguard WW2_CARGO_SLOT=S1, CPU0-3,6GiB/swap0/400%, jobs4/threads1 and immediate20GiB floor apply to all authorized stages.

Exact native payloads through `/home/judiazm/.t3/handoffs/ww2-native-cargo-guard.py`:

```sh
cargo +1.99.0 fmt --manifest-path rust/Cargo.toml --all -- --check
cargo +1.99.0 test --manifest-path rust/Cargo.toml --locked --profile ci-test \
  -p ww2-core --lib -j4 \
  aligned_replay::attack_move::stances::stances_match_with_complete_behavior_and_recipient_branches \
  -- --exact --ignored --nocapture --test-threads=1
# Only if the diagnostic scenario passes:
cargo +1.99.0 clippy --manifest-path rust/Cargo.toml --locked --profile ci-test \
  -p ww2-core -p ww2-simtest --all-targets -j4 -- -D warnings
```

Only one new selected test because the registry and its vector are unchanged. No old attack-move/R1/four-test/guard/list/full/release run, worker or public action. Formatting correction may be mechanical only and requires another clean freeze. Slot transfers immediately on process exit plus flock release. Root handles private final-head review and any owner decision/publication; no eligibility pass is inferred.
