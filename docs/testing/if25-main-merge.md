# IF-25 main integration record

Merge parents: owned `6056545d76073cd70aefbaf5a7fa02b95553f7ec` and frozen main `9e5bf12439cead3ede90fdf7e62b73fcb7ed1881`. Main includes #401, #397, #395, #389 and #379. The exact automatic merge tree is `4a5fbaf7c82e43a9524555cbb18d6affebda8739`. It conflicts in nine files. No other merge parent is inferred from a moving remote ref.

The [complete Rust difference from the automatic conflict tree](if25-main-merge.diff) is generated directly from Git with zero unchanged context after formatting and records every manual source and test change, including conflict-marker removal. The sections below explain each hunk and every additional edit. No parent test or registration is intentionally removed. World admission remains closed. Earlier source qualification at 6056545d is historical and does not qualify this merge.

Parents: 6056545d76073cd70aefbaf5a7fa02b95553f7ec and 9e5bf12439cead3ede90fdf7e62b73fcb7ed1881.

- lib.rs, one conflict: keep numeric_keys plus both incoming field/world policy registrations. Existing private engineering module stays registered.
- match_ground_orders.rs, one conflict: use incoming shared convoy_retreat_home call instead of a duplicate inline scan. Companion nonconflict edit in match_reactions.rs::convoy_retreat_home replaces total_cmp minimum with shared numeric-difference first_by, retaining active/allied/source predicates and source insertion ties.
- match_human_observation.rs::apply_view_rows, one conflict: retain incoming field ground/bridge/owned-mine updates and own observed height/mines/found-mines updates, with only one height assignment. Companion nonconflict complete_ordinary_facts edit fills field.free_nodes from public nodes minus recipient-known claims, then keeps generic mine lists aligned with the incoming canonical recipient mine lists. Retain incoming public-map mine hiding and canonical found_mine list producer.
- server/lobby.rs::start_trusted, one conflict: preserve owned edited-map dispatch and shared MatchOptions. Select ArmySize::from_key(&room.army) from incoming main, keeping incoming named-army behavior for both edited and authored maps. Every other option remains exact to both parents.


# IF-25 AI merge resolution

Parents inspected: HEAD `6056545d76073cd70aefbaf5a7fa02b95553f7ec`; MERGE_HEAD `9e5bf12439cead3ede90fdf7e62b73fcb7ed1881`.

No Cargo, formatter, staging, commit or GitHub write ran in this subagent.

## Every manual conflict hunk

1. `commander_tactics.rs`, State fields: retain HEAD engineering and node assignment memory and incoming field and World memory. Incoming field and World fields receive serde defaults so older private policy saves omit them safely. Preserve HEAD serde derives and its static-string save adapter.
2. `commander_tactics.rs`, rush and construction: retain incoming delivered World rush centroid and endangered-engineer exclusion, plus its separate World construction branch. Ordinary/Classic construction calls HEAD `plan_with_nodes`, with the existing per-seat RNG and persisted node assignments. Keep HEAD ordinary rush escape access to assigned engineers; World retains incoming exclusion.
3. `commander_tactics.rs`, field dispatch: incoming callback policy runs exclusively when a complete `context.field` packet exists. HEAD engineering runs exclusively when it does not. Retain incoming remaining-budget debit, canonical delivered visibility preflight, denied-attempt/approach commands and busy assignments. No observation runs both dispatchers.
4. `commander_tactics.rs`, pre-unit loop: retain HEAD aircraft planner and incoming World-free actor set together.
5. `commander_tactics.rs`, point holder: complete field packets use incoming callback holder and its budget debit, placement feedback and busy marker. Packet-less component inputs retain HEAD point-holder fallback. A holder runs one implementation per look.
6. `commander_tactics.rs`, trailing helpers and test registration: retain HEAD point-holder module, aircraft/enemy-radius helpers, IF25 lifecycle module, and all three incoming field assault/policy/review test registrations.
7. `commander_fortification.rs`, first threat hunk: retain incoming World-region threat branch and ordinary center fallback. Keep HEAD requirement that every visited HQ sorts crew before a tier/completion skip; remove incoming early skip.
8. `commander_fortification.rs`, second threat hunk: ordinary enemy-spawn eligibility retains incoming team, eliminated-seat and hidden-spawn guards. Selection uses HEAD shared stable sorter with numeric distance subtraction and no extra ID fallback. Incoming World comparator remains unchanged and unclaimed by ordinary IF25.
9. `human_commander_policy_tests.rs`, final hunk: retain both HEAD AI66 private-stream/lifecycle and Classic admission tests, plus incoming `commander_field_live_tests.rs` registration. No assertion or test removed.

## Every nonconflict source or test edit by this subagent

- `commander_tactics.rs`, adaptive-available worker filter: preserve the HEAD ordinary/Classic rush engineer escape exception after incoming adaptive busy filtering. All paid combat field workers stay excluded; World keeps its separate endangered-engineer construction exclusion.
- `commander_field_policy.rs`, Sent and State: add serde derives to carry the incoming private memory under the already serializable shared State. Bridge memory changes from a sorted set to an insertion-preserving vector. `field_work` rejects duplicate/out-of-range saved bridge entries, zero saved actor IDs and negative/future saved job ticks before memory mutation.
- `commander_field_policy.rs`, bridge ingestion: sort newly supplied bridge cell indices numerically through the shared sorter, then append previously unseen cells. Earlier remembered order survives subsequent deliveries.
- `commander_field_policy.rs`, `nearest` and bridge selection: replace total-order minima and ID/cell fallbacks with the shared stable numeric comparator and sorted-first selection.
- `commander_field_policy.rs`, fill and demine: build one mutable eligible crew vector per operation and retain each earlier candidate's stable sort for later candidate ties. Preserve incoming eligibility, visibility callback, budget and denial/approach behavior.
- `commander_field_policy.rs`, `holder`: use the shared stable numeric spawn-distance selector and omit hidden enemy spawns. Preserve incoming needs, work, housing, admission feedback and fallback behavior.
- `commander_field_policy.rs`: register new active-consumer child module `commander_field_if25_tests.rs`. The engineering subagent owns its content and records its changes separately.
- `commander_world_policy.rs`, State only: add serde derives required by the serializable shared policy State. No World algorithm or admission change.
- `if25_tactics_tests.rs`, existing holder witness: exercise both the packet-less fallback and the active complete-packet callback implementation for every existing tie and work/housing case. Assert the same first command and preserve all previous assertions.
- `if25_ai_lifecycle_tests.rs`, existing holder lifecycle: use the actual delivered complete field packet and active callback holder, then preserve canonical command admission, actual paid job state and every-seat snapshot/tick assertions.

## Preserved automatic merges inspected

Incoming field and World context facts/defaults remain alongside HEAD node worth, engineering facts and aircraft readiness. Incoming full HQ wall/gate/flank logic and World site bounds remain. HEAD repeated HQ crew sort, tier savings, aircraft readiness and stable attack-target reuse remain. Incoming adaptive busy filtering keeps raids from replacing paid field work. Both ordinary and World component test registrations remain. World room admission is unchanged and excluded from this IF25 claim.

## Qualification

`git diff --check` passed for the resolved/edited files. Parent qualification must use the final merge tree. Earlier 6056545d receipts do not qualify this tree.


# IF-25 elimination merge resolution

Merge parents: HEAD `6056545d76073cd70aefbaf5a7fa02b95553f7ec`; MERGE_HEAD `9e5bf12439cead3ede90fdf7e62b73fcb7ed1881`.

Assigned files: `rust/crates/core/src/elimination_modes.rs` and `rust/crates/core/src/elimination_modes/tests.rs` only. Read the resolving-merge-conflicts skill and the actual conflict stages before editing. No Cargo, formatter, staging, commits or GitHub writes.

Primary intent inspected: incoming commit `397686ada33f2c3963bd3595ebf616531c9a87eb` (Classic node placement and construction endgame), incoming `docs/testing/classic-bases.md`, incoming Classic node/base tests, and the pinned prototype `e4e579a838aabae3b1d9dadbc17a0a87147bc52d` at `shared/sim.js:1012-1018,1688-1722` in the canonical read-only reference checkout.

## Every manual conflict hunk

1. `elimination_modes.rs`, beginning of `setup_classic_nodes` pair setup: retained HEAD's `classic_node_pairs(&self.start.players)` call and home-node loop over player references. The helper uses the actual V8 numeric-difference sorter and preserves first-discovered canonical pairs. Incoming main independently fixed pair insertion order using a vector/set combination; the retained helper preserves that intent and the IF-25 comparator semantics.
2. `elimination_modes.rs`, end of the home-node loop: retained HEAD's empty side of the conflict, removing incoming duplicate inline enemy sorting and pair production. Pair production remains wholly in the retained helper. No second sort or conflicting producer is introduced.
3. `elimination_modes/tests.rs`, final append conflict: retained both HEAD IF-25 direct tests (`if25_elimination_reuses_teammate_sort_order_for_later_equal_distance_transfer` and `if25_classic_node_pairs_keep_enemy_ties_and_first_discovery_order`), then appended incoming `classic_bases_tests` module registration unchanged. A blank line separates the retained blocks.

## Automatically merged parent intents retained

Incoming main's private site-search copy marks cells occupied by live, untransported, non-air, non-structure units as blocked. Both initial and walking-balanced candidate `find_site` calls use that private copy. Walking-distance queries still use canonical world terrain, and the incoming `classic_nodes_tests` module registration remains at file end.

Incoming test-helper visibility changes (`scene`, `ids`, `add` to `pub(super)`) remain, enabling the incoming sibling node tests. Incoming base and node test files are untouched. HEAD's Classic authored-scenario exception, reusable teammate sort order and direct witnesses remain unchanged.

## Nonconflict manual edits

None. Only the three conflict hunks above were edited. No reference, interface, World admission or registration outside the assigned files was changed.

## Source verification

Both assigned files contain no remaining conflict markers. Parent-diff inspection confirms incoming occupancy/layout repairs, both IF-25 tests and both incoming test module registrations survive. `git diff --check` for the two assigned files passes. Execution is parent-owned and pending after the complete merge.


# Engineering merge handoff

Incoming main is 9e5bf12439cead3ede90fdf7e62b73fcb7ed1881.

The parent selected incoming commander_field_policy as the sole ordinary dispatcher when Context.field is present, retaining the earlier commander_engineering provider only as the existing field-absent fallback. Incoming World consumers remain present. ai_consumers owns dispatcher resolution, canonical visibility preflight, field/world serde and active stable-sort implementation.

This agent changed only:

- rust/crates/core/src/commander_engineering_lifecycle_tests.rs: retain actual recipient producer and canonical accepted Dig, but assert Context.field is present and inspect active policy.field bridge/fill/demine state through serde. Keep pending and active policy round trips, real ordinary ticks, completion and every-seat snapshots. Do not delete the field packet or force fallback.
- rust/crates/core/src/commander_field_if25_tests.rs: five direct witnesses for active bridge ingestion/gap ties (45/65), bridge crew ties (72), repeated fill sorts (113), repeated mine sorts (132) and nonfinite found-mine input rejection. Both position permutations force a later equality after an earlier non-tie sort for fill and demine.

ai_consumers will register the new file as commander_field_policy::if25_tests. No edits to the production field-policy file were made by this agent.

No Cargo, formatter, staging or GitHub writes were run. Parent qualification remains required.



## Formatting and documentation

Rustfmt normalized the resolved source layout without changing literals, test names or assertions. The linked automatic-tree difference includes that formatting. Documentation changes add this complete merge record and diff, extend the literal consumer map for the active field dispatcher, record its compatibility choice in the existing engineering design fragment, and add one integration changelog fragment. These documentation edits introduce no runtime behavior.

The active-holder tie assertion uses a borrowed one-element slice, as required by strict Clippy, preserving its expected JSON value without a clone. Final integrated qualification is recorded on the PR at its exact producing head.
