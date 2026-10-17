# Mode startup component rows

Baseline: prototype `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`. Owner group N
permits the prototype shapes, confirmed in issue #5 comment 6218552386.
This document describes the existing JSON WebSocket v0 `mode` field. It adds no
ABI, header or version change.

`recipient_snapshot::mode_row` projects the public mode component from canonical
Match state. It follows `shared/sim.js:5895-5898`; the same row applies to every
recipient. It reads no hidden units, plans or per-seat facts. Missing prototype
properties remain absent in the mode objects. The component uses null as the
no-mode sentinel for Conquest; its JSON snapshot omits `mode`, as in the prototype.

| Mode | Public row fields |
| --- | --- |
| Conquest | No mode object; component null sentinel, including the native Default alias |
| Assault | `kind`, `defenderTeam`, `attackerTeam`, `timeLeft`, `suddenDeath: false`, `total` |
| Annihilation | `kind`, `suddenDeath`, `bunkers` |
| Classic | `kind`, `timeLeft`, `suddenDeath` |
| Horde | `kind`, `defenderTeam`, `attackerTeam`, `timeLeft`, `suddenDeath: false`, `wave`, `left`, `active`, `slot`; `nextProfile` only during a break; `profile` and `boss` only after wave zero |
| Tutorial | `kind`, `suddenDeath: false`, `slot`, `goal`, `step`, `steps` |

The timer is `max(0, ceil(timeLeft))`. Native projection refuses a nonfinite
timer or inconsistent option/game mode pair. Source setup fields are assigned
at `shared/sim.js:786-795,834-835,859-861,1679` and
`shared/tutorial.js:103-106`. Horde has no `profile` or `boss` at initial setup;
wave activation sets the profile (`sim.js:927-928`) and the active update sets
`boss` to the health pair or null (`sim.js:955`). Neither a hidden prepared
profile during an active wave nor private Horde budgets enter the row.

Ordinary Conquest recipient snapshots keep omitting `mode`: `modeRow` returns
undefined for absent `g.mode`, so JSON serialization omits the property at
`sim.js:6032`. The other five mode rows are component results;
their complete recipient snapshots and server room admission retain the existing
guards. World projection and admission remain closed in this stream.

The lobby retains its existing seven-name whitelist (`server.js:659`) and
`spawns` seat-count field (`server.js:336,345`). The shared start-readiness
predicate requires one through capacity players and, for Assault, at least one
player on each side of the selected defender team (`server.js:332,682`). The
trusted R1 start path also retains its narrower two-human Conquest policy.
No new lobby field or public Start capability is added.

Exact component rows and absence rules are checked by
`mode01_non_world_mode_rows_match_the_prototype_public_fields`,
`mode01_horde_row_omits_unset_profile_boss_and_active_next_profile` and
`mode01_mode_rows_round_timers_and_refuse_invalid_values_or_mode_pairs`.
`map06_lobby_map_selection_reaches_conquest_layout_and_every_recipient` checks
that both live recipients keep the absent mode field through ordinary ticks.
