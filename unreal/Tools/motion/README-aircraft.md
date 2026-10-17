# Original aircraft shadows and carrier decks

`export-aircraft.mjs` evaluates the pinned browser builders. It saves their twelve original 128x128 shadow canvases, aircraft dimensions, twenty faction/carrier hull point sets, and comparison fixtures from the original `deckSeats`, `aviation.tick` and `menDrawn` functions. It serves an in-memory module copy to expose the private seat function. It never writes reference files or changes runtime randomness.

Requirements: Node with built-in WebSocket support, Chromium at `/usr/bin/chromium`, and `three@0.186.1`. The exporter checks the clean reference pin `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`. Use a task-owned scratch directory for dependencies, the Chromium profile and export. No Blender is needed.

```sh
node unreal/Tools/motion/export-aircraft.mjs reference/ww2-rts \
  "$TC_LOOK_SCRATCH/deps/node_modules/three" \
  "$TC_LOOK_SCRATCH/aircraft" "$TC_LOOK_SCRATCH/aircraft-profile"

TC_AIRCRAFT_EXPORT="$TC_LOOK_SCRATCH/aircraft" \
  "$UE/Engine/Binaries/Linux/UnrealEditor-Cmd" "$TC_CONTENT_PROJECT" \
  -run=pythonscript -EnablePlugins=PythonScriptPlugin \
  -script="$PWD/unreal/Tools/motion/import-aircraft.py" \
  -AllowCommandletRendering -RenderOffscreen -unattended -nop4 -nosplash -nosound \
  -trace=none -NoTraceThread -notraceserver -nollm
```

Import into an isolated project before integrating its `Content/Models/Motion/Aircraft/` folder. `aircraft.json` contains original-source SHA256 hashes and each PNG hash; the importer verifies them. Generated `.uasset` files follow the existing Git LFS rule. The existing Models cook/staging rules include these assets and the JSON, without another config change.

The native client uses source extents, silhouette alpha and the original hull-derived seat formula. Each cargo squad draws three source far kneelers, up to nine. Their corpse bookkeeping stays separate from squad casualties. A commanded aircraft's shadow sits 0.3 metres above ground and follows its heading. Damaged aircraft use the original 60%/30% smoke thresholds and doubled Low intervals.

The shadow parent explicitly saves `used_with_instanced_static_meshes`. Without it Unreal substitutes an opaque default material. `AircraftShadowMesh` checks this saved flag, translucent blending, upper-face winding and canvas UVs; successful asset loads alone do not prove the visible silhouette.

Focused automation: `ThreeCrossroads.Motion.CarrierDecks`, `PlanePresentation`, `SquadMembers` and `AircraftShadowMesh`. Existing motion and atmosphere source-oracle tools remain independent arithmetic diagnostics. Captures and arithmetic checks do not establish aligned whole-match acceptance.
