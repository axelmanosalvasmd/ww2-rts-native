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
UnrealEditor-Cmd <scratch project> -run=pythonscript -script=<absolute path>/make-section-material.py -AllowCommandletRendering -RenderOffscreen -unattended
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
Round 18 found darker native section faces and lighter dust. Round 19 identified an uncooked UE 5.8 ODSC
shader fallback to WorldGridMaterial, despite a correct Color parameter. The scoped renderers now complete the
generated material shaders during setup. Replay captures log `lookfx surface` with the requested and actual
render-thread material, so a fallback cannot be mistaken for a palette or lighting discrepancy. Cooked shader
behaviour is unchanged. Dust uses the live reference hemisphere divided by PI, without the engine sky/environment
substitution or sun-scaled hot sprites. High/Low contact emission follows fx.js.

Optional browser capture refinements: `--native-log <native game.log>` uses the four recorded native eye positions
in the browser's camera, including orientation (to remove differences in ground-height easing). `--visual-clock`
holds effects at age zero while normal reference frames apply the queued bridge structure rebuild, then stops
the page loop and advances the unmodified effects update in at most 1/60 s substeps to exact visual ages 0, 1 and
3 seconds. It calls the reference's `light.renderFrame` on its normal Graphics High path. Camera placement,
render and canvas readback share one task. This drives only local
presentation. Delivered section poses stay unchanged. The browser log includes wall age and visual age; keep normal
timing captures when diagnosing renderer timing. This controlled pass is not a normal-browser wall-time run.


Round 19 HQ cloth and detail views:

- `capture-browser.mjs --cloth-out <scratch>/flag-cloth.png` also exports the actual reference `flagMat` canvas.
  The supplied tick-8398 scene already contains it. The canvas is deterministic, 128 x 64, with the painted folds,
  weave and pole-side hem. SHA256: `0bd4dcc58c36c7446afbf001b97c22f9ea3b80a35ba6b7a45cd1c0fc36d71a80`.
- Generate the texture and Lambert cloth material on the scratch project with `LOOKFX_CLOTH=<PNG path>` and
  `UnrealEditor-Cmd <project> -run=pythonscript -script=<absolute path>/make-lookfx-assets.py
  -AllowCommandletRendering -RenderOffscreen -unattended`. `LOOKFX_ASSET_LOG=<JSON>` records provenance.
  Use a real RHI for material generation checks. NullRHI cannot detect shader compile errors.
- `TCCampModels.Export.mjs --deps <pinned npm install>` exports the reference plane with the other camp geometry.
  The pinned `main.js` uses a static 4.5 x 2.8 m plane centred at (2.3,13,0). It contains no flag-wave formula or
  faction emblem. The material uses the exact sRGB cloth canvas, seat tint, DoubleSide and hemisphere Lambert fill.
  Remove the superseded flat quad in TCMarkers when integrating the camp cloth.
- `TC_CAPTURE_VIEW=camp|soldier` changes the native replay camera to spawn 0 at 40 m or the lowest-id rifle
  at the normal 25 m minimum.
  `--native-log` reads that framing as well as the eye positions, so the browser camera matches detail views too.
  Browser warm-up uses the same focus and distance. Each readback rebuilds the reference crowd for the matched
  eye before rendering, as the normal main.js loop does.
- Diagnostics only: `TC_CAPTURE_BUFFER=BaseColor|WorldNormal|PreTonemapHDRColor|LightingOnly` and
  `TC_CAPTURE_NO_SHADOWS=1`. Buffer visualizations can still be affected by the custom post process.

No random pose spread or client physics was added. `section-oracle.mjs` continues to exercise the reference
`debris-motion.js` spread/tumble at five ages and four directions; live poses remain entirely packet-driven.
