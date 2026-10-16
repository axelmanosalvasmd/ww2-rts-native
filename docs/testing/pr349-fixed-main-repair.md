# PR #349 fixed-main repair

This repair merges `origin/fix/main-elimination-serde-20261016` at
`4edfcd5e8b795aa47fde114de7d51d7eb6430078` into the existing #349 head
`627724fa71f0a396270eb77b9bcf1d89a52cd150`. The branch remains
`runtime/cmb-batch6-20261016`. It includes #337 at
`7d777289cba44bb1a6573a9522487efbea9d462a` and still depends on that PR.

The author checkout was not attached to this branch. The clean remote head was
attached to the dedicated `fix-349` worktree. No author worktree was edited.
All verification runs on the Vostro under the shared Cargo slot protocol.

The task-start issue #5 readback includes coordinator comment 6213339263:
#349 follows #337 and its runtime ack is deferred. Hermes comment 6213385678
accepts AI-33 in #351 only and explicitly leaves AI-38 with #349. This repair
addresses their dependency and scope requests, without posting to the issue.
The six approved ABI 2 calls remain authoritative; broader contract drafts
remain drafts. Reference `e4e579a` is unchanged.

## Hand-edited merge conflicts and repairs

Every manual conflict hunk and additional source repair is recorded below.
Line references describe this commit's files. Git's other incoming files and
test registrations merge without manual edits.

- `DESIGN.md:1`: retain the complete #349 medic/house-shelling and inherited
  #337 FireAt sections, followed by every incoming fixed-main section. No
  historical text or scope restriction is removed.
- `rust/crates/core/src/commander_tactics.rs:1338`: retain the chores test
  module and all incoming batch7, shopping-input, regression and IF-25 modules.
  The planner body merged automatically and keeps both sets of behavior.
- `rust/crates/core/src/match_commands.rs:779`: retain bounded Group validation
  and incoming Rail/Enter/FireAt finite-coordinate validation in one match.
- `rust/crates/core/src/match_commands.rs:1641`: retain Group and incoming
  command variants, including Attack, in the terminal unreachable arms.
  Attack keeps its inherited mixed-aircraft/ground dispatcher above this
  match. No attack guard is removed.
- `rust/crates/core/src/match_firing.rs:2677`: retain the ground fire_at runtime
  and fire_at_tests modules plus incoming Kaiju test registration.
- `rust/crates/core/src/recipient_snapshot/composition.rs:108`: retain both
  projected flights and projected aircraft, defaulting only missing entries
  to empty arrays. Shots retain the caller-owned batch adapter.
- `rust/crates/core/src/recipient_snapshot/composition.rs:186`: retain #337's
  canonical firing validation and every salvo bound. Retain main's aircraft
  owner admission and aviation-event refusal. All other actor, logistics,
  scenario, producer-buffer and grenade guards remain intact.
- `rust/crates/core/src/match_commands.rs:75`: remove the automatically merged
  duplicate FireAt enum variant, retaining main's identical shared variant
  and serde derives. Keep Group as the inherited internal compound wrapper.
- `rust/crates/core/src/match_commands.rs:515`: keep main's single finite
  Rail/Garrison/FireAt JSON parser and remove #337's redundant FireAt parser.
  Queue and coordinate semantics are identical.
- `rust/crates/core/src/match_commands.rs:979`: route ground-only FireAt to
  the inherited ground owner; selections containing an owned aircraft
  continue to main's unchanged aircraft area owner. Its mixed shell-capable
  ground/air Unsupported refusal and read-only preparation remain unchanged.
  Remove duplicate FireAt unreachable patterns from automatic merging.
- `rust/crates/simtest/tests/recipient_effects.rs:3`: document the additional
  live ground-flight comparison in the test's scope comment.
- `rust/crates/simtest/tests/recipient_effects.rs:570`: update the stale launch
  refusal comment and assertion. The actual #349 hosted log at run
  38885111811 fails the expected Unsupported assertion, because the inherited
  #337 adapter now supports live ground flights. Compare complete recorded
  shots and live-flight rows for every seat instead. The new launch frame
  adds one frame, five recipient outputs, four shot rows and one supplied
  event. The final count assertion at line 650 becomes 159/795/94/25. Keep
  the source fixture, numeric-bit
  comparisons, random-state preservation, empty-selection atomicity and all
  separate malformed-batch refusals unchanged. This is a deliberate replaced
  behavior test, explained in the changelog fragment.
- `rust/crates/core/src/match_firing/fire_at_tests.rs:454`: add a merge
  regression proving mixed tank/bomber FireAt refuses Unsupported without
  state writes, bomber-only FireAt still sets an Area mission, and tank-only
  FireAt sets the ground cell without changing that aircraft mission.


## Scope and overlap

AI-38 maps literally to the registered `commander_tactics::chores_tests` tests:

- `ai38_medics_follow_the_ground_army_center_beyond_twenty_four_meters`
- `ai38_house_shelling_requires_a_watched_garrison_strictly_within_sixty_meters`
- `ai38_house_shelling_uses_contact_id_order_and_keeps_the_prototype_chore_precedence`
- `ai38_busy_fighters_keep_their_existing_work`
- `ai38_planned_chores_survive_ordinary_ticks_all_seat_snapshots_and_match_finish`

Prototype `shared/ai.js:593,673-681` computes the ground army mean and performs
medic and house-shelling chores before the general busy check. The source was
read at the unchanged prototype pin. The lifecycle test runs ordinary ticks,
every seat's snapshots, victory and ending ticks. No new Match field is added.

#337 supplies the ground FireAt actuator and flight projection. Main supplies
current aircraft area orders, actor lifecycles, tick guards and serde support.
These are overlapping owners, not duplicate row credit. #351's AI-33 ack does
not accept this AI-38 row. #322 and #375 concern full logistics and do not
supply the medic or shelling planner. Their work is not imported or enabled
here. CMB-150 remains dropped without World recovery, sandbox spawn and convoy
supply/resume integration; CMB-146 remains dropped without rail dispatch/tick
and complete recipient delivery. LOG-05/08 remain dropped without all firing
families and full logistics tick/snapshot evidence. No parity status is moved.

## Validation

The first local focused attempt ran no tests: the added mixed-selection
regression used MatchCommands with Debug formatting, which that type does not
implement (E0277). The regression now compares its existing serialized state;
no production type derive was added.

Focused final-tree checks, formatting and strict workspace all-target Clippy
are pending. No whole-workspace test result or hosted-check success is claimed.

merges after #373; needs fresh Hermes ack
