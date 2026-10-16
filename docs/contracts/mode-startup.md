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
`sim.js:6032`. Complete recipient snapshots now admit the matching Classic, Annihilation and
Assault option/game pairs as well. Classic retains its public nodes and per-team
ghosts from construction-production. Horde and Tutorial remain component results
in this stream; their hosted lifecycles belong to separate changes. World
projection and admission remain closed.

The lobby retains its existing seven-name whitelist (`server.js:659`) and
`spawns` seat-count field (`server.js:336,345`). The shared start-readiness
predicate requires one through capacity players and, for Assault, at least one
player on each side of the selected defender team (`server.js:332,682`). The
trusted R1 Start path admits Conquest, Classic, Annihilation and Assault within
its existing private, two-human, opposing-team profile. Default remains the
internal Conquest alias, not an eighth lobby name. The mode whitelist and
host-only Start/Restart follow `server.js:659-686`; ordinary constructors follow
`shared/sim.js:682-689,786-861`. Assault uses the selected defender team.
World, Tutorial, Sandbox and hosted Horde stay outside this admission.
No lobby field, message shape or version changes.

Each admitted mode reuses the existing Start envelope and full recipient path.
Public mode fields follow the table above. The existing result object preserves
the actual mode name, uses `{t, vp}` samples for Conquest and
`{t, points, structures}` samples for the other ordinary modes, and keeps
`winVp` null for non-Conquest modes, following
`shared/story.js:52-70`. Save/load retains the version 1 envelope and canonical
mode owners; loading revalidates the matching mode, bound map, startup layout,
every seat and complete snapshots before publishing any room state. The save
content and its private native representation remain documented in save-format.md.
The narrower aircraft, logistics and command capability guards remain in force.

Exact component rows and absence rules are checked by
`mode01_non_world_mode_rows_match_the_prototype_public_fields`,
`mode01_horde_row_omits_unset_profile_boss_and_active_next_profile` and
`mode01_mode_rows_round_timers_and_refuse_invalid_values_or_mode_pairs`.
`map06_lobby_map_selection_reaches_conquest_layout_and_every_recipient` checks
that both live recipients keep the absent mode field through ordinary ticks.
