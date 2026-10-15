# Private MOV terrain companion, paused before execution

Historical paused checkpoint40229feb. The resumed source and current gaps are in
[the consumer readiness plan](mov-speed-consumer-readiness.md). The original
failure finding below remains as history; no execution is implied.

Based on exact fixed main396f. Root's priority PR256 rebase interrupts this draft.
No Source/Node/Cargo/rustc/native/fmt/discovery ran. No fixture exists yet.

The authored matrix has12 scenes: grass, road, partial road, shallow/deep mud,
rifle mud/grass, churn reference/half-churn, hill/flat pairs and dry-to-wet.
The three registered native functions are
`mov_terrain_companion::original_roads_and_mud_distances_include_partial_road`,
`mov_terrain_companion::original_half_churn_and_wet_ground_distances`, and
`mov_terrain_companion::original_uphill_and_downhill_distances`.
They preserve the original40+40,40+20, paired40 and800 rain-tick windows and all
nine selected distance inequalities/negative controls. Additional dust, smoke
and rain assertions retain their existing providers. These are authored test
sources, not compiled or passing tests.

Known unfinished setup: both new recorder and consumer currently require a
constructor engineer to build the Motor Pool. Pinned Source sim.js:159 starts
ordinary Conquest with rifle/rifle/MG; native match_runtime.rs:421-435 chooses
the same CFG.startForce. The exact builder-policy provider must be inspected
before replacing that invalid lookup with an actually supported command actor.
No direct actor insertion, footprint removal, maker stub or expected-state load
is permitted. This is a static setup finding, not an executed native refusal.

The original recorder is retained unchanged fromd1e426dd and writes a separate
raw packet. The companion recorder requires that raw packet and binds its hash;
it cannot stand in for original cleared/negative-producer admission. Both keep
the first failure. The native fixture is read at runtime and remains absent.

Pending Source commands, only after the draft is completed and Root grants a slot:

```sh
WW2_REFERENCE=/home/judiazm/Projects/ww2-rts-native/reference/ww2-rts /usr/bin/node24.21.0 harness/mov-owned-speed-checks-draft.mjs /tmp/mov-owned-speed-original-checks.json.gz
WW2_REFERENCE=/home/judiazm/Projects/ww2-rts-native/reference/ww2-rts /usr/bin/node24.21.0 harness/mov-terrain-companion-fixture.mjs /tmp/mov-owned-speed-original-checks.json.gz /tmp/mov-terrain-companion.json.gz
```

Future native selection is `test(mov_terrain_companion::)` (three unique
instances) plus the existing core and simtest registration guards (two more).
No current grant exists and these commands have not run. Keep every incoming
fixture and parity cell unchanged. MOV08's complete World distance consumer is
not authored or counted here. MOV09 stays paused and MOV72 stays held.
