# Great Bridge tick-8400 replay (round 17)

The Unreal decoder replay of the Great Bridge recipient captures (release `legacy-great-bridge-8400-efbd55c7`), for
the Legacy adoption gate. The bundle is not in this repository.

- `run.sh <bundle.tar.gz> <work dir> [out dir]`: verifies the archive and every manifest hash (`bundle.py verify`),
  extracts into the work dir, prints an independent summary (`bundle.py summary`) and runs the automation test
  `ThreeCrossroads.Net.LegacyBridgeReplay` with `TC_BRIDGE_REPLAY_DIR` set. Results: `summary.txt`, per stream
  `<stream>.summary.txt` and `<stream>.state-8400.txt` (the applied client state), `compare-slot0.txt` and
  `compare-slot1.txt` (native against source per seat), `test.log`.
- `capture.sh <extracted dir> <stream> <out dir> <en|es> [shot seconds]`: the rendered tick-8400 view. `replay-server.mjs`
  serves the recorded stream, unchanged, to one game client over a local WebSocket (Node standard library only), holding
  tick 8400 back for 60 s; the client's atmosphere check frames the bridge with and without the HUD.

The test also runs without the bundle: it then warns that the replay was not run. `ThreeCrossroads.Net.CellRows` checks
the cell row decode the replay relies on (state, data and structural sections) on hand-made rows.
