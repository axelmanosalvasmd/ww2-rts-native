# Tutorial and scenario guidance tools (round 14)

Reproducible inputs and oracles for `Source/ThreeCrossroads/Client/TCTutorial.*` and `Client/TCScenarioText.*`.
All read the pinned reference (`reference/ww2-rts`) and write nothing into it.

| Tool | What it does |
| --- | --- |
| `export-tutorial.mjs <reference> [TCTutorial.cpp --write]` | writes `shared/tutorial.js` STEPS (goal, cell, lines, manpower, spawns) into the generated block of `TCTutorial.cpp`, and checks that `client/es.js` has the Spanish of every goal and line |
| `objectives-oracle.mjs <reference copy with node_modules>` | runs the reference's own `client/objectives.js` (stub hooks, a minimal document) and `client/i18n.js` under node: goal box text and class, `collapse()` sentences, the banner's 4 s, damaged-building emitters and their burst cadence, the Spanish of the Tutorial's texts. The C++ tests quote its output |
| `player.mjs <reference copy> <ws url> <out base> [room] [name]` | a browser-shaped Tutorial player for an isolated reference server: the public lobby's Tutorial button (a new private room, hello with `listing.tutorial`), then `policy.mjs`; records every message it receives to `<out base>.jsonl` |
| `policy.mjs` | the verification policy, from one seat's snapshot only; `ATCGameMode`'s `-TCTutorialDrive` runs its C++ port (`FTCTutorialDriver`) |
| `drive.mjs <reference>` | the same policy against the reference sim directly (no server), to see whether an honest scripted player finishes all nine steps |
| `stream-oracle.mjs <reference copy with node_modules> <stream.jsonl>` | feeds a recorded stream through the reference's own `client/world-state.js` and `client/objectives.js`: the goal box at each change, the collapse shots and the banners they raised, the smoking buildings; the full-stream test quotes its numbers |
| `make-tutorial-fixture.mjs <stream.jsonl> <TCTutorial.cpp> [--write]` | trims a recorded stream to the lobbies, the start and the snapshots that changed the step or carried a line or the winner, and writes the fixture block the `ThreeCrossroads.Tutorial.Recorded` test replays |

The reference copy for `objectives-oracle.mjs` and `player.mjs` needs its `node_modules` (`three`, `ws`):
`unreal/Tools/tc-local.sh serve` makes one under its run folder.

The full recorded stream can be replayed by the test too:
`-TCTutorialStream=<stream.jsonl>` on the automation command line.
