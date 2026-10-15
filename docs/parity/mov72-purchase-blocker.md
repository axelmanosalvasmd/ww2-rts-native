# MOV-72: canonical purchase setup boundary

Source-only inspection at `f7d9ada0028a4f65e67b3f312db5751989d852fd` found
no supported native adapter for the complete original cleared transport scene.
The four prepared checks remain byte-identical. No Source, Cargo, rustc or native
job ran, and no runtime or admission rule changed.

## First unsupported setup operation

Original `test-transport.mjs:9-15` creates ordinary Conquest with weather off,
calls `clearFixtureUnits`, sets the buyer's MP to 1,000,000 before each purchase,
and purchases tank/rifle/rifle/MG before assigning the four authored poses.
`test-fixtures.js:22-34` clears the original actors, building footprints and
structural links. Its `fixtureCommand` then inserts temporary producer ID `-100`
for owner zero, calls the real Source command, and removes that producer in
`finally` (`test-fixtures.js:5-20`). The producer is a fixture object without the
canonical building footprint, structure and metadata required by native Match.

Native `Command::Buy` delegates to `recorded_buy`
(`rust/crates/core/src/match_commands.rs:903-934`). Both `recorded_buy` and public
`recruit` use `RecruitParts::stage`; neither accepts independent fixture-producer
facts (`match_recruitment.rs:232-250`, `321-335`, `339-379`). Production selection
iterates the actual roster, and no selected maker returns
`production::Error::Denied(Denial::Needs)` (`production.rs:462-490`, `620-627`).
The command adapter maps that denial to `match_commands::Denial::Needs`.
This is a source-derived result, not an observed native failure.

Adding a producer only to production metadata does not solve this: the maker
must exist in `game.units`, have a `building_rallies` entry and have canonical
production state (`match_recruitment.rs:375-390`). `grid::UnitId` is `u32`
(`grid.rs:15`), so the Source fixture's negative ID has no exact representation.
An arbitrary positive replacement would change the roster/identity and still
need authentic building state. No such replacement was introduced.

The available construction route is `Match::execute_build`. After the original
clear there is no builder to select, so it reaches the empty-crew
`Denial::NoBuilders` branch (`match_construction.rs:798-826`). Restoring only a
builder or calling `initial_unit` to obtain the four test actors would bypass the
requested purchase proof. The current `match_carrier_tick` helper uses exactly
that direct initialization path (`tests/match_carrier_tick.rs:182-202`); its
component evidence remains valid within its stated scope and is not reused as
this missing consumer.

## Existing downstream consumers

These APIs exist in source, but this checkpoint has not reached or executed them:

- `execute_command` dispatches Board and Unload to `carrier_board_command` and
  `carrier_unload_command`; Move uses the existing ground command path.
- `advance_tick` invokes `apply_unit_riding`, then the represented movement and
  `apply_unit_boarding_with_routes` paths (`match_tick.rs:557-563`, `598-606`).
  A future consumer must run exactly 100 ticks before the first cargo check and
  40 more after Move. It must not replace those runs with authored pose changes.
- `SnapshotCache::new(&match).recipient(0)` supplies the owner's real unit rows.
  `recipient_snapshot.rs:293-297` emits CARGO_FLAG `524288` and cargo length shifted
  by 23. Its `CargoCount` refusal applies only above three riders (`272-273`),
  which does not rule out this two-rider predicate. These are implemented selected
  fields; the projection still reports other missing capabilities.
- `carrier_unload_command` clears each rider's carrier link and riding state and
  removes it from cargo through its own checked placement (`match_carriers.rs:461-541`).

The companion predicate map lists all ten original assertions at
`test-transport.mjs:21-34`, all four setup purchase assertions, their timing and
intended consumers. No assertion or original command was removed. No row becomes
ported from this source inspection.

## Canonical setup proposal for Root review

Keep the complete original recorder/check unchanged as the original evidence.
Prepare a separate canonical companion only if Root approves this setup change:

1. Construct Source and native Conquest on the original 80 by 80 map, with the
   same two names, teams, factions, spawns and no points. Preserve every constructor
   actor, building footprint, structural section, rally and production record.
   Use weather off, tech off and logistics off; retain Source's default supply
   setting. Bind all actual native options explicitly and record original seeds.
2. Set owner zero's MP to 1,000,000 before each of four purchases, as the original
   helper does. Use the real positive constructor producer IDs through raw Source
   `command` and native `execute_command`/`recorded_buy`. Assert each admission and
   resulting publication, wallet debit, story spending, control, combat, firing,
   ability and recovery owner. Never substitute `initial_unit` or a pre-paid job.
3. Apply only the original four authored pose/path/order/target/hold-fire inputs
   after each successful purchase. Track the resulting purchased IDs by aliases.
   Do not discard constructor units or erase terrain. Preserve default reactions
   and auto-retreat rather than silently making the entire roster quiet.
4. Replay the original Board/100 ticks/cargo recipient checks/third Board refusal/
   Move/40 ticks/Unload sequence with every original assertion unchanged. Compare
   Source and native through the real command and runner APIs, then consume the
   real owner row at index 12 for both flag and count. No synthetic row is allowed.

This changes the original helper's initial roster and terrain, producer identities,
population, vision and possible interactions. It is not an exact replay of the
cleared original fixture and is not preapproved acceptance. Source must record
the companion under its own honest name, the original scene must stay present,
and any new failure must be retained. Root must decide whether this disclosed
canonical companion satisfies MOV-72's intended check or whether an explicit
fixture-purchase adapter is required. An adapter design must preserve canonical
publication/admission and cannot simply admit the negative fixture object.

The four prepared tests and full 67-row map remain in this branch. This checkpoint
adds the blocker map and execution-policy update only. Root owns the next setup
decision and separate reviewer; no approval is inferred from this proposal.
