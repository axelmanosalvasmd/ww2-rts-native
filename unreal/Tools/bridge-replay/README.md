# Great Bridge tick-8400 replay (round 17)

The Unreal decoder replay of the Great Bridge recipient captures (release `legacy-great-bridge-8400-efbd55c7`), for
the Legacy adoption gate. The bundle is not in this repository.

- `run.sh <bundle.tar.gz> <work dir> [out dir]`: verifies the archive and every manifest hash (`bundle.py verify`),
  extracts into the work dir, prints an independent summary (`bundle.py summary`) and runs the automation test
  `ThreeCrossroads.Net.LegacyBridgeReplay` with `TC_BRIDGE_REPLAY_DIR` set. Results: `summary.txt`, per stream
  `<stream>.summary.txt` and `<stream>.state-8400.txt` (the applied client state), `compare-slot0.txt` and
  `compare-slot1.txt` (native against source per seat), `test.log`.
- `capture.sh <extracted dir> <stream> <out dir> <en|es>`: four rendered moments around tick 8400. See the round 18
  options and fixture limitations below.

The test also runs without the bundle: it then warns that the replay was not run. `ThreeCrossroads.Net.CellRows` checks
the cell row decode the replay relies on (state, data and structural sections) on hand-made rows.

Round 18 adds reference section transforms and a generated lit surface. Generate it before rendering or cooking:

```
UnrealEditor-Cmd <scratch project> -run=pythonscript -script=<absolute path>/make-section-material.py -nullrhi -unattended
node section-oracle.mjs <pinned reference root> <three package root> section-oracle.json
TC_BRIDGE_TEST_WORK=<scratch>/tests node --test replay-server.test.mjs
```

`Effects.SectionPose` checks 20 poses and 160 corners against the generated Three.js oracle. The reference box uses
YXZ Euler `(0,-dir,-tilt)`, which becomes Unreal pitch `-tilt`, yaw `dir`, roll 0. No random pose jitter exists in the
reference. The server supplies motion and the client interpolates its positions and tilt, then holds the last pose.
The section surface uses the model master's established Three.js hemisphere/environment shader, roughness 1,
metallic 0, dielectric specular and cast/receive shadows. Its source of truth is `make-section-material.py`.

The four-moment `capture.sh` now uses `-TCBridgeCapture=<prefix>`: before at tick 8398, first drawn tick 8400, then
1 and 3 seconds after receipt (actual elapsed times and camera are logged). Set `TC_PROJECT`, `TC_CAPTURE_WORK`
and optionally `TC_SHOT_PREFIX`. It explicitly sets Graphics High, waits for the before PNG, then releases the held
packet by POST `/replay/release`. The old atmosphere-based shot-seconds argument is superseded.

For the browser, start `replay-server.mjs` with `--reference <pinned root> --three <three package root>` and
`--release-after 600`, then run `capture-browser.mjs --origin http://127.0.0.1:<port> --out <PNG prefix>
--profile <scratch directory> --block-maps`. It sets High and uses the same (119,98), 70 m, north-up camera.
The server only serves reference files read-only and recorded payloads; it does not start a simulation. It echoes
application `ping` as `pong` with the same `c`, so the browser does not reconnect while the recording is held.

The archive ends at tick 8400 with 84 upright stone sections on a grid. The later PNGs therefore show held tick-8400
poses and ageing local dust, not unrecorded server physics. The browser's unseeded visual randomness and the native
particle seed differ. Software-rendered browser frames can arrive later than the requested 1 and 3 seconds; read the
capture log's actual ages. Do not present those frames as proof of later simulation tumble.
Round 18 capture review still finds darker native section faces and lighter dust than the browser. The section
surface and sky inputs are ported, but rendered lighting parity remains open.

Optional browser capture refinements: `--native-log <native game.log>` uses the four recorded native eye positions
in the browser's camera, including orientation (to remove differences in ground-height easing). `--visual-clock`
holds effects at age zero while normal reference frames apply the queued bridge structure rebuild, then stops
the page loop and advances the unmodified effects update in at most 1/60 s substeps to exact visual ages 0, 1 and
3 seconds. It calls the reference's `light.renderFrame` on its normal Graphics High path. Camera placement,
render and canvas readback share one task. This drives only local
presentation. Delivered section poses stay unchanged. The browser log includes wall age and visual age; keep normal
timing captures when diagnosing renderer timing. This controlled pass is not a normal-browser wall-time run.
