# PR #301 merge record

Reviewed baseline: `e010c1c13e0f1c0b5b156e44f715b3f978c7a234`.
This record covers all five main-integration merges on the PR's first-parent history after that baseline: `e4fe2dfa`, `e86b9f3c`, `07f06fad`, `56a1710d`, and the final main refresh `4de6e424` (main `edeb4ca2`). Incoming main's own merge commits are not resolutions authored on #301.

Each numbered item is one complete unified-3 hunk between `git merge-tree --write-tree <parent1> <parent2>` and the resolved tree. Split marker-removal hunks are listed separately. File:line anchors refer to the committed tree for that merge, including context at the start of a hunk. Main is parent 2; #301 is parent 1. Reasons describe source semantics, not inferred author intent. No test or guard is removed or weakened.

Acknowledged coordination requests `axel-301-runtime-ack-20261016`, `axel-fresh-main-heads-readback-20261016`, and the subsequent request for a hunk-by-hunk readback of the non-registration refresh. This document supplies the requested account. A new owner acknowledgment and exact-head hosted checks remain with the coordinator. No issue post is made. The agreed six-call ABI, table contract, reference pin, public ingress and World server-admission boundaries remain unchanged.

## Full object identities and method

Git version: `2.43.0`. `merge.conflictStyle` is unset. The commands use the original first parent followed by the original second parent. In these historical commits, the first parent is the PR #301 branch and the second parent is incoming main. Conflict marker labels therefore follow PR #301 first, main second.

### Historical merge `e4fe2dfa291ec596fe35805319e34b66a83d1eed`

- PR #301 side, parent 1: `e010c1c13e0f1c0b5b156e44f715b3f978c7a234`.
- Main side, parent 2: `09e03d07dfb7a0458c0d795db1fbb7a9a3912e51`.
- Merge base: `5046ddb6bd604229fa9e459106dfdc31a24ba6a0`.
- Committed tree: `c06c26f514e028501a4b8b2df57fa4d6789f117f`.
- Reconstructed conflict-marker tree: `0091a3f1f28dd1dec63664cf4848e390c9e6d8ec`.
- Exact command: `git merge-tree --write-tree e010c1c13e0f1c0b5b156e44f715b3f978c7a234 09e03d07dfb7a0458c0d795db1fbb7a9a3912e51`.
- Exit code: `1`, meaning conflicts were produced, rather than successful source validation.
- Raw output: `/tmp/fix301-historical-e4fe2dfa-merge-tree.txt`.
- Complete diff: `/tmp/fix301-historical-e4fe2dfa-tree-diff.patch`.
- Zero-context diff: `/tmp/fix301-historical-e4fe2dfa-tree-diff-u0.patch`.
- Complete numbered parent, synthetic and committed sources: `/tmp/fix301-historical-e4fe2dfa-numbered-source.txt`.

### Historical merge `e86b9f3ce60a5fb01d0966ef263d0837e89accf6`

- PR #301 side, parent 1: `e4fe2dfa291ec596fe35805319e34b66a83d1eed`.
- Main side, parent 2: `6ee0ba1122db75b72f6ff41bf6b65560793c3589`.
- Merge base: `09e03d07dfb7a0458c0d795db1fbb7a9a3912e51`.
- Committed tree: `285d683c4b6afac27dee20e56d2f7272e69df577`.
- Reconstructed conflict-marker tree: `931951907217e55536254b5e0880a62096241444`.
- Exact command: `git merge-tree --write-tree e4fe2dfa291ec596fe35805319e34b66a83d1eed 6ee0ba1122db75b72f6ff41bf6b65560793c3589`.
- Exit code: `1`, meaning conflicts were produced, rather than successful source validation.
- Raw output: `/tmp/fix301-historical-e86b9f3c-merge-tree.txt`.
- Complete diff: `/tmp/fix301-historical-e86b9f3c-tree-diff.patch`.
- Zero-context diff: `/tmp/fix301-historical-e86b9f3c-tree-diff-u0.patch`.
- Complete numbered parent, synthetic and committed sources: `/tmp/fix301-historical-e86b9f3c-numbered-source.txt`.

## First merge: every hunk

All file:line references in this section refer to committed merge `e4fe2dfa291ec596fe35805319e34b66a83d1eed`, unless a parent is explicitly named.

| Hunk | Committed file:line | Main side | PR #301 side | Committed resolution | Source-supported reason |
| --- | --- | --- | --- | --- | --- |
| M1-01, `-2047,10 +2047,9` | `rust/crates/server/src/lobby.rs:2049` and `:2053` | Parent 2 `lobby.rs:2049` registers `net83_followup_tests` under `#[cfg(test)]`. | Parent 1 `lobby.rs:2049` registers `mode_runtime_a_tests` under `#[cfg(test)]`. | Keep `net83_followup_tests` at 2049-2051 and append an independent `#[cfg(test)]` plus `mode_runtime_a_tests` at 2053-2055. Remove the conflict markers. | Both test modules remain discoverable, and each declaration retains its own test-only gate. |

There are no synthetic-to-committed differences in other paths in this merge.

## Second merge: every hunk

All file:line references in this section refer to committed merge `e86b9f3ce60a5fb01d0966ef263d0837e89accf6`, unless a parent is explicitly named. Main means `6ee0ba1122db75b72f6ff41bf6b65560793c3589`; PR #301 means `e4fe2dfa291ec596fe35805319e34b66a83d1eed`.

| Hunk | Committed file:line | Main side | PR #301 side | Committed resolution | Source-supported reason |
| --- | --- | --- | --- | --- | --- |
| M2-01, `-35,12 +35,9` | `rust/crates/core/src/lib.rs:38` | Test-only `engineering_runtime_assertion_tests` at main 38-39. | Public `elimination_modes` at PR 38. | Keep public elimination module at 38 and independently gated engineering tests at 39-40. | Both runtime mode code and incoming engineering test registration are retained. |
| M2-02, `-142,16 +142,12` | `rust/crates/core/src/match_aviation.rs:145` | Main 146 accepts matching Default/Conquest, or `horde_spawn && Mode::Horde && Some(ModeKind::Horde) && shared_defenders.is_some()`. | PR 142 accepts `ordinary_mode(g, options.mode)`. | Use `ordinary_mode(...) \|\|` the complete incoming guarded Horde-spawn clause. Spell Horde as `crate::economy::Mode::Horde`. | Preserves PR mode-pair validation and incoming initialized Horde spawn support. The qualified type matches the automatically merged imports, which have no `economy::Mode` import. |
| M2-03, `-233,16 +229,11` | `rust/crates/core/src/match_aviation.rs:232` | Main 233 accepts matching Default/Conquest or matching Horde with shared defenders. | PR 224 accepts `ordinary_mode(&world.game, options.mode)`. | Use `ordinary_mode(...) \|\|` matching Horde with shared defenders; qualify `crate::economy::Mode::Horde`. | Shares air-base lookup across represented ordinary modes while preserving the incoming Horde base path. Unlike the spawn gate, this existing base-query clause does not require a `horde_spawn` argument. |
| M2-04, `-1,8 +1,4` | `rust/crates/core/src/match_end.rs:1` | Main 1 says shared finish/timeline services do not decide each mode's runner admission. | PR 1 lists Conquest, Assault, elimination and World phases. | Take main's complete module comment. | Keeps the distinction between shared services and actual mode-runner admission. |
| M2-05, `-81,11 +77,8` | `rust/crates/core/src/match_end.rs:80` | Main 80-81 includes matching Horde and Tutorial option/Game pairs. | PR has no Horde or Tutorial pair after its Classic pair at 79. | Keep Horde and Tutorial arms at 80-81 with the already-common ordinary and World pairs. | Retains incoming mode-specific finish/story service validation without removing PR modes. |
| M2-06, `-300,11 +293,7` | `rust/crates/core/src/match_end.rs:296` | Main 296 uses `self.options.mode.is_construction()`. | PR 294 uses `matches!(self.options.mode, Mode::World \| Mode::Classic)`. | Take `is_construction()` inside the living-structure-or-bunker count. | Uses the common construction classification in timeline structure counting. This does not change the surrounding positive-HP and structure checks. |
| M2-07, `-91,21 +91,13` | `rust/crates/core/src/match_logistics.rs:94` | Main 94-103 defines initialized no-logistics Horde and accepts Default/Conquest or that Horde; message says ordinary Conquest or no-logistics Horde. | PR 92-93 uses `ordinary_mode(...)`, otherwise the ordinary-Conquest error. | Keep main's whole `horde_spawn` expression and error message. Replace its Default/Conquest-only branch with PR `ordinary_mode(...)` at 98. | Retains strict matching mode pairs for expanded ordinary modes and incoming limited Horde birth reserves. The Horde clause still requires matching canonical tags, shared defenders and logistics disabled. |
| M2-08, `-360,16 +352,8` | `rust/crates/core/src/match_logistics.rs:355` | Main 358 has parameterless `validate_supply_context(&self)`; it checks Default/Conquest only. Main 352 and 355 call it without a boolean. | PR 348 takes `garrisons: bool` and checks `ordinary_mode(...)`; PR callers pass false/true. | Keep the incoming parameterless signature and auto-merged parameterless calls at 350/353. Retain the PR `ordinary_mode(...)` predicate at 356. | Avoids a signature/caller mismatch while preserving expanded matching-mode admission. Incoming main removed the boolean-based distinction in the body; taking PR's signature would leave an unused parameter and fail the merged callers. |
| M2-09, `-128,13 +128,8` | `rust/crates/core/src/match_recruitment.rs:131` | Main 131-132 rejects `game.mode.is_some() && !horde` and `Mode::Horde && !horde`. | PR 125-126 rejects `checked_game_mode(...).is_err()` and all construction modes. | Reject `(checked_game_mode(...).is_err() && !horde) \|\| options.mode.is_construction()`. | Permits the initialized incoming Horde exception while retaining PR canonical-pair validation for ordinary modes and the construction-mode exclusion. |
| M2-10, `-94,11 +94,8` | `rust/crates/core/src/match_runtime.rs:97` | Main `StartupProfile` at 97 adds `Horde`. | PR `StartupProfile` at 96 adds `Elimination`. | Keep both variants at 97-98. | Both incoming Horde startup and PR elimination startup need their profile variant. |
| M2-11, `-421,12 +418,9` | `rust/crates/core/src/match_runtime.rs:421` | Main 400 keeps a map objective when no defender-team/Horde restriction applies, or its MP income is positive: `(defender_team.is_none() && !horde_mode) \|\| p.mp > 0.0`. | PR 381 restricts kept map objectives for defender-team or Annihilation modes: `!(defender_team.is_some() \|\| options.mode == Mode::Annihilation) \|\| p.mp > 0.0`. | Combine restrictions as `!(defender_team.is_some() \|\| options.mode == Mode::Annihilation \|\| horde_mode) \|\| p.mp > 0.0`, then `.then_some(i)`. | Keeps only positive-MP map objectives for Assault, Annihilation and Horde, retaining every objective in other modes. The `kept` indices then construct the objective vector and remap prerequisite links. |
| M2-12, `-647,11 +641,8` | `rust/crates/core/src/match_runtime.rs:644` | Main 621 adds `pub horde: Option<HordeState>`. | PR 591 adds `pub elimination: Option<EliminationState>`. | Keep both fields at 644-645. | Both mode states remain owned by `Match`. |
| M2-13, `-831,11 +822,8` | `rust/crates/core/src/match_runtime.rs:825` | Main 801 initializes `horde: None`. | PR 762 initializes `elimination: None`. | Initialize both at 825-826. | The ordinary constructor initializes both mode-specific optional states. |
| M2-14, `-346,15 +346,10` | `rust/crates/core/src/match_story.rs:349` | Main 349-352 maps Annihilation, Classic, Horde and Tutorial to their names. | PR 349-350 maps Classic and Annihilation in the opposite order. | Take all four main arms in main order at 349-352. | Retains PR names and incoming Horde/Tutorial report names. Arm order has no effect for these distinct enum variants. |
| M2-15, `-301,11 +301,8` | `rust/crates/core/src/match_world.rs:304` | Main 304 initializes `horde: None`. | PR 304 initializes `elimination: None`. | Keep both at 304-305. | The World constructor must initialize both optional fields added to `Match`. |
| M2-16, `-558,15 +558,10` | `rust/crates/core/src/production.rs:561` | Main 561-563 documents instant recruitment, scripted Horde seat/shared facilities and Match-owned admission. | PR 561-563 documents skirmish recruitment, absent Conquest object, construction queues and Match-owned admission. | Write combined four-line comment at 561-564: instant skirmish recruitment; absent Conquest mode object; construction queues; Horde scripted seat/shared facilities; Match admission. | Documents both supported families rather than dropping one parent's qualification. This wording is newly combined, not copied verbatim from a single parent. |
| M2-17, `-578,22 +573,17` | `rust/crates/core/src/production.rs:576` | Main 575-581 defines initialized Horde by the excluded final slot, accepts Default/Conquest or that Horde, and reports `recruitment requires Conquest or initialized Horde`. | PR 575-580 accepts Default/Conquest/Assault/Annihilation and reports `recruitment requires a represented skirmish mode`. | Preserve main's complete `horde` computation at 576-579. Accept PR's four-mode match or `horde` at 580-584. Combine the error into `recruitment requires a represented skirmish mode or initialized Horde` at 586. | Keeps each parent's supported recruitment family and describes the combined gate exactly. |
| M2-18, `-67,11 +67,9` | `rust/crates/core/tests/it/main.rs:71` | Main 70 onward registers four movement modules: aircraft controls, aircraft immunity, hull clearance and wreck journey. | PR 69-70 registers `mode_runtime_world` after `mode_point_services`. | Keep World mode tests at 71-72 followed by incoming movement registrations at 73 onward. | Both sets of integration tests stay registered. This hunk removes the opening and divider markers and retains the PR blank line. |
| M2-19, `-80,7 +78,6` | `rust/crates/core/tests/it/main.rs:79` | Main ends incoming movement additions with `mov_wreck_journey` at 76-77. | PR proceeds from its World mode tests to the pre-existing objectives module and has none of these movement additions. | Remove the closing conflict marker after `mod mov_wreck_journey` at 80. | Completes the same registration union as M2-18. It is a separate raw diff hunk and is counted separately. |
| M2-20, `-2227,10 +2227,9` | `rust/crates/server/src/lobby.rs:2229` | Main 2229-2231 registers `net_runtime2` from `net_runtime2_tests.rs` under `#[cfg(test)]`. | PR 2053-2055 registers `mode_runtime_a_tests` under `#[cfg(test)]`. | Keep mode tests at 2229-2231, then add a blank line and independent `#[cfg(test)]` before incoming network module at 2233-2235. | Retains both test-only module registrations. |

## Extra-edit accounting

The complete tree comparison finds no extra changed path outside the 11 conflict paths in the second merge and the one conflict path in the first. Every zero-context changed span is inside a synthetic conflict block or replaces its marker line. This means there are no separate nonconflict hunks omitted from the tables.

The resolutions do include substantive edits beyond choosing a complete side or concatenating alternatives. They are not hidden by this classification:

- Both aviation hunks substitute `ordinary_mode(...)` for main's Default/Conquest clause and qualify the two incoming Horde references as `crate::economy::Mode::Horde`. The merged imports no longer contain `economy::Mode`.
- Birth reserves substitute the canonical `ordinary_mode(...)` branch while retaining the full no-logistics Horde exception and main's error wording.
- Supply validation combines main's parameterless function signature with PR's `ordinary_mode(...)` predicate. Its callers are already parameterless in the synthetic automatic union, so no separate call-site diff exists.
- Recruitment combines `checked_game_mode(...).is_err() && !horde` with the PR construction exclusion.
- Map-objective pruning adds `horde_mode` to the PR defender-team/Annihilation restriction, so Assault, Annihilation and Horde retain only positive-MP objectives.
- Production combines both documentation texts, both supported-mode families and both error descriptions.
- Each lobby conflict adds a second independent `#[cfg(test)]` so the retained incoming and PR test modules are both test-only.

These are merge-resolution edits, including edits necessary because of cleanly merged surrounding context. They are not runtime proof and do not establish that either merge passed tests.

## Five rewritten tests at `32da51ca32caa0b44f486a385cf95114c08587eb`

Its sole parent is `e86b9f3ce60a5fb01d0966ef263d0837e89accf6`. The five Rust files below and one changelog fragment are its complete changed path list. There are no runtime-file changes in this commit. Full before/after source is saved as `/tmp/fix301-historical-five-tests-source.txt`. Its changelog fragment explicitly names all five changed tests.

### 1. Ability fixture mismatch refusal

`rust/crates/core/src/match_abilities/tests.rs:505`, `conquest_abilities_match_original_source_phases`. Before line 539:

```rust
assert!(error.contains("Conquest"), "{name} mode rejection: {error}");
```

After lines 539-543:

```rust
// Only options changed; the canonical Game still has Conquest mode.
assert_eq!(
    error, "actor mode does not match canonical Game mode",
    "{name} mode rejection"
);
```

The fixture still changes only `m.options.mode = Mode::Classic` at 529, still calls `execute_command(...).unwrap_err()`, and still requires the complete local `snapshot` to equal its `before` value at 544. It increments `unsupported` and skips the case as before. This strengthens the checked diagnostic from a substring to exact canonical-mode mismatch; it does not prove valid matching Classic abilities. At this head, `match_actor.rs:26-42` accepts Classic only with `Some(ModeKind::Classic)` and emits the exact mismatch string. `match_abilities.rs:166` invokes actor-mode validation.

### 2. Firing fixture mismatch refusal

`rust/crates/simtest/tests/match_firing.rs:119`, `unsupported_modes_and_later_bad_payloads_leave_firing_unchanged`. Before lines 198-201:

```rust
assert!(
    error.contains("ordinary Conquest mode"),
    "{mode:?}/{kind:?}: {error}"
);
```

After lines 198-201:

```rust
assert_eq!(
    error, "actor mode does not match canonical Game mode",
    "{mode:?}/{kind:?}"
);
```

The loop still exercises seven options modes with `None` canonical mode and six canonical mode kinds each paired with both Default and Conquest, for 19 rejected pairs. Its ten calls remain `capture_firing_prior`, `begin_firing_tick`, `advance_firing_timers`, `apply_unit_firing`, `can_shoot_target`, `step_firing_flights`, `advance_firing_salvos`, `advance_firing_grenades`, `launch_ability_flight` and `launch_weapon_salvo`; every call still requires `unwrap_err()`. The firing/combat/logistics/commands/RNG and formatted canonical Game equality checks remain at 203-208. The malformed owner set at 155 also remains, so these checks preserve mode-refusal precedence over later invalid payloads. This strengthens the exact diagnostic while preserving refusal and mutation protection. It does not test any newly valid matching mode pair.

### 3. Component production refusal

`rust/crates/simtest/tests/match_production.rs:873`, `ordinary_training_refusal_precedes_research_progress`. Before lines 882-887:

```rust
assert_eq!(
    m.validate_production_phases(0.05),
    Err(ProductionStepError::InvalidState(
        "nonempty Conquest training queue"
    ))
);
```

After lines 882-887:

```rust
assert_eq!(
    m.validate_production_phases(0.05),
    Err(ProductionStepError::InvalidState(
        "nonempty skirmish training queue"
    ))
);
```

The prepared research request, paid barracks training row and final `assert_eq!(unchanged(&m), before)` at 888 stay exact. Only the expected error payload changes. `match_production.rs:193-197` at this head emits the new string for a nonempty queue outside construction modes.

### 4. Tick production refusal and recovery

`rust/crates/simtest/tests/ordinary_production_tick.rs:411`, `restored_ordinary_training_queue_is_refused_before_the_clock`. Before lines 437-440:

```rust
assert_eq!(
    failure.reason,
    "production admission: InvalidState(\"nonempty Conquest training queue\")"
);
```

After lines 437-440:

```rust
assert_eq!(
    failure.reason,
    "production admission: InvalidState(\"nonempty skirmish training queue\")"
);
```

Every surrounding predicate remains exact: source fixture TypeError containing `suddenDeath`; source tick 0 to 1; the pending Unsupported command; native tick 0 refusal; no events and no latched tick fault; identical projection, command queue and RNG; then clear the restored training queue and successfully advance once, yielding the expected tick, command key, `Denial::Unsupported`, empty pending queue and next command tick. This is a diagnostic rename, not a weakening of the refusal, atomicity or recovery checks.

### 5. World fort capture changes from refusal to exact success

`rust/crates/simtest/tests/world_finish.rs:54`, `world_finish_uses_actual_constructed_match_and_original_phase_order`. Before line 88:

```rust
if case["supported"] == true {
```

After lines 88-91:

```rust
// The fixture's old native admission label predates fort capture support.
let fort_capture =
    case["name"] == "winning capture needing fort transfer remains refusal";
if case["supported"] == true || fort_capture {
```

The named fixture has `supported: false` and one step at tick 200. Before the rewrite it ran these exact refusal assertions:

```rust
assert!(result.unwrap_err().starts_with("unsupported World"));
assert_eq!(snapshot(&m), before);
assert_ne!(step["before"], step["after"]);
```

After the rewrite the same case instead runs the existing success assertions, now at lines 92-98:

```rust
result.unwrap();
assert_eq!(snapshot(&m), step["after"], "{} after", case["name"]);
if let Some(report) = m.match_result().unwrap() {
    assert_eq!(report.mode, "world");
    assert_eq!(report.win_vp, None);
    assert_eq!(report.names.len(), 3);
    assert!(m.world.game.reveal);
}
```

The full `snapshot` equality covers `regions`, `players`, `units`, `story`, `finish`, `timeline`, `reveal` and `report`, using encoded numeric bits. The fixture's two unit rows change from unit 2 owner -1 to owner 0 while unit 1 remains owner 0. The saved `after.finish` has winner 5 and reason `world`. The fixture itself is unchanged. This is a substantive expectation change to successful fort capture, rather than only an error rename. It checks the existing snapshot projection, which does not include grid membership, footprint owners, per-unit subsystem metadata or recipient delivery. Independent tests must establish those facts.

The other unsupported cases retain refusal/no-change assertions. All cases still check their exact `before` snapshot, reject Conquest victory/sample/result helpers, retain sorted projection and finish with 15 cases and 16 phases. This test calls the World capture/income component directly, so it does not prove ordinary full World ticking or a recipient snapshot.

## Third merge `07f06fadf6a03930685cd94ee5bfca6f086eb7c9`: every hunk

Committed tree: `e612c5862991d4aa8f2d2d9b2981638574cee3a3`.

- Parent 1 (#301): `32da51ca32caa0b44f486a385cf95114c08587eb`.
- Parent 2 (merged main): `b6d05874078f183fc95421cd25e13762b253c57c`.
- Reconstructed conflict-marker tree: `579e200ab9caf2bccb3888a855d0df9826b092ef`.
- Exact command: `git merge-tree --write-tree 32da51ca32caa0b44f486a385cf95114c08587eb b6d05874078f183fc95421cd25e13762b253c57c`.
- The command exits 1 and reports conflicts in 20 paths. The final comparison also includes clean-merge repairs: the construction import, production cancellation context/duplicate, and lobby Connection fixture. These repairs are included below rather than treated as automatic.
- Reproduce the table's hunk boundaries with `git diff --no-ext-diff --no-renames --full-index --unified=3 579e200ab9caf2bccb3888a855d0df9826b092ef 07f06fadf6a03930685cd94ee5bfca6f086eb7c9 -- . ':(exclude)docs/testing/fix301-merge-record.md' ':(exclude)changelog.d/2026-10-16-fix301-merge-record.md'`.

| Hunk | Committed file:line | Main side | PR #301 side | Resolution | Why |
| --- | --- | --- | --- | --- | --- |
| M3-01, `-22,7 +22,6` | `DESIGN.md:22` | New scenario/refund and map-authoring decisions. | Existing MODE runtime A decision. | Retain both sections; remove opening marker. | Preserve both documented scopes. |
| M3-02, `-37,7 +36,7` | `DESIGN.md:36` | Scenario/refund section follows this divider. | MODE runtime A section precedes it. | Replace divider with whitespace. | Keep complete independent decisions. |
| M3-03, `-51,7 +50,6` | `DESIGN.md:50` | Map-authoring section ends here. | MODE runtime A section is before it. | Remove closing marker. | Complete the documentation union. |
| M3-04, `-129,13 +129,8` | `rust/crates/core/src/field_project_tick.rs:129` | Tutorial-aware ordinary_ground_mode predicate. | Expanded ordinary_mode predicate and matching-mode diagnostic. | Use shared union predicate and PR diagnostic. | All represented ordinary modes retain mode-pair admission. |
| M3-05, `-156,13 +156,10` | `rust/crates/core/src/match_abilities.rs:156` | Tutorial capability. | Assault, Annihilation and Classic capabilities. | Keep both alternatives before existing actor validation. | Retain both runtime families without admitting mismatched Game tags. |
| M3-06, `-22,39 +22,26` | `rust/crates/core/src/match_actor.rs:22` | ordinary_ground_mode accepts matching Default/Conquest and Tutorial; checked_game_mode also accepts World. | ordinary_mode accepts matching Default/Conquest, Assault, Annihilation and Classic; checked_game_mode also accepts World. | Use ordinary_ground_mode for the canonical union, plus World only in checked_game_mode. | One shared strict tuple predicate retains every parent mode and rejects mismatches. |
| M3-07, `-142,11 +142,7` | `rust/crates/core/src/match_aviation.rs:142` | Tutorial-aware ordinary_ground_mode. | Expanded ordinary_mode plus guarded Horde spawning. | Use shared union predicate; retain full Horde-spawn clause. | Preserve ordinary aircraft and initialized Horde spawning. |
| M3-08, `-233,13 +229,8` | `rust/crates/core/src/match_aviation.rs:229` | Tutorial-aware ordinary_ground_mode plus initialized Horde base lookup. | Expanded ordinary_mode plus initialized Horde base lookup. | Use shared union; qualify Horde as crate::economy::Mode::Horde. | Retain both families and match merged imports. |
| M3-09, `-51,13 +51,8` | `rust/crates/core/src/match_carriers.rs:51` | Tutorial-aware shared guard and Conquest diagnostic. | Expanded ordinary guard and matching-mode diagnostic. | Use shared union and PR diagnostic. | Keep mode-pair validation before carrier state checks. |
| M3-10, `-1,6 +1,7` | `rust/crates/core/src/match_construction.rs:1` | No Mode import after main switched to the shared predicate. | Mode import required by explicit World exclusion. | Restore economy::Mode import. | The combined runtime guard still names Mode::World. |
| M3-11, `-265,12 +266,8` | `rust/crates/core/src/match_construction.rs:266` | Tutorial-aware shared predicate before tick checks. | Actor validation, World exclusion, and tick checks. | Keep actor validation and explicit World exclusion. | Actor validation includes Tutorial runtime checks and all expanded ordinary pairs. |
| M3-12, `-327,11 +324,7` | `rust/crates/core/src/match_construction.rs:324` | Tutorial-aware shared worker predicate. | Actor validation plus World exclusion. | Keep actor/runtime validation and World exclusion. | Preserve strict worker admission for both families. |
| M3-13, `-418,11 +411,7` | `rust/crates/core/src/match_construction.rs:411` | validate_ordinary_finish_state before finishing crews. | validate_match_finish_state before finishing crews. | Keep expanded ordinary finish validation. | Retain incoming Tutorial invariants while accepting Classic. |
| M3-14, `-569,11 +558,7` | `rust/crates/core/src/match_construction.rs:558` | validate_ordinary_finish_state before command selection. | validate_match_finish_state before command selection. | Keep expanded ordinary finish validation. | Selection retains incoming Tutorial invariants and PR modes. |
| M3-15, `-24,15 +24,9` | `rust/crates/core/src/match_engineering.rs:24` | pub(super) validator with Tutorial-aware shared predicate. | Private validator with expanded ordinary predicate. | Keep pub(super), shared union predicate, and matching-mode diagnostic. | Incoming cross-module callers and all ordinary modes remain supported. |
| M3-16, `-335,24 +335,7` | `rust/crates/core/src/match_firing.rs:335` | Explicit provider tuple guard before actor/runtime validation. | Actor validation supports expanded canonical mode pairs and exact mismatch diagnostic. | Call checked_game_mode before validate_actor_mode. | Keep provider-mode preflight before actor binding and the exact mismatch error required by 32da51ca. |
| M3-17, `-14,13 +14,8` | `rust/crates/core/src/match_ground_orders.rs:14` | Tutorial-aware shared predicate. | Expanded ordinary predicate and matching-mode error. | Use shared union predicate and PR error. | Retain strict command mode admission. |
| M3-18, `-109,12 +109,8` | `rust/crates/core/src/match_logistics.rs:109` | Tutorial-aware shared predicate plus guarded no-logistics Horde births. | Expanded ordinary predicate plus same Horde exception. | Use shared union and retain the entire Horde exception. | No logistics admission is opened. |
| M3-19, `-372,11 +368,7` | `rust/crates/core/src/match_logistics.rs:368` | Tutorial-aware supply predicate. | Expanded ordinary supply predicate. | Use shared union predicate. | Preserve supported supply adapters for both runtime families. |
| M3-20, `-118,13 +118,10` | `rust/crates/core/src/match_production.rs:118` | Validated Tutorial state as production context. | Canonical Assault, Annihilation and Classic production contexts. | Keep all four arms. | All parent runtime contexts retain their validation. |
| M3-21, `-338,6 +335,7` | `rust/crates/core/src/match_production.rs:335` | Transactional cancel_training context has no sudden-death argument. | Context helper requires elimination_sudden_death. | Pass canonical sudden-death state into incoming cancellation implementation. | Preserve Classic policy and main transaction/story semantics. |
| M3-22, `-403,37 +401,6` | `rust/crates/core/src/match_production.rs:401` | cancel_training clones queue/wallet, prepares negative MpSpent, then commits all owners. | Second cancel_training directly refunds queue/wallet with sudden-death context. | Remove duplicate method; retain the transactional implementation with sudden-death input. | One implementation preserves both guards, refunds and story updates. |
| M3-23, `-246,11 +246,7` | `rust/crates/core/src/match_projects.rs:246` | Tutorial-aware shared predicate. | Expanded ordinary predicate. | Use shared union predicate. | Keep unsupported-mode denial before project work. |
| M3-24, `-652,23 +652,11` | `rust/crates/core/src/match_recovery.rs:652` | Tutorial home-spawn recovery, allied World producer recovery, and allied depots. | Allied producer recovery for both construction modes, plus allied depots. | Combine Tutorial spawn clause with is_construction allied producer predicate. | Preserve Tutorial healing and allied Classic/World recovery. |
| M3-25, `-677,7 +665,6` | `rust/crates/core/src/match_recovery.rs:665` | Closing marker after the depot clause. | Same allied depot behavior. | Remove closing marker, retaining complete depot branch. | Complete the same recovery union without changing depot checks. |
| M3-26, `-128,12 +128,8` | `rust/crates/core/src/match_recruitment.rs:128` | Tutorial-aware shared predicate with initialized Horde exception. | Canonical pair guard with initialized Horde exception and construction exclusion. | Use shared union, retain Horde exception and explicit is_construction refusal. | Classic/World continue to use queued training. |
| M3-27, `-256,11 +252,11` | `rust/crates/core/src/match_recruitment.rs:252` | Ordinary finish validator also validates Tutorial state. | Generic finish validator retains initialized Horde purchases. | Explicitly validate Tutorial state, then generic finish state; admission_facts still enforces the ordinary/Horde gate. | Retain Tutorial invariants without rejecting the existing Horde path. |
| M3-28, `-600,13 +600,8` | `rust/crates/core/src/match_routing.rs:600` | Tutorial-aware shared routing predicate. | Expanded ordinary routing predicate and matching-mode diagnostic. | Use shared union predicate and PR diagnostic. | Keep routing admission for both runtime families. |
| M3-29, `-94,11 +94,8` | `rust/crates/core/src/match_runtime.rs:94` | Tutorial startup variant. | Elimination startup variant. | Keep both variants. | Both constructors need their distinct profile. |
| M3-30, `-120,11 +117,7` | `rust/crates/core/src/match_runtime.rs:117` | prepare_tutorial declaration. | prepare_elimination declaration. | Retain prepare_elimination here and a complete prepare_tutorial below. | Keep both public-to-crate startup entry points. |
| M3-31, `-133,11 +126,22` | `rust/crates/core/src/match_runtime.rs:126` | prepare_tutorial body uses StartupProfile::Tutorial. | prepare_elimination body uses StartupProfile::Elimination. | Keep both complete methods, each with its own profile and cfg(test) replay argument. | Neither mode can silently use the other startup profile. |
| M3-32, `-222,11 +226,8` | `rust/crates/core/src/match_runtime.rs:226` | Conquest-only fallback exempts Tutorial. | Conquest-only fallback exempts elimination. | Exempt both validated profiles. | Their explicit profile/mode checks remain above the fallback. |
| M3-33, `-439,13 +440,12` | `rust/crates/core/src/match_runtime.rs:440` | Keep only positive-MP objectives for Assault, Tutorial and Horde. | Keep only positive-MP objectives for Assault, Annihilation and Horde. | Apply the restriction to the union of all four families. | Retain every parent objective-pruning rule. |
| M3-34, `-897,11 +897,7` | `rust/crates/core/src/match_runtime.rs:897` | Tutorial skips ordinary base creation. | Classic skips ordinary base creation. | Skip setup_bases for both Classic and Tutorial. | Avoid unwanted Tutorial bases and duplicate Classic HQ setup. |
| M3-35, `-919,11 +915,7` | `rust/crates/core/src/match_runtime.rs:915` | Tutorial-aware shared routing/logistics initialization. | Expanded ordinary routing/logistics initialization. | Use shared union predicate. | All represented ordinary runtimes initialize required phase state. |
| M3-36, `-965,11 +957,7` | `rust/crates/core/src/match_runtime.rs:957` | Point phase calls ordinary finish validator. | Point phase calls generic finish validator. | Keep expanded ordinary finish validator. | Preserve Tutorial runtime checks and all PR mode pairs. |
| M3-37, `-1066,25 +1054,16` | `rust/crates/core/src/match_runtime.rs:1054` | Income uses Tutorial kind and excludes the Tutorial enemy slot. | Income uses actual non-Conquest mode and Assault defender team. | Keep actual mode, Assault defender and Tutorial income-excluded slot; remove redundant default update. | Preserve both income policies and strict Clippy. |
| M3-38, `-146,11 +146,7` | `rust/crates/core/src/match_support.rs:146` | Tutorial-aware shared support predicate. | Expanded ordinary support predicate. | Use shared union predicate. | Keep represented support modes and later actor checks. |
| M3-39, `-289,8 +289,7` | `rust/crates/core/src/match_tick.rs:289` | Tutorial-aware ordinary admission or World. | Expanded ordinary admission or World. | Use shared union predicate or World. | Retain strict runner mode pairing. |
| M3-40, `-298,13 +297,9` | `rust/crates/core/src/match_tick.rs:297` | Tutorial runtime validation. | Classic/Annihilation elimination-state validation. | Run each mode-specific validation under its own condition. | No runtime owner validation is removed. |
| M3-41, `-471,18 +466,7` | `rust/crates/core/src/match_tick.rs:466` | World/Tutorial shared sampler; other modes use Conquest wrapper. | All modes use sample_match_tick. | Use shared sampler for all admitted modes. | Conquest-only wrapper would reject Assault, Annihilation and Classic. |
| M3-42, `-807,7 +791,6` | `rust/crates/core/src/match_tick.rs:791` | Tutorial skips Conquest victory. | Assault/elimination/Conquest victory match begins here. | Retain mode-specific victory match. | Keep Assault and elimination phase dispatch. |
| M3-43, `-817,17 +800,14` | `rust/crates/core/src/match_tick.rs:800` | Tutorial skips Victory phase and progresses afterward. | Assault/elimination victory dispatch plus ordinary Conquest fallback and Victory phase. | Add Tutorial no-op arm and emit Victory only outside Tutorial. | Tutorial progression remains separate and all other victory paths survive. |
| M3-44, `-2369,13 +2369,12` | `rust/crates/server/src/lobby.rs:2369` | net_runtime3 test module. | mode_runtime_a_tests module. | Keep both modules, each with its own cfg(test), plus existing net_runtime2. | Preserve all test registrations and release-build gating. |
| M3-45, `-14,7 +14,12` | `rust/crates/server/src/mode_runtime_a_tests.rs:14` | Connection gained an optional buffered accounting field; main fixtures use None. | Lobby fixture initializes only id, tx and cancel. | Initialize buffered: None in this test fixture. | Adapt the fixture to main without weakening any assertion. |

## Fourth merge `56a1710d09ed32ae8a532b064afbc260d9cc7025`: every hunk

Committed tree: `dc9675b2a7abbb564198658f799dee083c3f3d5c`.

Main advanced during this repair. The fourth merge incorporates that refresh before final validation rather than claiming that the earlier frozen-main check covered it.

- Parent 1 (#301): `07f06fadf6a03930685cd94ee5bfca6f086eb7c9`.
- Parent 2 (merged main): `1cb49541930dca7f17b3ab01ec4b4e193e70fdf4`.
- Reconstructed conflict-marker tree: `1863be9fedaa6609569566b9509bf42cc859decc`.
- Exact command: `git merge-tree --write-tree 07f06fadf6a03930685cd94ee5bfca6f086eb7c9 1cb49541930dca7f17b3ab01ec4b4e193e70fdf4` (exit 1).
- The only synthetic-to-committed changed path is DESIGN.md, with three unified-3 hunks below. All incoming Horde AI sources, test registration and changelog bytes are the automatic merge output. There are no additional manual runtime edits in this refresh.

| Hunk | Committed file:line | Main side | PR #301 side | Resolution | Why |
| --- | --- | --- | --- | --- | --- |
| M4-01, `-22,7 +22,6` | `DESIGN.md:22` | Private Horde director queue decision. | MODE runtime A decision. | Remove opening marker and retain MODE runtime A text. | Preserve both documented designs without changing either runtime. |
| M4-02, `-37,7 +36,7` | `DESIGN.md:36` | Private Horde director queue decision. | MODE runtime A decision. | Keep the complete incoming Private Horde director queue section after MODE runtime A. | Preserve both documented designs without changing either runtime. |
| M4-03, `-55,7 +54,6` | `DESIGN.md:54` | Private Horde director queue decision. | MODE runtime A decision. | Remove closing marker; retain the shared scenario/refund and map sections below. | Preserve both documented designs without changing either runtime. |

## Fifth merge `4de6e424a52c5223537f4c069dc8a9b028f9de55`: final frozen main `edeb4ca218e23b25daea33b43e2809d9fe17cf41`

The first full workspace run passed, but main advanced during it and introduced three new conflict blocks in match_actor.rs, match_firing.rs and match_runtime.rs. The user explicitly granted one additional final full run with `-j2`, the same 6 GiB/zero-swap cap and one slot. The user also instructed that any later main movement must not be chased during that final run: push the frozen result and report which main it contains.

- Committed tree: `e43a8f932da368367beca406bc9cc6a51ad8aace`.
- Parent 1 (#301): `56a1710d09ed32ae8a532b064afbc260d9cc7025`.
- Parent 2 (merged main): `edeb4ca218e23b25daea33b43e2809d9fe17cf41`.
- Reconstructed conflict-marker tree: `3b0aa084d640fda73883aca7771275c53e056b14`.
- Exact command: `git merge-tree --write-tree 56a1710d09ed32ae8a532b064afbc260d9cc7025 edeb4ca218e23b25daea33b43e2809d9fe17cf41` (exit 1).
- The three conflict blocks produce four unified-3 diff hunks. Seven further hunks record necessary clean-merge repairs and the test extension. No synthetic-to-resolved changed hunk is omitted.

| Hunk | Committed file:line | Main side | PR #301 side | Resolution | Why |
| --- | --- | --- | --- | --- | --- |
| M5-01, `-36,18 +36,9` | `rust/crates/core/src/match_actor.rs:36` | Horde roster branch with 2-6 seats, canonical slots, team 0 humans and last-slot team 1 enemy. | Strict ordinary_ground_mode union plus matching World. | Keep the ordinary union guard, World arm and complete guarded Horde branch. | Preserve all represented canonical actors without admitting malformed Horde. |
| M5-02, `-58,7 +49,6` | `rust/crates/core/src/match_actor.rs:49` | Closing marker after complete Horde roster branch. | Closing portion of strict actor mode helper. | Remove marker and keep canonical mismatch rejection. | Complete the same strict mode union. |
| M5-03, `-1,7 +1,6` | `rust/crates/core/src/match_construction.rs:1` | No Mode import in ordinary-predicate construction. | World-only exclusion used imported Mode. | Remove now-unused Mode import. | Construction now names the ordinary predicate directly. |
| M5-04, `-267,7 +266,7` | `rust/crates/core/src/match_construction.rs:266` | Construction admission requires ordinary_ground_mode, excluding Horde and World. | Actor validation plus World-only exclusion. | Require ordinary_ground_mode as well as actor/runtime validation. | New Horde actor support must not silently open construction. |
| M5-05, `-324,7 +323,9` | `rust/crates/core/src/match_construction.rs:323` | Construction workers require ordinary_ground_mode. | Actor validation plus World-only exclusion. | Require ordinary_ground_mode and retain actor/runtime validation. | Prevent partial Horde worker admission when finish still refuses it. |
| M5-06, `-356,28 +356,7` | `rust/crates/core/src/match_firing.rs:356` | Explicit firing mode preflight includes matching Horde before actor/runtime validation. | Shared checked_game_mode preflight preserves the exact canonical mismatch error. | Keep shared preflight followed by actor/runtime validation, now including guarded Horde. | Both parents retain strict mode preflight; malformed Horde remains rejected. |
| M5-07, `-133,6 +133,31` | `rust/crates/core/src/match_horde_tests.rs:133` | Existing Horde setup test covers private wave setup, guarded full ticks and aviation. | The test is inherited unchanged at the prior head. | Extend it with valid actor admission, exact construction/repair refusals and unchanged Game/journal assertions. | Protect the merge-specific Horde guard boundary without dropping existing predicates. |
| M5-08, `-121,7 +121,7` | `rust/crates/core/src/match_recovery.rs:121` | Explicit matching Default/Conquest preflight with no canonical mode, excluding Horde. | Actor validation plus World-only exclusion. | Require ordinary_ground_mode and retain actor/runtime validation. | Keep Horde building repair closed after broadening actor support. |
| M5-09, `-176,7 +176,7` | `rust/crates/core/src/match_recovery.rs:176` | The mutation path repeats the explicit matching Default/Conquest guard. | Actor validation plus World-only exclusion. | Require ordinary_ground_mode and retain actor/runtime validation. | Both preflight and mutating entry points preserve the same admission. |
| M5-10, `-967,13 +967,13` | `rust/crates/core/src/match_runtime.rs:967` | Local income context handles Horde separately; other modes default to Conquest except Tutorial. | Actual ordinary mode kind, Assault defender and Tutorial exclusion. | In non-Horde context retain actual mode, Assault defender and Tutorial exclusion; preserve main Horde context unchanged. | Avoid losing Annihilation income, Classic nodes/currencies and Assault defender policy in a cleanly merged block. |
| M5-11, `-1071,22 +1071,7` | `rust/crates/core/src/match_runtime.rs:1071` | advance_income consumes the prepared local mode context. | advance_income constructs the PR mode context inline. | Use the combined prepared mode context, retaining Classic node facts. | Share main Horde validation and all PR income semantics without duplicate contexts. |

The automatically merged combat path keeps #301's Annihilation howitzer reduction and main's Horde-only bunker/Kaiju defender scaling. Their predicates are disjoint, so neither multiplier replaces or duplicates the other. Incoming aircraft snapshot adapters, enemy faction checks and refund tests remain intact. Existing public server and full Horde tick admission stay guarded.

This record and `changelog.d/2026-10-16-fix301-merge-record.md` are added in a separate documentation commit after the merges and final validation. Neither is a hidden synthetic-to-merge runtime change. The full resolution inventory is 1 + 20 + 45 + 3 + 11 = 80 unified-3 hunks. The five prior rewritten tests are accounted separately above.

## Validation

The first full workspace invocation passed at producer `56a1710d09ed32ae8a532b064afbc260d9cc7025`, Rust tree `9c941e57ece7f5f7241fa0126ae4453a89b75eaa`: 1,202 passed, 0 failed, 10 skipped. Counts: core 812, FFI 10, server 119, simtest 261. Its nextest run ID is `487b4e59-1db1-44e8-aeb9-efc7d509cee5`. Formatting and strict workspace/all-target Clippy also passed on that Rust tree. The full run used S3 with `memory.max=6442450944` and `memory.swap.max=0`; its process and scope exited, and the S3 lock was verified available afterward. Raw receipts remain under `/tmp/fix301-validation/`. These results do not qualify the subsequent edeb4ca2 integration.

The preliminary focused invocation reported 56/56 passed. Its compilation overlapped the first main refresh, and its shell wrapper later exited 2 after being edited during execution. It is diagnostic only. Initial formatting and compilation diagnostics were repaired without weakening any test: layout, missing Mode references, and the Connection buffered initializer. Historical results in mode-runtime-a.md retain their original producing source.

Final producer: `4de6e424a52c5223537f4c069dc8a9b028f9de55`, Rust tree `1b45f0a9d9738ab67a4ab5dcac46f62017ed53c6`, containing main `edeb4ca218e23b25daea33b43e2809d9fe17cf41`. The final documentation commit preserves this Rust tree exactly.

From rust/ on the Vostro with Rust 1.99.0:

- `cargo fmt --all --check`: PASS.
- `cargo clippy --workspace --all-targets -j4 -- -D warnings`: PASS.
- Focused `cargo nextest run -p ww2-core -p ww2-server -p ww2-simtest --cargo-profile ci-test -j4` with the overlapping mode/Horde/Tutorial/aircraft/refund/guard filter: 74 passed, 0 failed, 1,150 skipped.
- Final explicitly authorized `cargo nextest run --workspace --cargo-profile ci-test -j2` with `CARGO_BUILD_JOBS=2`: 1,224 passed, 0 failed, 10 skipped, across 16 binaries. One slow test passed; no retries. Test execution took 615.304 seconds. Counts: core 831, FFI 10, server 119, simtest 264.
- Final nextest run ID: `ca406fe0-a697-4696-bafa-60e7ebac8dea`.
- S3 target: `/tmp/ww2-native-target-S3-0e8d6d9b`; actual active cgroup `memory.max=6442450944`, `memory.swap.max=0`. The nextest process and scope exited and the lock was released immediately when Cargo returned.

Logs: `/tmp/fix301-validation/fmt-refresh.log`, `clippy-refresh.log`, `focused-refresh.log`, `final-suite.log`, `final-suite.exit`, `final-suite-caps.json` and `final-suite-counts.json`. Their hashes are recorded below so a receipt cannot silently be substituted. Initial compile/format diagnostics and the first-run receipts remain in that directory. The only later source change from the first passing full run is the documented edeb4ca2 merge and its guarded repairs/test extension.

The final main head is deliberately frozen by the user's instruction. Any later main movement is reported at push and is not incorporated or presented as tested by these receipts. Hosted checks and a fresh owner acknowledgment remain the coordinator's gates. No issue post, force-push, server admission, reference change, ABI or contract change is part of this repair.

```text
f1036bc69cf8ec07ba0a11982f687af8d4038c9502b443631cad0126a79f96cb  fmt-refresh.log
6c558804fa90f576496f5e2664b98ab80c0254012bc14b538c43bee9ccc1062e  clippy-refresh.log
a06453d03b51762c48c878fbe32b0d79c1657823d8d30e3b68689507730ede1d  focused-refresh.log
dec676ba59ff042bda2ab6846b7261859fd49d3c25a62007ce78d88b9f9806fa  final-suite.log
9a271f2a916b0b6ee6cecb2426f0b3206ef074578be55d9bc94f6f3fe3ab86aa  final-suite.exit
2ec40c02d2cdf3e1547cade1f771c6a3a7de0bcd12a6600325a93b2a08089054  final-suite-caps.json
9275b87e1e920069696048b0bacc07dbfd06c371aca9092be3d9f5168fea1d5a  final-suite-counts.json
```
