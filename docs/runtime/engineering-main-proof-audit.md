# Existing-main engineering acceptance audit

Baseline: `8d6e2eae45c4b304e196ee654ca3a412ea94b796`. Reference: `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`.

This audit reads every one of the 52 ENG rows that is in progress at the baseline. It applies the existing PARITY definition of ported: Rust passes the row's unchanged Acceptance check. Component fixtures can prove a selected check without establishing every behavior or consumer named in the row. No row is jointly accepted by this audit.

This audit marks 15 additional rows ported, reserves ENG-18..22 for pending PR #224, and holds 32 rows. Reserved rows are not edited here, so the independent #224 candidate and its review and hosted gates remain intact. Baseline live status count: 36 ported of 939 unique rows. This candidate alone brings that baseline to 51 ported of 939, including 15 ENG rows; pending independent batches are not added. The 65 non-status table entries and three fenced historical BASE duplicates are outside that denominator.

## Execution attribution

The existing [three-platform run 38667938011](https://github.com/axelmanosalvasmd/ww2-rts-native/actions/runs/38667938011) executed `e7adb0c3ddf31e709cab117a53e3108f36f489cb`: 483 passed, 0 failed and 0 ignored across 111 result sections on each platform. The complete log SHA256 is `7ed2d216921b47bfaa3f8569eb68590184f2c10ff931779865cfebab0658ea0e`. These are old-head executions, not executions of this documentation candidate.

The promoted rows use 17 distinct named tests, each recorded once per platform (51 named passing records). Static mode/type/blob comparison preserves all 664 existing Rust, harness, workflow, reference-pin and attribute entries from that execution at the audit baseline; the entire core tree is identical. The only additional Rust/harness inputs are the separate R1 diagnostic binary and recorder. No existing input changes, new execution or inferred additional passing test is counted. New local native and Source-recorder jobs: 0.

## Named proof providers

Raw log line numbers below refer to the complete log identified above. Each name passed on Ubuntu 24.04, Windows 2025 and macOS 15. Fixture inventory sizes in row notes are recorded data, not extra test executions.

### P01

`actual_engineering_commands_ticks_and_recipient_reads` at `rust/crates/simtest/tests/recipient_engineering.rs:239`.

Fixtures: `rust/crates/simtest/tests/fixtures/recipient-engineering.json.gz`.

Passing raw log lines: ubuntu-24.04 1659, windows-2025 3664, macos-15 5552.

### P02

`authoritative_environment_matches_original_actions_and_order` at `rust/crates/core/tests/environment.rs:252`.

Fixtures: `rust/crates/core/tests/fixtures/environment.json`.

Passing raw log lines: ubuntu-24.04 716, windows-2025 2721, macos-15 4609.

### P03

`authoritative_maps_match_pinned_validation_and_static_terrain` at `rust/crates/core/tests/authoritative_maps.rs:32`.

Fixtures: `rust/crates/core/tests/fixtures/authoritative-maps.json`.

Passing raw log lines: ubuntu-24.04 625, windows-2025 2630, macos-15 4518.

### P04

`canonical_mass_projects_replay_original_full_steps` at `rust/crates/simtest/tests/mass_project_tick.rs:689`.

Fixtures: `rust/crates/core/tests/fixtures/mass-project-tick.json`.

Passing raw log lines: ubuntu-24.04 1456, windows-2025 3461, macos-15 5349.

### P05

`constructor_defaults_mass_tables_and_rows_match_original_helpers` at `rust/crates/core/tests/environment_debris.rs:120`.

Fixtures: `rust/crates/core/tests/fixtures/environment-debris.json`.

Passing raw log lines: ubuntu-24.04 723, windows-2025 2728, macos-15 4616.

### P06

`field_works::match_engineering::tests::original_paid_dig_lifecycles` at `rust/crates/core/src/match_engineering_tests.rs:155`.

Fixtures: `rust/crates/core/tests/fixtures/match-engineering.json`.

Passing raw log lines: ubuntu-24.04 489, windows-2025 2494, macos-15 4382.

### P07

`field_works::match_projects::tests::actual_match_project_phases_match_original_source` at `rust/crates/core/src/match_projects_tests.rs:363`.

Fixtures: `rust/crates/core/tests/fixtures/mass-entrenchment.json`.

Passing raw log lines: ubuntu-24.04 506, windows-2025 2515, macos-15 4402.

### P08

`infantry_naval_and_vehicle_paths_match_original_find_path` at `rust/crates/simtest/tests/pathfinding.rs:59`.

Fixtures: `rust/crates/simtest/tests/fixtures/pathfinding.json`.

Passing raw log lines: ubuntu-24.04 1644, windows-2025 3649, macos-15 5537.

### P09

`initial_structures_match_original_validation_graphs_materials_and_health` at `rust/crates/core/tests/structures.rs:55`.

Fixtures: `rust/crates/core/tests/fixtures/structures.json`.

Passing raw log lines: ubuntu-24.04 923, windows-2025 2928, macos-15 4816.

### P10

`live_weather_matches_original_steps_and_order` at `rust/crates/core/tests/weather_runtime.rs:103`.

Fixtures: `rust/crates/core/tests/fixtures/weather-runtime.json`.

Passing raw log lines: ubuntu-24.04 959, windows-2025 2964, macos-15 4852.

### P11

`match_recovery::tests::represented_recovery_phases_match_original_source` at `rust/crates/core/src/match_recovery/tests.rs:377`.

Fixtures: `rust/crates/core/tests/fixtures/match-recovery.json`.

Passing raw log lines: ubuntu-24.04 558, windows-2025 2563, macos-15 4451.

### P12

`motion_contact_thresholds_and_settlement_match_original_helpers` at `rust/crates/core/tests/environment_debris.rs:160`.

Fixtures: `rust/crates/core/tests/fixtures/environment-debris.json`.

Passing raw log lines: ubuntu-24.04 724, windows-2025 2729, macos-15 4617.

### P13

`mutation_state_journals_and_pending_match_original_source` at `rust/crates/core/tests/world.rs:261`.

Fixtures: `rust/crates/core/tests/fixtures/world.json`.

Passing raw log lines: ubuntu-24.04 967, windows-2025 2972, macos-15 4861.

### P14

`ordered_damage_riders_terrain_and_death_match_original_source` at `rust/crates/core/tests/match_combat.rs:308`.

Fixtures: `rust/crates/core/tests/fixtures/match-combat.json`.

Passing raw log lines: ubuntu-24.04 815, windows-2025 2820, macos-15 4708.

### P15

`original_shared_pattern_order_front_and_segment_prices` at `rust/crates/core/tests/field_projects.rs:19`.

Fixtures: `rust/crates/core/tests/fixtures/mass-entrenchment.json`.

Passing raw log lines: ubuntu-24.04 738, windows-2025 2743, macos-15 4631.

### P16

`paid_placement_and_timed_cells_match_original_source` at `rust/crates/core/tests/field_works.rs:128`.

Fixtures: `rust/crates/core/tests/fixtures/field-works.json`.

Passing raw log lines: ubuntu-24.04 746, windows-2025 2751, macos-15 4639.

### P17

`recipient_snapshot::tests::represented_fields_match_actual_cached_reference_snapshots` at `rust/crates/core/src/recipient_snapshot.rs:1006`.

Fixtures: `rust/crates/core/tests/fixtures/recipient-snapshot.json`.

Passing raw log lines: ubuntu-24.04 597, windows-2025 2602, macos-15 4491.

### P18

`startup_plan_and_initial_force_match_original_create_game` at `rust/crates/simtest/tests/match_start.rs:49`.

Fixtures: `rust/crates/simtest/tests/fixtures/match-start.json`.

Passing raw log lines: ubuntu-24.04 1532, windows-2025 3537, macos-15 5425.

### P19

`support_is_undirected_and_excludes_failed_or_dead_sections` at `rust/crates/core/tests/structures.rs:218`.

Fixtures: `rust/crates/core/tests/fixtures/structures.json`.

Passing raw log lines: ubuntu-24.04 922, windows-2025 2927, macos-15 4815.

## All 52 original in-progress rows

The Behavior, Source evidence and Acceptance check text below are copied from the baseline. A held row identifies the missing selected-check witness found in this audit, rather than treating every broader runtime limitation as an added acceptance clause.

### ENG-01

**Disposition:** ported by existing selected-check proof.

**Behavior:** Trench: 30 MP, 4 cells of T (COVER, TRENCH), laid across the builders' approach. Each dug cell remembers a front facing away from the diggers (or the pattern front).

**Source evidence:** `shared/sim.js:68`, `:175`, `:424`, `:3641`, `:5408`.

**Unchanged Acceptance check:** test.js "Digging" 'plans a 4-cell line', 'four trench cells logged'; "Dig direction"

**Existing proof:** P04, P06, P16. Fixtures: `field-works.json`, `match-engineering.json`.

**Selected-check finding:** All 22 field-work traces compare complete:trench, twenty-hertz-trench, the four placement-angle cases and exact front bits. Full ticks retain four committed T cells and fronts in completion-clears-on-next-turn.

**Limit:** Component and full-tick work checks do not establish every recipient/whole-match consumer.

### ENG-02

**Disposition:** ported by existing selected-check proof.

**Behavior:** Sandbags: 20 MP, 4 cells of # (COVER), 150 hp, destroyed to crater; tanks crush them to crater.

**Source evidence:** `shared/sim.js:78`, `:175`, `:425`.

**Unchanged Acceptance check:** test.js "Fortifications" sandbags/wire/traps chars '#','X','Y'

**Existing proof:** P04, P06, P16. Fixtures: `field-works.json`, `match-engineering.json`.

**Selected-check finding:** The complete selected #/X/Y output check is compared by complete:sandbags, complete:wire and complete:traps field-work traces. Moved crushing remains explicitly refused; this broader runtime limitation stays in evidence.

**Limit:** Moved vehicle crushing remains explicitly refused at match_ground_contact.rs:60-78.

### ENG-03

**Disposition:** in progress; selected-check proof incomplete.

**Behavior:** Barbed Wire: 25 MP, 5 cells of X (WIRE). Infantry move at 0.35x and pay +4 path cost; vehicles pass and flatten it to open ground; 40 hp, destroyed to open ground.

**Source evidence:** `shared/sim.js:68`, `:78`, `:426`, `:2366`, `:3160`.

**Unchanged Acceptance check:** test.js "Fortifications" 'wire slows the squad', 'but it gets through', 'the tank flattened the wire it drove over'

**Existing proof:** P06, P16. Fixtures: `field-works.json`, `match-engineering.json`.

**Selected-check finding:** The selected wire check includes a tank flattening wire during movement. match_ground_contact.rs still explicitly refuses moved crushing; no such successful native Source trace is claimed.

### ENG-04

**Disposition:** in progress; selected-check proof incomplete.

**Behavior:** Tank Traps: 40 MP, 4 cells of Y (VBLOCK, COVER). Vehicles cannot enter; infantry pass and get cover; 250 hp, destroyed to open ground. A trap cell is skipped while a vehicle stands on it.

**Source evidence:** `shared/sim.js:78`, `:175`, `:427`, `:5391-5392`.

**Unchanged Acceptance check:** test.js "Fortifications" 'no way through for the tank', 'infantry walks through'

**Existing proof:** P06, P16. Fixtures: `field-works.json`, `match-engineering.json`.

**Selected-check finding:** Paid trap flags/health/occupied-cell skip are compared, but no bound Source-backed full Y-barrier tank-no-path versus infantry-path pair was located for the two selected assertions.

### ENG-05

**Disposition:** in progress; selected-check proof incomplete.

**Behavior:** Gun Pit (key `nest`): 60 MP, a T pit cell with a horseshoe of 5 sandbags on its sides and front. The pit cell joins `g.pits`; an MG, AT gun, mortar, flak or howitzer standing in a pit cell that is still T gets 1.25x weapon range. A projectile's first segment ignores sandbag cells beside the shooter.

**Source evidence:** `shared/sim.js:428`, `:445-447`, `:463`, `:5409`, `:4231`.

**Unchanged Acceptance check:** test.js "Fortifications" 'MG nest: a trench pit and sandbags'; test-fortress.js 'the finished Gun Pit marks its pit cell', 'from the pit the MG reaches 42 m'

**Existing proof:** P06, P16. Fixtures: `field-works.json`, `match-engineering.json`.

**Selected-check finding:** Nest stamping is compared. The selected finished-pit membership and MG 42 m range witness is not bound to the existing Source/native firing fixture; no all-weapon pit or first-sandbag-segment claim.

### ENG-06

**Disposition:** ported by existing selected-check proof.

**Behavior:** Minefield: 40 MP, 4 cells of mine overlay. Rules in ENG-62..66.

**Source evidence:** `shared/sim.js:429`.

**Unchanged Acceptance check:** test.js "Mines" 'four mines are laid'

**Existing proof:** P04, P06, P16. Fixtures: `field-works.json`, `match-engineering.json`.

**Selected-check finding:** complete:mines compares 40 MP debit, four mine overlays/owners and ground layers. Does not promote detonation or the separately held mine rows.

**Limit:** Mine laying does not establish hostile contact or detonation; that guard remains.

### ENG-09

**Disposition:** ported by existing selected-check proof.

**Behavior:** Clear mines: 5 MP, 4 cells; only mines the ordering team knows; worked from 8 m.

**Source evidence:** `shared/sim.js:434-436`, `:466`, `:5393`.

**Unchanged Acceptance check:** test.js line 7728 check 'unknown mines cannot be cleared...', 'the mines are lifted'

**Existing proof:** P04, P06, P16. Fixtures: `field-works.json`, `match-engineering.json`.

**Selected-check finding:** mine-road-and-known-clearing compares 5 MP payment, unknown refusal and known-only clearing; demine-distant-cached-approach and Match cached goal compare 8 m approach.

**Limit:** This evidence covers the selected known-only clearing check, not hostile mine contact.

### ENG-10

**Disposition:** ported by existing selected-check proof.

**Behavior:** Field Hospital: 100 MP, one A cell (passable, no flags), 12 s; one per player standing or going up (`max`); 150 hp, destroyed to crater. Allied infantry within 12 m reinforce at half pace for the same MP.

**Source evidence:** `shared/sim.js:78`, `:109`, `:175`, `:438`, `:3644-3645`, `:5411`, `:5652-5653`.

**Unchanged Acceptance check:** test.js line 7728 check 'one going up is the limit', 'one per player'

**Existing proof:** P04, P06, P16. Fixtures: `field-works.json`, `match-engineering.json`.

**Selected-check finding:** Both selected hospital maximum checks pass exact Source snapshots: max while aid job exists, max while A stands, then admission after removal. Broader reinforcement/destruction checks remain separately scoped.

**Limit:** The selected maximum checks do not establish every reinforcement, surface or destruction consumer.

### ENG-13

**Disposition:** in progress; selected-check proof incomplete.

**Behavior:** Builders and pace: rifle, conscript and engineer squads (concrete: Engineers in construction modes). One cell per 3 s (`FORTS.time` where set: hospital 12, wall 6, gate 10), Engineers twice as fast. The crew must be within reach (3 m default, bridge 9, Clear mines 8, wall and gate 6).

**Source evidence:** `shared/sim.js:68`, `:379`, `:451`, `:3185`, `:3189`, `:5386-5389`.

**Unchanged Acceptance check:** test.js "Digging" 'MG teams do not build'; "Fortifications" 'engineers build', 'conscripts build'

**Existing proof:** P06, P16. Fixtures: `field-works.json`, `match-engineering.json`.

**Selected-check finding:** MG refusal and Engineer work are present. The selected Conscript builds-wire execution is not among the existing paid-work traces; unsupported concrete/bridge branches remain broader runtime limits.

### ENG-14

**Disposition:** in progress; selected-check proof incomplete.

**Behavior:** Buildable ground: line works take open ground, crater, rubble, road or mud; a work across a road replaces it. Each cell is rechecked when its turn comes; a cell that changed is skipped (no refund).

**Source evidence:** `shared/sim.js:454`, `:466`, `:5404-5411`.

**Unchanged Acceptance check:** test.js line 7728 check 'no trench among the trees'

**Existing proof:** P06, P16. Fixtures: `field-works.json`, `match-engineering.json`.

**Selected-check finding:** The existing H-cell exclusion is a partial three-cell plan, not the selected fully wooded-footprint command returning blocked. No complete all-O blocked placement witness is bound.

### ENG-15

**Disposition:** in progress; selected-check proof incomplete.

**Behavior:** Dig command: first selected squad; known kind; tier; builder type; not retreating; MP >= cost unless queued. Direction = `dir`, else the bearing from the squad plus 90 degrees (bridge: along it). Pays the full cost at the start. A queued dig pays when it starts and is dropped if unaffordable.

**Source evidence:** `shared/sim.js:3635-3650`.

**Unchanged Acceptance check:** test.js "Queued digging spends MP when it starts, and an unaffordable task is skipped"; test.js "Capture, VP and win" 'malformed fortification names do not spend manpower'

**Existing proof:** P06, P16. Fixtures: `field-works.json`, `match-engineering.json`.

**Selected-check finding:** The selected queued Dig payment/skip cannot be claimed: match_commands.rs returns Unsupported for queued Dig. Malformed names alone do not satisfy the full selected check.

### ENG-16

**Disposition:** in progress; selected-check proof incomplete.

**Behavior:** Fort placement: plan cells centred on the click along `dir` (Gun Pit uses its 6-cell stamp). If any plan cell is unseen the answer is `notVisible` with no cells; cells are filtered by ground; bridge plans keep only cells chained to a bank or standing deck; an empty plan is `blocked`. The client preview calls the same function.

**Source evidence:** `shared/sim.js:459-469`, `:2542-2551`, `:3264-3276`; `client/main.js:1208`.

**Unchanged Acceptance check:** test.js "Entrenchment placements reveal nothing about unseen terrain and recheck sight when queued segments start"

**Existing proof:** P06, P16. Fixtures: `field-works.json`, `match-engineering.json`.

**Selected-check finding:** Initial hidden placement is tested; the selected whole deferred-sight check also removes a scout after admission and requires no paid job/MP/ghost. That after-admission sight-loss witness is not present in the inspected project fixtures.

### ENG-17

**Disposition:** in progress; selected-check proof incomplete.

**Behavior:** Segment cost: max(1, round(cost x buildable cells / cells per segment)).

**Source evidence:** `shared/sim.js:512`.

**Unchanged Acceptance check:** test.js "Mass entrenchment" (line 1428) 'the line cost five segments'

**Existing proof:** P04, P07, P15. Fixtures: `mass-entrenchment.json`, `mass-project-tick.json`.

**Selected-check finding:** Supported segment price vectors and four-segment default project are compared. The selected completed 40 m/five-segment paid-line cost witness is absent from those cases; unsupported Fill/Wall prices remain an explicit broader limit.

### ENG-18

**Disposition:** reserved for PR #224; status unchanged.

**Behavior:** Trench line: segments of (cells x 2) m (trench 8 m) along a to b, 1-12 segments centred on the midpoint, facing away from the diggers' mean position. A plain click is one segment across the approach.

**Source evidence:** `shared/sim.js:477-488`, `:494`.

**Unchanged Acceptance check:** test.js "Mass entrenchment" (line 1347) 'a 40 m line is five trench segments', 'a plain click is one segment', 'a line faces away from the diggers'

**Existing proof:** P04, P07, P15. Fixtures: `mass-entrenchment.json`, `mass-project-tick.json`.

**Selected-check finding:** Root reserves the status edit to unchanged pending PR 224 to avoid duplication; no gate or test-source change. Existing main has 168 exactordered planner comparisons and project snapshots; ENG-22 additionally has exact 10 m/eight-job circle/front proof.

**Limit:** Reserved for pending PR #224; its strengthened test bytes and qualified local proof are not attributed to this main baseline.

### ENG-19

**Disposition:** reserved for PR #224; status unchanged.

**Behavior:** Zigzag: 1-16 segments turned 0.6 rad alternately forward and back of the line.

**Source evidence:** `shared/sim.js:489-493`.

**Unchanged Acceptance check:** test.js line 1347 'a zigzag packs more segments into the same frontage', 'zigzag segments alternate between two directions'

**Existing proof:** P04, P07, P15. Fixtures: `mass-entrenchment.json`, `mass-project-tick.json`.

**Selected-check finding:** Root reserves the status edit to unchanged pending PR 224 to avoid duplication; no gate or test-source change. Existing main has 168 exactordered planner comparisons and project snapshots; ENG-22 additionally has exact 10 m/eight-job circle/front proof.

**Limit:** Reserved for pending PR #224; its strengthened test bytes and qualified local proof are not attributed to this main baseline.

### ENG-20

**Disposition:** reserved for PR #224; status unchanged.

**Behavior:** Double line: the line plus a second row 6 m behind it, on the diggers' side.

**Source evidence:** `shared/sim.js:494`.

**Unchanged Acceptance check:** test.js line 1347 'a double line is two rows', 'the second row is behind the first...'

**Existing proof:** P04, P07, P15. Fixtures: `mass-entrenchment.json`, `mass-project-tick.json`.

**Selected-check finding:** Root reserves the status edit to unchanged pending PR 224 to avoid duplication; no gate or test-source change. Existing main has 168 exactordered planner comparisons and project snapshots; ENG-22 additionally has exact 10 m/eight-job circle/front proof.

**Limit:** Reserved for pending PR #224; its strengthened test bytes and qualified local proof are not attributed to this main baseline.

### ENG-21

**Disposition:** reserved for PR #224; status unchanged.

**Behavior:** Arc: trench segments over 120 degrees centred on a; radius = distance a to b clamped to 8-30 m (12 m on a plain click); 2-20 segments facing out from a.

**Source evidence:** `shared/sim.js:495-498`.

**Unchanged Acceptance check:** test.js line 1347 'an arc bows toward the second point'

**Existing proof:** P04, P07, P15. Fixtures: `mass-entrenchment.json`, `mass-project-tick.json`.

**Selected-check finding:** Root reserves the status edit to unchanged pending PR 224 to avoid duplication; no gate or test-source change. Existing main has 168 exactordered planner comparisons and project snapshots; ENG-22 additionally has exact 10 m/eight-job circle/front proof.

**Limit:** Reserved for pending PR #224; its strengthened test bytes and qualified local proof are not attributed to this main baseline.

### ENG-22

**Disposition:** reserved for PR #224; status unchanged.

**Behavior:** Ring: a full circle; radius clamped to 6-24 m (10 m on a plain click); 4-20 segments facing out.

**Source evidence:** `shared/sim.js:495-498`.

**Unchanged Acceptance check:** test.js line 1347 'a 10 m ring is eight segments', 'ring segments sit on the circle'; "Trench mechanics" 'a ring faces out'

**Existing proof:** P04, P07, P15. Fixtures: `mass-entrenchment.json`, `mass-project-tick.json`.

**Selected-check finding:** Root reserves the status edit to unchanged pending PR 224 to avoid duplication; no gate or test-source change. Existing main has 168 exactordered planner comparisons and project snapshots; ENG-22 additionally has exact 10 m/eight-job circle/front proof.

**Limit:** Reserved for pending PR #224; its strengthened test bytes and qualified local proof are not attributed to this main baseline.

### ENG-23

**Disposition:** in progress; selected-check proof incomplete.

**Behavior:** Strongpoint: a square of 4 trench segments around a, plus 2 wire segments 6 m beyond the side it faces; trenches are dug before wire.

**Source evidence:** `shared/sim.js:499-506`.

**Unchanged Acceptance check:** test.js line 1347 'a strongpoint is a trench square with wire in front'; line 1428 'a closed square of twelve trench cells', 'with two runs of wire in front'

**Existing proof:** P04, P07, P15. Fixtures: `mass-entrenchment.json`, `mass-project-tick.json`.

**Selected-check finding:** The six strongpoint plan descriptors are compared, but the selected completed twelve-unique-cell square plus two complete wire runs is not reached in admission-only strongpoint traces.

### ENG-24

**Disposition:** in progress; selected-check proof incomplete.

**Behavior:** Pattern material and order: line, zigzag and double may be any line fortification (trench, sandbags, wire, traps, mines, fill, clear mines, wall); arc, ring and strongpoint are trench. Segments are handed out middle first.

**Source evidence:** `shared/sim.js:449`, `:508-509`, `:3661-3664`.

**Unchanged Acceptance check:** test.js line 1347 'the middle segment comes first'; "Mass entrenchment is a shared project" 'tank traps too', bridge/nest/ring-wire refused

**Existing proof:** P04, P07, P15. Fixtures: `mass-entrenchment.json`, `mass-project-tick.json`.

**Selected-check finding:** Middle-first ordered descriptors and unsupported-kind refusals are compared. The complete selected tank-traps shared-project execution and bridge/nest/ring-wire refusal set is not bound by the admitted trench-only pattern traces.

### ENG-25

**Disposition:** ported by existing selected-check proof.

**Behavior:** Pay as you dig: the order charges nothing but needs MP for the cheapest segment. Each free digger takes the nearest of the next `crew` segments, rechecks placement and sight, pays the segment cost, or waits for MP. Failed segments are dropped.

**Source evidence:** `shared/sim.js:3651-3679`, `:3317-3329`, `:5382-5383`.

**Unchanged Acceptance check:** test.js line 1428 'nothing is paid up front', 'each digger takes a different segment', 'the diggers wait for manpower'

**Existing proof:** P04, P07, P15. Fixtures: `mass-entrenchment.json`, `mass-project-tick.json`.

**Selected-check finding:** All three selected assertions are Source-compared: no upfront payment, distinct free-digger segments and MP waiting/resumption. No after-admission sight-loss witness is claimed; retain it as a broader coverage limit.

**Limit:** No after-admission sight-loss witness is claimed; it remains a broader coverage limit.

### ENG-26

**Disposition:** ported by existing selected-check proof.

**Behavior:** Projects: allies may join (each pays for what it digs); an unknown or enemy project is `blocked`; a new accepted order detaches a digger, a refused one does not. Once a second a project with no jobs and nobody working or waiting is dropped.

**Source evidence:** `shared/sim.js:3348-3360`, `:3654-3667`, `:5322-5335`.

**Unchanged Acceptance check:** test.js "Mass entrenchment is a shared project" 'an ally can help', 'an enemy cannot join'; "Refused replacement orders leave active entrenchments and paid segments intact"; "Full entrenchment queues cannot allocate unpaid projects or ghosts"

**Existing proof:** P01, P04, P07, P15. Fixtures: `mass-entrenchment.json`, `mass-project-tick.json`, `recipient-engineering.json.gz`.

**Selected-check finding:** 23 Match traces cover allied/enemy/unknown joins, accepted replacement versus refused work, queueFull/partial waits, cancellation/dead workers and ticks 19/20/40/60 recounts; seven full-tick cases cover immediate join, waiters and retirement.

**Limit:** The selected project checks do not establish every socket/whole-match consumer.

### ENG-36

**Disposition:** ported by existing selected-check proof.

**Behavior:** Map structures: every B and = cell becomes a section of a house or bridge structure (each connected span, up to 128 sections per structure). Section hp = cell hp. Material: map override, else bridge stone (stone bridge) or wood, house stone (stone building and landmarks) or wood. House sections are all anchors; bridge sections are anchors when beside dry ground. Supports are 4-neighbours in the same structure.

**Source evidence:** `shared/sim.js:2465-2493`.

**Unchanged Acceptance check:** Seeded map: structure and section list (ids, anchors, supports, materials, hp) identical to JS

**Existing proof:** P09, P19. Fixtures: `structures.json`.

**Selected-check finding:** 59 accepted maps including 33 authored,36 rejections; exact ordered structures/section IDs, anchors/supports/materials/hp, automatic 127/128/129/257 splitting and bridge variants.

**Limit:** Structure initialization proof does not establish every destruction or recipient consumer.

### ENG-37

**Disposition:** in progress; selected-check proof incomplete.

**Behavior:** Authored structures (`map.structures`): kind house or bridge; at most 2048 structures and 1-128 sections each; unique short ids; footprints inside the map matching B or =; positive hp up to 100000; known material; anchor flag; up to 8 neighbouring supports; every section reaches an anchor; bridge anchors touch a bank.

**Source evidence:** `shared/structures.js:3-45`; `shared/sim.js:2468-2476`.

**Unchanged Acceptance check:** Port test: the same maps pass and fail validation with the same message

**Existing proof:** P09, P19. Fixtures: `structures.json`.

**Selected-check finding:** Native invalid-map cases assert MapError::Invalid only. The unchanged selected check requires the same rejection message, which is not compared.

### ENG-38

**Disposition:** in progress; selected-check proof incomplete.

**Behavior:** Building sections: each footprint cell of a base building is a wood anchor section with hp = building hp / cells; the entity's hp is their sum. Entity hp loss is spread to the sections nearest the hit direction each tick; section damage debits the entity once.

**Source evidence:** `shared/sim.js:2494-2503`, `:2617-2628`, `:5306`.

**Unchanged Acceptance check:** test-engine-world.js 'section loss debits the existing health budget once', 'a breach preserves the recruitment identity and queue'

**Existing proof:** P02, P14. Fixtures: `environment.json`, `match-combat.json`.

**Selected-check finding:** Entity/section debit and nearest-direction synchronization are compared, but the selected breach preserving an existing recruitment queue/identity is not asserted by the damage-only projection.

### ENG-39

**Disposition:** in progress; selected-check proof incomplete.

**Behavior:** Support: a live section stands while an undirected path of live neighbours reaches an anchor. When one fails, every unsupported section fails too (impulse carried over, x0.7 across materials). A building dies when no section stands.

**Source evidence:** `shared/structures.js:4-13`; `shared/sim.js:2606-2615`.

**Unchanged Acceptance check:** test-engine-world.js 'a disconnected support cycle cannot support itself', 'the far anchored deck survives'; test-engine-acceptance.js 'the breach and its unsupported dependent fail', 'the independent supported section survives'

**Existing proof:** P02, P14. Fixtures: `environment.json`, `match-combat.json`.

**Selected-check finding:** Static support traversal and damage-chain failure are present, but the full selected actual failed-cycle plus independent supported/far-anchored damage witnesses are not all bound in the runtime fixture.

### ENG-40

**Disposition:** in progress; selected-check proof incomplete.

**Behavior:** Section damage: each contact id hits a section once (last 2048 remembered). Impulse = clamp(damage / divisor, 0.25, 4) with divisor wood 180, stone 400, concrete 500, other 300. While hp > 0 the state is damaged, or burning when on fire.

**Source evidence:** `shared/sim.js:2592-2606`.

**Unchanged Acceptance check:** test-engine-acceptance.js 'duplicate contact cannot repeat collapse'

**Existing proof:** P02, P14. Fixtures: `environment.json`, `match-combat.json`.

**Selected-check finding:** Duplicate nonlethal damage and 2048 FIFO are compared. The selected duplicate contact after an actual collapse is not exercised by the repeated 20-damage-on 100 hp or tiny FIFO hits.

### ENG-41

**Disposition:** in progress; selected-check proof incomplete.

**Behavior:** Section failure: state failed, hp 0, object cleared (the cell opens), fire put out, a `collapse` event with direction and material, a falling debris body (settled at once on overflow), garrison evicted, and on a bridge cell every ground unit on it dies (planes, boats and riders spared).

**Source evidence:** `shared/sim.js:2571-2591`.

**Unchanged Acceptance check:** test.js "Bridge demolition affects the ground units on it, while planes fly above it"; test-engine-world.js 'only failed deck occupants are lost'

**Existing proof:** P02, P14. Fixtures: `environment.json`, `match-combat.json`.

**Selected-check finding:** The selected bridge-occupant fixture contains rifle, tank, rider and gunboat, but no plane. The plane-above-bridge exemption therefore lacks its actual witness; no universal aircraft-constructor refusal is claimed.

### ENG-42

**Disposition:** in progress; selected-check proof incomplete.

**Behavior:** Base building damage: a direct hit does the weapon's vehicle damage when that is 20 or more, else its infantry damage x 0.25. Pillbox: small arms x0.2, hits from behind its slit x2. Allied direct fire does nothing. A blast hits a building once, at its nearest cell's strength (terrain value), nearest cells first, overflow carried on. K cells never catch fire.

**Source evidence:** `shared/sim.js:99`, `:4203`, `:4264`, `:4268-4273`, `:4734-4738`, `:4785-4794`.

**Unchanged Acceptance check:** test.js "Explosives vs base buildings" 'a shell on the HQ does at most...', 'ten shells never set it alight'; test-fortress.js 'rifle fire still chips it'

**Existing proof:** P02, P14. Fixtures: `environment.json`, `match-combat.json`.

**Selected-check finding:** Shared health and local tank hits are compared. The selected HQ per-shell cap, ten-shell nonignition and rifle-chip predicates are not fully bound by the inspected providers. Allied/slit behavior is a broader limit, not an added acceptance gate.

### ENG-43

**Disposition:** in progress; selected-check proof incomplete.

**Behavior:** Damage stages to clients: per-cell state byte: wear in quarters (bits 0-1), burnt (bit 2), damage stage (bits 3-4: under 67% damaged, under 34% nearly gone), scar stage (bits 5-6). Logged only when the byte changes.

**Source evidence:** `shared/sim.js:2349-2361`.

**Unchanged Acceptance check:** test.js "Cover fades with distance and with damage" 'the wall passes through a damaged stage before it falls'; "Shelling short of a crater marks the ground" 'clients are told'

**Existing proof:** P02, P14. Fixtures: `environment.json`, `match-combat.json`.

**Selected-check finding:** World logs/state bytes are compared. The selected wall damaged-stage-to-failure plus client-visible subcrater scarring sequence is not completely bound by the state-prime and trap/house damage snapshots.

### ENG-44

**Disposition:** in progress; selected-check proof incomplete.

**Behavior:** Ruins and rubble: a settled falling house section drops R (keeping its material) on its landing cell and 1.4 m further along the fall, on open ground, road, crater, mud or rubble not under a live vehicle. Bridge sections leave none. A destroyed base building turns every footprint cell into R. R (VBLOCK, COVER) has no hp; only Fill in clears it.

**Source evidence:** `shared/sim.js:175`, `:1103`, `:5154-5163`.

**Unchanged Acceptance check:** test.js line 7728 check 'the falling house throws rubble across the road', 'and rubble stops vehicles'; test-engine-debris.js 'the final obstruction footprint is durable'

**Existing proof:** P02, P14. Fixtures: `environment.json`, `match-combat.json`.

**Selected-check finding:** Landing rubble and durable flags are compared, but the complete selected rubble-across-road and resulting vehicle-route obstruction/expiry witness is not bound to the existing component assertions. Unsupported Fill is retained as a broader limit, not used to narrow the selected check.

### ENG-45

**Disposition:** in progress; selected-check proof incomplete.

**Behavior:** Other destructible cells (no section): hp hedge 60, sandbags 150, wire 40, traps 250, mine 1, woods 200, Field Hospital 150, wreck 300. Destroyed: hedge, wire, traps, woods to open ground; sandbags, Field Hospital, wreck to crater. Crushing vehicles flatten hedge and wire to open ground, sandbags and trench to crater.

**Source evidence:** `shared/sim.js:78`, `:2298`, `:4782-4783`, `:4924-4943`, `:5513`.

**Unchanged Acceptance check:** test.js check "test.js:701" 'tank flattened the hedge'; "Fortifications" 'the tank flattened the wire it drove over'

**Existing proof:** P02, P14. Fixtures: `environment.json`, `match-combat.json`.

**Selected-check finding:** The selected tank-flattened hedge/wire checks require moved crushing, which remains explicitly refused in match_ground_contact.rs.

### ENG-47

**Disposition:** in progress; selected-check proof incomplete.

**Behavior:** Demolition: every blast with a terrain value runs cell damage, falling from full at the centre to half at radius + 2 m. Values in the weapon data: grenade 30, satchel and Petard 600, artillery 90, bombing 400, dive bomber 500, bomber 250, Kaiju breath 250, tank shells their `shellTerrain`.

**Source evidence:** `shared/sim.js:191`, `:304`, `:311`, `:324`, `:340`, `:401`, `:405`, `:408`, `:4741`, `:4754`.

**Unchanged Acceptance check:** test.js "Bombing" 'most of the row is gone'

**Existing proof:** P02, P14. Fixtures: `environment.json`, `match-combat.json`.

**Selected-check finding:** Terrain blast helper and support contacts are compared, but the selected bombing of a row with at least three quarters removed is not established by a lone late-house target.

### ENG-48

**Disposition:** in progress; selected-check proof incomplete.

**Behavior:** Materials: soil, road, wood, stone, concrete, steel, water with hardness, absorption, rebound, crater and fire factors; defaults by object and ground (Q, Y steel; #, B, R stone; H, K, =, T, O, X, A wood; D road; W, F water; else soil). Used for scar depth, house fire damage, section impulse, debris mass and bounce, and projectile contact.

**Source evidence:** `shared/world-layers.js:7-18`; `shared/sim.js:2504-2508`.

**Unchanged Acceptance check:** Port test: `worldMaterial` for every ground and object letter, with and without overrides, equals JS

**Existing proof:** P09, P19. Fixtures: `structures.json`.

**Selected-check finding:** The selected every-ground/object-letter with-and-without-overrides material query is incomplete: current structures materialProbes include neither K nor A and do not form the stated override matrix.

### ENG-49

**Disposition:** ported by existing selected-check proof.

**Behavior:** Road breakup: each hit adds hit / 200 to a road cell's wear; at 1 the road becomes a crater. A crater's depth (wear) grows by hit / 500.

**Source evidence:** `shared/sim.js:4756-4762`.

**Unchanged Acceptance check:** test.js "Ground that wears" 'a bomb turns the road under it into a crater', 'leaves the road out of its reach alone'

**Existing proof:** P02, P14. Fixtures: `environment.json`, `match-combat.json`.

**Selected-check finding:** Exact full snapshots compare road hit/200 accumulation and threshold conversion, crater hit/500, float 32 assignment and untouched out-of-range roads in authored maps.

**Limit:** The selected wear snapshots do not establish every weapon/runtime consumer.

### ENG-50

**Disposition:** in progress; selected-check proof incomplete.

**Behavior:** Scarring: artillery (scar 1) and tank shells (0.25) add hit x scar x ground material crater factor / 600 to the cell's scar on open ground, crater or road. At 1 the cell dents a level (open ground becomes crater) and scar resets; a cell the slope rule holds back stays at 1.

**Source evidence:** `shared/sim.js:156`, `:401`, `:4765-4770`.

**Unchanged Acceptance check:** test.js "Scarring" 'ground beside the shell holes sinks', 'no cliffs from shelling'; "Shelling short of a crater marks the ground"

**Existing proof:** P02, P14. Fixtures: `environment.json`, `match-combat.json`.

**Selected-check finding:** Scarring/dent helpers and some shell contacts are compared. The selected neighbouring-sink, no-cliff and visible subcrater predicates are not fully bound in the inspected providers. Component composition is allowed; these checks need not share one scene.

### ENG-51

**Disposition:** in progress; selected-check proof incomplete.

**Behavior:** Dents and round holes: a dent lowers a cell one level unless that passes -2 or leaves a 4-neighbour more than one level higher. Bombing (4.2 m) and dive bombers (3.2 m) dig round holes: dent every cell within the radius, crater open ground, dent the inner half again except river cells.

**Source evidence:** `shared/sim.js:405`, `:408`, `:2742-2748`, `:2776-2785`, `:5255`, `:5283`, `:5290`.

**Unchanged Acceptance check:** test.js "Bombing" 'each bomb digs a 3x3 patch', 'no cliffs from craters'; test.js "A blast that lowers a cliff makes a previously unreachable region walkable"

**Existing proof:** P02, P14. Fixtures: `environment.json`, `match-combat.json`.

**Selected-check finding:** Bombing/dive hole height grids and dent bounds are compared. The selected postblast formerly unreachable-cliff route becoming walkable is not bound by the independent height and path fixtures.

### ENG-53

**Disposition:** in progress; selected-check proof incomplete.

**Behavior:** Flooding: twice a second, a crater with a W or F 4-neighbour at its level or higher becomes a ford, keeping its wear as depth; water advances one cell per pass.

**Source evidence:** `shared/sim.js:4848-4856`, `:5642`.

**Unchanged Acceptance check:** test.js "Flooding" 'the crater on the bank fills first', 'as deep as the crater was', 'a crater above the water stays dry'

**Existing proof:** P02, P14. Fixtures: `environment.json`, `match-combat.json`.

**Selected-check finding:** W/F equal-or-higher floods and one-cell waves are compared. No recorded soak candidate above adjacent water was found for the selected stays-dry assertion; cadence is implemented.

### ENG-55

**Disposition:** ported by existing selected-check proof.

**Behavior:** Burnt ground: a dead vehicle scorches its cell and 4 neighbours of open ground; fires leave burnt cells; burnt grass cannot burn again.

**Source evidence:** `shared/sim.js:4799`, `:4840`, `:5686-5690`.

**Unchanged Acceptance check:** test.js "Wrecks settle near..." 'the hulk scorches the ground under it'; "Wind, weather, dust and fire" 'burnt ground is marked for the clients'

**Existing proof:** P02, P13, P14. Fixtures: `match-combat.json`, `environment.json`, `world.json`.

**Selected-check finding:** Source wreck-lifecycle retirement compares burnt 42/43/41/62/22 and all five state 4 client log rows; environment expiry and World pending rows retain burnt state. Repeat-ignition broader behavior is not claimed.

**Limit:** The selected scorch/client-mark witnesses do not establish every repeat-ignition or general wire path.

### ENG-62

**Disposition:** ported by existing selected-check proof.

**Behavior:** Laying: a mine is an overlay owned by the layer, over the existing ground and object (a road stays a road). Map mines belong to nobody.

**Source evidence:** `shared/sim.js:742`, `:2695`, `:2731-2733`, `:5411`.

**Unchanged Acceptance check:** test.js "Mines" 'four mines are laid'; test-engine-acceptance.js 'mine placement keeps its base road', 'a mine preserves the separate object layer'

**Existing proof:** P02, P03, P09, P13, P16. Fixtures: `field-works.json`, `environment.json`, `world.json`, `structures.json`, `authoritative-maps.json`.

**Selected-check finding:** Paid mine overlay ownership/road preservation, neutral initial mines and layered hidden-mine material/object probes are exact Source-backed snapshots. Hostile mine contact remains unsupported and separate.

**Limit:** The selected mine-layer checks do not establish hostile contact or detonation.

### ENG-65

**Disposition:** ported by existing selected-check proof.

**Behavior:** Detection: twice a second, rifle, conscript and engineer squads find enemy mines within 6 m (Engineers always, others after 2 s standing still). A found mine is known to the whole team and routes avoid it (+30 cost, no corner cuts).

**Source evidence:** `shared/sim.js:102`, `:3160`, `:4969-4981`, `:5654`.

**Unchanged Acceptance check:** test.js line 7728 check 'a builder squad standing near finds them', 'the team knows them', 'the route goes around known mines'

**Existing proof:** P08, P11. Fixtures: `match-recovery.json`, `pathfinding.json`.

**Selected-check finding:** Source recovery compares tick 9/10, still 1.999/2, Engineer and team masks; Source path fixture changes no-mines straight path 1 point to known 64..67 detour 3 points and no-fine-path corner case. No hostile contact or socket claim.

**Limit:** Discovery, team knowledge and routing are composed component checks, without hostile contact or socket claims.

### ENG-66

**Disposition:** in progress; selected-check proof incomplete.

**Behavior:** Clearing: Clear mines lifts known mines only; any blast that damages a cell removes its mine; clearing restores the surface and its wear.

**Source evidence:** `shared/sim.js:4755`, `:5393`, `:2733`.

**Unchanged Acceptance check:** test.js "Mines" 'a bomb clears the mines'; test-engine-world.js 'clearing a mine does not reset the road damage threshold'; test-engine-acceptance.js 'clearing restores the same road surface'

**Existing proof:** P02, P09, P13, P16. Fixtures: `field-works.json`, `environment.json`, `world.json`, `structures.json`.

**Selected-check finding:** Known-only clearing and blast mine removal are compared. The selected clearing preserving a nonzero road damage threshold is not bound by the zero-wear clearing case; laying alone cannot prove it.

### ENG-67

**Disposition:** in progress; selected-check proof incomplete.

**Behavior:** Vehicle wrecks: a ground vehicle that dies off a MOVE cell leaves a wreck with debris motion. When it settles on open ground, crater, road or mud not under a live vehicle, the cell becomes Q (VBLOCK, COVER, 300 hp, destroyed to crater). Settled hulks give cover like vehicles. At most 40 wrecks; the oldest goes and its Q is cleared.

**Source evidence:** `shared/sim.js:157`, `:2336`, `:2714`, `:5164-5170`, `:5679-5693`.

**Unchanged Acceptance check:** test.js "Wrecks settle near the knocked-out tank and cover infantry behind their final hull"; "A wreck plugs a causeway for vehicles, not for infantry, until it is blown apart"

**Existing proof:** P02, P14. Fixtures: `environment.json`, `match-combat.json`.

**Selected-check finding:** Wreck spawn/settlement/cap state is compared. The selected settled-hull cover and causeway vehicle-versus-infantry route until blown apart are not completely bound by the lifecycle-only fixture.

### ENG-68

**Disposition:** in progress; selected-check proof incomplete.

**Behavior:** Debris motion as rule state: falling sections and wrecks have mass (sections wood 350, stone 1200, concrete 1600, steel 1800; wrecks by type, e.g. Tiger 57000), impulse, gravity 9.81, drag and swept contacts against cells, vehicles and map edges with material bounce. A body settles after 2.5 s, 8 contacts, 4 m travel (wreck 2 m) or when slow on the ground. At most 96 active bodies; overflow is solved and settled at once. Debris never damages or pushes units.

**Source evidence:** `shared/debris-motion.js:1-54`; `shared/sim.js:5125-5183`.

**Unchanged Acceptance check:** test-engine-debris.js 'both hulls become static in bounded time', 'falling section contacts do not crush living units', 'a large collapse obeys the authoritative active body cap'

**Existing proof:** P02, P05, P12, P14. Fixtures: `environment-debris.json`, `environment.json`, `match-combat.json`.

**Selected-check finding:** Mass/motion/settlement limits and active overflow are compared. The selected falling-section contact with a living unit and unchanged health is not bound by the empty-unit overflow and standalone contact callbacks. Durable obstruction is broader scope, not an additional selected check.

### ENG-69

**Disposition:** ported by existing selected-check proof.

**Behavior:** Ignition: a cell hit of 60+ rolls the environment RNG per flammable cell: hedge 0.3, house 0.15, open ground 0.03, woods 0.2. Flammable: hedge, house, woods, or open ground not burnt, wear under 0.5 and wetness under 0.35. A burning house section shows `burning`. Hedges and houses raise one smoke cloud (5 m, 12 s) per stretch of fire.

**Source evidence:** `shared/sim.js:99`, `:4773`, `:4796-4809`.

**Unchanged Acceptance check:** test.js "Wind, weather, dust and fire" 'a client observing the hedge is told what burns'

**Existing proof:** P02, P17. Fixtures: `environment.json`, `recipient-snapshot.json`.

**Selected-check finding:** The fire-order-rain-expiry-and-path provider starts with H at cells 42/43; ignite at 42 produces the exact Source fire key [42,14] after the existing [44,20] entry. The recipient public:current-effects provider receives keys 105/357, includes both for recipients 0/1 and omits 105 for recipient 2; reveal includes both for every recipient. Source sim.js:6050 and native recipient_snapshot/public_state.rs:117-147 apply cell-center visibility to fire keys without a terrain-letter branch. Together these actual provider comparisons prove the selected observed-hedge fire-list predicate as components; no same-scene or socket execution is claimed.

**Limit:** This composes ignition/fire-key production and recipient visibility components. It does not claim a fresh socket scene, every ignition probability or the separate propagation/escape/lifecycle checks.

### ENG-70

**Disposition:** in progress; selected-check proof incomplete.

**Behavior:** Burning and spread, every 0.5 s: burn time hedge 14, house 25, open ground 5, woods 20 s, burning faster by (1 + 3 x rain). Each of 8 neighbours catches with chance spread x dt x max(0.1, 1 + 1.5 x wind along the step) x (1 - rain); spread hedge 0.25, house 0.03, open ground 0.022, woods 0.12.

**Source evidence:** `shared/sim.js:99`, `:4826-4843`, `:5641`.

**Unchanged Acceptance check:** test.js "Wind, weather, dust and fire" 'the fire ran down the hedge', 'rain puts a fire out before it spreads', 'the fire burns out'

**Existing proof:** P02, P14. Fixtures: `environment.json`, `match-combat.json`.

**Selected-check finding:** A two-hedge spread/expiry case is compared, but it is not the selected Source run requiring at least 12 of 14 hedge cells gone. Rain extinction alone does not satisfy all three selected assertions.

### ENG-71

**Disposition:** in progress; selected-check proof incomplete.

**Behavior:** Fire effects: infantry in or garrisoned in a burning cell lose 8 hp and gain 12 suppression per second; an idle one steps out, not along the hedge. A burning house section loses maxHp / 25 per second x (0.2 + 0.8 x material fire) x (1 - 0.7 x rain), through the normal failure path. Burnt-out hedges and woods become open ground; other cells are marked burnt. Visible fires go to observers only.

**Source evidence:** `shared/sim.js:4812-4843`, `:6050`.

**Unchanged Acceptance check:** test.js "Wind, weather, dust and fire" 'infantry in the fire are hurt', 'an idle squad steps out of the hedge', 'an unseen enemy fire remains private'; test-engine-world.js 'fire causes progressive damage before failure', 'fire uses the same support failure path as shelling'

**Existing proof:** P02, P14. Fixtures: `environment.json`, `match-combat.json`.

**Selected-check finding:** Burn damage and escape-path assignment are compared, but the selected squad actually leaving the hedge requires movement; the selected progressive-fire unsupported-dependent collapse is not the all-anchor house fixture. Projection presence alone does not close those checks.

### ENG-72

**Disposition:** ported by existing selected-check proof.

**Behavior:** Wind: angle and strength (0.15-1) random-walk every tick on the environment RNG; smoke drifts at strength x 1.2 m/s; wind drives fire spread and is sent in `wx`.

**Source evidence:** `shared/sim.js:93`, `:773`, `:4858-4860`, `:5907`.

**Unchanged Acceptance check:** test.js "Wind, weather, dust and fire" 'smoke drifts downwind'

**Existing proof:** P02, P10, P17. Fixtures: `weather-runtime.json`, `environment.json`, `recipient-snapshot.json`.

**Selected-check finding:** 59 weather sequences compare wind/clamps/drift bits; environment burn uses wind in exact spread/effect snapshots; recipient fixture wx-rounding compares output wind/rain/wet row.

**Limit:** The selected smoke-drift check and supplemental weather/fire/output components do not establish whole-match draw identity.

### ENG-77

**Disposition:** ported by existing selected-check proof.

**Behavior:** Showers (living rain): `wx` {rain, raining, wet, next}. First shower 180 + rng x 240 s in; showers last 90-180 s, dry spells 240-480 s; rain ramps over 20 s; ground soaks over 90 s while rain > 0.5 and dries over 240 s. Rain weather rains all match on ground soaked from the start; Snow never rains; Mud pins wet at 1; showers still pass in Clear, Fog and Mud. When Rain turns to Mud the rain stops at once. `opts.weather === false` disables it.

**Source evidence:** `shared/sim.js:91`, `:775`, `:806-809`, `:4861-4869`.

**Unchanged Acceptance check:** test.js "Weather" 'a Rain match starts raining on soaked ground', 'when Rain turns to mud the rain stops...', 'no showers in snow', 'in Clear a shower can still come'; "Wind, weather, dust and fire" 'the rain sets in', 'weather can be switched off'

**Existing proof:** P10, P18. Fixtures: `weather-runtime.json`, `match-start.json`.

**Selected-check finding:** 59 sequences/386 steps coverfixed/disabled clear/fog/rain/mud/snow/null, wetting threshold and transitions;63 real createGame starts compareliving first-shower seed/raining/next, including disabledweather.

**Limit:** The selected first-shower/wetting/disabled-weather checks and update branches do not establish whole-match draw identity.

### ENG-78

**Disposition:** in progress; selected-check proof incomplete.

**Behavior:** Wet effects: sight x (1 - 0.2 x rain); vehicles on open ground and mud x (1 - 0.2 x wetness), wetness doubled below level 0, roads untouched; fords x (1 - 0.3 x min(1, wetness)); traffic wear x (1 + 3 x wetness); smoke decays x (1 + 0.5 x rain); no dust above 0.3; open ground will not ignite above 0.35.

**Source evidence:** `shared/sim.js:91`, `:2363-2371`, `:4548`, `:4799`, `:5520-5525`, `:5639`.

**Unchanged Acceptance check:** test.js "Wind, weather, dust and fire" 'wet ground slows a tank at steady speed'

**Existing proof:** P10, P13. Fixtures: `weather-runtime.json`, `world.json`.

**Selected-check finding:** Wet 0.2 drying/Snow movement cases exist. The selected fully wet steady tank/dry-speed comparison is not bound by that different wetness/weather workload.

### ENG-79

**Disposition:** in progress; selected-check proof incomplete.

**Behavior:** One mutation path: every terrain change goes through `mutateWorldCell`, which validates ground, object, mine, material and height (-2 to 4); keeps ground, object and mine overlay separate; updates materials, flags, cell hp, fires, trench front, wear, flood candidates, region and obstruction versions and the cell log. `setCell` maps a letter to the right layer.

**Source evidence:** `shared/sim.js:2650-2663`, `:2680-2740`; `shared/world-layers.js:16`, `:53-66`.

**Unchanged Acceptance check:** test-engine-world.js 'invalid mutations fail before changing any layer', 'migration is idempotent', 'legacy mine and rubble never invent roads'

**Existing proof:** P13. Fixtures: `world.json`.

**Selected-check finding:** Mutation snapshots/invalid changes are compared. The selected migrateWorldMap idempotence and legacy N/=/R conversion check is not invoked by world.json; no new migration criterion is inferred.

### ENG-80

**Disposition:** in progress; selected-check proof incomplete.

**Behavior:** Environment RNG: wind, showers, fire spread and ignition draw from a seeded stream (`g.seed`, the same step as mulberry32) kept apart from combat rolls; Random weather uses its own seed.

**Source evidence:** `shared/sim.js:771-775`, `:2345`; `shared/weather.js:53-58`.

**Unchanged Acceptance check:** Seeded bench run with injected seeds: same fires, wind and showers tick by tick as JS

**Existing proof:** P10, P13. Fixtures: `weather-runtime.json`, `world.json`.

**Selected-check finding:** Independent seeded draws and component weather/fire sequences are compared. The stated same-fires/wind/showers seeded-bench witness is not established by independent streams or the diagnostic whose aligned ticks are zero; whole-match agreement remains pending.

## Test and fixture input hashes

These 33 input paths are a subset of the 664 unchanged entries. The recorded execution and baseline use the same Git blobs. SHA256 values identify the complete file bytes, including compressed fixture bytes.

| Input | Git blob | SHA256 |
| --- | --- | --- |
| `rust/crates/core/src/match_engineering_tests.rs` | `2a01d4d62fe10b7bb7cd9917b10e5f805ccba240` | `5a9659b0c9f61d1f01c1c695926dcc92c920ecd80b5aab46153ca96ee93be4cb` |
| `rust/crates/core/src/match_projects_tests.rs` | `539c6b28aa2ca83591e5bbe86d31a3677c8a33fa` | `ae5a39bd6254f1dd1ab863765317d1e59e715aa99a6d0bf764e1bb0f7cf7dbe8` |
| `rust/crates/core/src/match_recovery/tests.rs` | `6ae080991e7fb125960d30c5547ed7d6ce777d30` | `791f76e5dee33810076cb324fdf385c7fbd3e21dd4e4d5e2af2c30bcc7128463` |
| `rust/crates/core/src/recipient_snapshot.rs` | `13e4ec6aedba2a085739c23ab4e00f340e661a8d` | `d0350f971b48b1c72e43500894bcf2ccff9ee0bbda3c18e2c5b418f66fcb1c78` |
| `rust/crates/core/tests/authoritative_maps.rs` | `e6650824b63aabf0be419ff91405c05146893c4b` | `95ceb77cb01265e0f9bedb5675de9fb39d1af05179430c8fac8b08d70146ad17` |
| `rust/crates/core/tests/environment.rs` | `f468b0e7f42c2e448a062a7bb34d802b3ded8318` | `bded7ef9fa4de4b393e9cbcd83920272f9d5058bb45f89ff5b3126aee5046ca4` |
| `rust/crates/core/tests/environment_debris.rs` | `9ec4eecb6fd7fd0125061ac8e02da66e8708151e` | `c8c5e572f1a696667ddf907bd76b20c32cd5f117b81b9e7c3d1a7e94555639a8` |
| `rust/crates/core/tests/field_projects.rs` | `a7f71e5159539be1b414b1de803bb8653e079400` | `e90e0d23144017827576ec300bd40b1626d396610e9bb635043b42416eec18b6` |
| `rust/crates/core/tests/field_works.rs` | `aee4b66ff18e0f2bd44c0ace77c95f3841f0bc5d` | `2a03d22662b35b40734111ba6c0f4456c7bac1ead1424bcd44a5c341dbf8cdaa` |
| `rust/crates/core/tests/fixtures/authoritative-maps.json` | `6e0d518e5daeeffe3743271b0b868a728f94f107` | `7cfc9bd78df01e2be623357a0a84429511220e914ffe01c2baa67e4e6012b5b8` |
| `rust/crates/core/tests/fixtures/environment-debris.json` | `3cf3e7a2229fa1c74924ccaf735351c325b44e2f` | `d8034cfdb68b42371b8d08715ccb49e05ab0892e329a1cd3df4d99342b355c8c` |
| `rust/crates/core/tests/fixtures/environment.json` | `4a98fe9ffbcecc4d97aa72da91f9397a6619aae4` | `7c8bf29a1405fcba3805ad7c4cc431d78ba607b0a759fafd98d4f94715f1232f` |
| `rust/crates/core/tests/fixtures/field-works.json` | `c6ffd96c026f140473d1a4f4fb3e711eb5c2f651` | `4ec000d0ee800463ba0cec64dabab30dd70d891c1b2270b2e3fac01db40a354b` |
| `rust/crates/core/tests/fixtures/mass-entrenchment.json` | `43fd441b8572eff6b118ee50c723bbda473b930a` | `4c4c4d49ed8046155c1931ee4d6f1c23cb45c7f23c6333d1763d628521728569` |
| `rust/crates/core/tests/fixtures/mass-project-tick.json` | `277458da42e0af9f11ba180a2f4df55b4b4c31f4` | `3dca1cc8d26ec913e79d0187361541822f01dbfc87b13111ab9a1c67b90ad8d4` |
| `rust/crates/core/tests/fixtures/match-combat.json` | `057719fa7ad1a9c44e023c43f9eec7299857ca68` | `6dcda8411c11156ee1b6d5ec3037a3e758c7a37dd76db48ac967fc0387a5b03a` |
| `rust/crates/core/tests/fixtures/match-engineering.json` | `c95370838f50dc4e40f01a4eafdd7a9007e74d3a` | `7df959cdff2bd07058a391c53161902edadcf3e3f23cd14d50e864aa5c4194b8` |
| `rust/crates/core/tests/fixtures/match-recovery.json` | `a057de6f8b750ff08ad1b721772792d973843901` | `519da3f95c70874457f5ae62391abe3e455d8c16d88a8af789cca507c33a954c` |
| `rust/crates/core/tests/fixtures/recipient-snapshot.json` | `c610a276ad5ebf0710f81b571843ceddf43ed149` | `fb3a0e27f7a124b410ebc665b19d44df474ab74eff23674d9bf5fd934d26ce92` |
| `rust/crates/core/tests/fixtures/structures.json` | `411c212fee5553380bb6346967d59f0c52fb6a59` | `4eeb7966ce557bcc3bb5fa5e44bbac832b55a97017d5314c4e7cd2a10d64727f` |
| `rust/crates/core/tests/fixtures/weather-runtime.json` | `0a1d7524737bc57d56f80170bc997b1e10950b73` | `9313c99a3bf61eab9e356a529900cadcca8a25ebddd1ea62fdea7805f6b776a5` |
| `rust/crates/core/tests/fixtures/world.json` | `dbb8ed8c3d7151d43ba4f9c5693a175fd87d7464` | `0eb6e18e225543480d7934681da3f0cfcfdfadebaa7d0c8758127f1fb03c06f2` |
| `rust/crates/core/tests/match_combat.rs` | `e465f196d981396951ecf015d39a1efdfd372096` | `48aab1b231ae60edac7f4eafedd3292ec9639fd1e84f9589fdfcf76afff6561f` |
| `rust/crates/core/tests/structures.rs` | `30e99ad8fff5e802e37ce8fdc00de14ccf5fca3c` | `a0aa41fee245e79abc569edfbe209b83d5baa0eae63fa1ee33a75db3cded0e3f` |
| `rust/crates/core/tests/weather_runtime.rs` | `ac71b6974592409570b6c2938cb1b5c566471f13` | `0e234ba52a7bc47ebda69c02a36aa84f504e33ee10a30702fe68c31fbfeb47e3` |
| `rust/crates/core/tests/world.rs` | `fea9a08aae94890f883c6ecb8c509c3f812edf09` | `c016b0cf34b34d0f9e6182de94103d01bec37419e2c5949a383785f5eea6d561` |
| `rust/crates/simtest/tests/fixtures/match-start.json` | `c61a9b1009d23ca580823e2440de9f217f20d685` | `473b8cb0bb4fcca64ba47edb8d3810274501a49e2b90c2f5d8a664ad03e86229` |
| `rust/crates/simtest/tests/fixtures/pathfinding.json` | `3438176166d98e8151815afb17b2be76c6f1dfa5` | `8f7d687451091b420ff6b5ebf194dba13ec7d2f96585e247bed6b860d8d26aa2` |
| `rust/crates/simtest/tests/fixtures/recipient-engineering.json.gz` | `2dc9c0b9ab959cb430a7b726a858acc1dd286112` | `073a07adcf8c3c0436a1ab7f99f53c40bae1285f5e37fd9a7fe485f194039f3f` |
| `rust/crates/simtest/tests/mass_project_tick.rs` | `eeb2a5e50e3a06c816f472ea0fd6f9b1f7d9beeb` | `f18fbd838ab785c8f682fb3947ea0f5d1b92837b138e7f624bb1c2b95fc9581a` |
| `rust/crates/simtest/tests/match_start.rs` | `7abeb3d10a1c4500a29e2f4bcad5da767ecb2ac4` | `e8d4a9057f945894359cf1fb13c5688cac292f888ee6799fdc918e617ac4e1ce` |
| `rust/crates/simtest/tests/pathfinding.rs` | `bf4d170529d7da16229f231972c3aa6dd67ee60c` | `715afc9ccf98126e5a2768a04f9e792e2f6299769c83fcc98f514f8b81183912` |
| `rust/crates/simtest/tests/recipient_engineering.rs` | `1f3b8173c68a3bb0e71b4b74cb0429ea39b90e56` | `03bff592a003e6a8216442bc7e2766892304115235d4b7e648b9b89180aca73f` |
