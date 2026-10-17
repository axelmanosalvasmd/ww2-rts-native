# Mode coverage driver (round 16)

Plays every reference mode end to end with the Unreal client against an isolated copy of the pinned reference server,
and writes one row per mode: started, objectives shown, end condition reached, end report shown, snapshots adopted and
refused under the Rust decoder, the Rust view shadow's agree counts, crashes.

| File | What it does |
| --- | --- |
| `modes.json` | one entry per mode: the lobby's Mode and Map values (the messages the reference lobby sends: `{t:'mode', v}`, `{t:'map', name}`), AI seats, teams, whether a second native client takes a seat, whether the natives play (`-TCCapturePlay`, `-TCTutorialDrive`), the time cap and the mid-match capture times. The authored scenarios (`shared/scenarios.js` `migrateScenario`: the maps with legacy triggers, El Alamein, Hurtgen Forest, Prokhorovka, Seelow Heights, The Great Bridge) are Conquest matches on their maps |
| `run-mode.sh <id> <out> [room]` | one run: a rendered native host (English) makes the room, a headless native guest (Spanish) takes a seat as a player, the host sets mode, map, AI seats and teams through the lobby's own requests and starts; both clients run with `TC_DECODER=rust`; the run ends with the match (the clients leave 6 to 8 s after the result lobby) or at the cap |
| `summarize.mjs` | reads the run's logs into `row.json` and a markdown row; `--table <dirs...>` joins rows into one table. The engine writes CRLF line ends: lines are split on `\r?\n` |
| `summarize.test.mjs` | `node --test unreal/Tools/modes/summarize.test.mjs`: the reader against a synthetic engine log (CRLF, coverage lines, fallback to the objectives strip, cap and crash columns) |
| `collect.sh <shots dir> <table.md> <runs...>` | one table from chosen runs, and their captures copied as `modes-<mode>-<what>.png` |

`cmdlines.txt` also names the module the run played (`module: <mtime>, <size>, md5 <12 hex>` of
`libUnrealEditor-ThreeCrossroads.so`), so runs made while the project was rebuilt say which binary they used. A mode's
`"guestRender": true` renders the Spanish guest too (its captures carry `-es`); the Tutorial does, so both seats' step
captures exist. With `-TCTutorialDrive` each seat logs `tutorial drive totals: seat N, step K: sent X (type n, ...)` at
every step change and at the end: the seated guest walks the Tutorial with its own orders, not as a watcher.

The row's evidence comes from the clients' own log lines: `start #`, `objectives: tick ...` (`-TCObjectivesLog`),
`result lobby: ...`, `rust view decoder (...)`, `rust view shadow totals (...)`, and the mode coverage lines
`modes: start|strip|goal|say|decided|summary` written by `FTCModeWatch` (`Source/ThreeCrossroads/Client/TCObjectives.h`),
which logs what the HUD showed of the mode (the score strip, the goal box, the scripted lines, the winner and stamp).

Example (server from `unreal/Tools/tc-local.sh serve`):

```
TC_RUN_DIR=<server folder> TC_MODES_TOKENS=<folder outside the evidence> unreal/Tools/modes/run-mode.sh horde <out>/horde
node unreal/Tools/modes/summarize.mjs --table <out>/*
```

Nothing here spawns units (a sandbox-spawned supply truck crashes the reference server at e4e579a). Seat tokens stay in
`TC_MODES_TOKENS`, never in the run folder or on a command line.

Optional keys in `modes.json` (round 16): `"tour": "<file in tours/>"` flies a camera tour (`-TCTourFile`, poses at match
times, one capture each); `"spectator": true` adds a third native client (rendered, English, `-TCSpectate`) that flies the
tour instead of the host. A seat only learns of terrain its units see (reference `shared/sim.js` `observedTerrainFor`), so
an event far from the armies (The Great Bridge's blow at 420 s) shows only to a spectator (`server.js` `watchGame` sees
every cell). `run-mode.sh` stops the host 30 s after a guest that dies before the match starts (GPU out of memory, say),
instead of leaving it waiting in the lobby. Mode ids beyond the plain modes: `annihilation-sandbox` and `horde-sandbox`
(the host spawns four enemy Kaiju from the sandbox panel so the mode's own end comes within a run), `tutorial-es` (the
Spanish guest rendered too).
