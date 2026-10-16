# Research across native modes

This runtime batch covers the ordinary-mode clauses of BASE-49 and BASE-50.
It also connects the existing canonical Horde research wrapper to shared defender
labs. World admission, Horde full ticking and hosting, and Unreal remain outside
this batch. No FFI, header, wire field or research cancellation API changes.

## Prototype reading

The prototype is read-only at `e4e579a`.

- `shared/tech.js:14-25` defines tier and Armory prices, durations and construction
  Munitions charges.
- `shared/sim.js:387` gives Horde defenders shared production access and excludes
  the attacker. `:1053-1064` counts finished accessible labs, advances jobs in their
  recorded order, and retains progress when labs disappear.
- `shared/sim.js:799-800` enables technology for ordinary research modes and gives
  Horde defenders tier 2 and its attacker tier 3. Tutorial disables technology.
- `shared/sim.js:3723-3736` admits paid research, enforces duplicate, tier and
  capacity denials, and records the charge. `:5321` runs the early research phase.

## Literal row mapping

The module is `rust/crates/core/src/economy_research_tests.rs`, registered from
`lib.rs`. Its shared Match setup supplies ample wallets and canonical finished
building factories. Armory checks explicitly initialize player technology to tier
3 and preserve each existing building's production row; they exercise the real
request, tick and recipient paths from that stated setup.

| Row | Test | Covered intent |
| --- | --- | --- |
| BASE-49 | `base49_ordinary_mode_tiers_use_live_hqs_keep_progress_and_survive_hq_loss` | Default, Conquest, Assault, Annihilation and Classic: unfinished HQ denial, both tier prices and durations, Classic Munitions denial/payment, duplicate and max denial, retained progress without a finished HQ, same-HQ skirmish recruitment, and player tiers retained after real HQ retirement. |
| BASE-49 | `base49_classic_same_hq_trains_while_tier_research_is_pending` | A real rifle training job completes from the selected Classic HQ while that player's tier job is still pending; tier progression then finishes. |
| BASE-50 | `base50_ordinary_modes_enforce_armory_access_capacity_prices_and_retained_progress` | All five ordinary modes: foreign Armories do not qualify, unfinished labs do not add capacity, one line per finished own lab, all three lines and levels charge their listed prices, Classic Munitions denial/payment, scarce-lab order retains paused progress, resumption and max denial. |
| BASE-50 | `base50_ordinary_armory_next_level_requires_matching_player_tier` | All five ordinary modes reject level 2 at tier 1 and level 3 at tier 2 without changing the wallet or jobs. |
| BASE-49, BASE-50 | `base49_base50_horde_research_uses_shared_defender_labs_and_private_progress` | Canonical Horde component admission: both defenders use the single shared HQ and Armory while their own jobs, charges and levels remain separate; the attacker cannot borrow the defender Armory. Early-phase pause and resumption complete both defenders' jobs. Full tick admission remains guarded. |
| BASE-49, BASE-50 | `base49_base50_tutorial_disables_research_without_charging_any_seat` | Every Tutorial seat rejects all four research kinds with no wallet or job changes, then continues ordinary ticks. |

Ordinary tests advance through `Match::advance_tick`, collect a recipient snapshot
for every seat at each setup or progression transition, and compare its tier with
the canonical player tier. Pending research, paused labs, completed levels,
Classic training and HQ retirement all reach snapshots. Tutorial snapshots omit
technology. Horde's explicit component check uses `step_research`; it does not
claim a full Horde tick or recipient lifecycle. The World clauses remain deferred
under the milestone's World admission decision. These limits must remain visible
when assessing either mixed row.

## Validation

Focused nextest selector: `test(economy_research_tests::)`.

Execution and final-head validation belong to the parent stream's shared Cargo
slot run. This subtask has not run Cargo and makes no local PASS claim.
