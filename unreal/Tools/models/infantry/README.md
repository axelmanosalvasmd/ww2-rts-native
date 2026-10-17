# Infantry for Unreal

Builds the soldiers from the pinned reference's Blender-authored body (`client/models/infantry-authored-data.js`, the
baked payload of `art/model-reference-2026-10-03/infantry-rig-v3.blend`) and its pose solver, and checks them.

```
bash unreal/Tools/models/infantry/run.sh            # export + Node/C++ oracle (no Unreal)
bash unreal/Tools/models/infantry/run.sh --unreal   # also: scratch-project build, import, in-engine checks, offscreen
                                                    # render, copy of the packages into unreal/Content/Models/Infantry
```

- `export-infantry.mjs`, `figures.mjs`, `gltf.mjs`, `reference-hooks.mjs`, `register.mjs`: the body as a skinned mesh on
  a flat 31-bone skeleton (root, 15 deform bones, 15 kit anchors), the crowd body and the German helmet crown as static
  meshes whose UV channels carry joints and weights, one static mesh per kit part, and `kits.json` (which kit mesh hangs
  on which anchor for each type, faction and job).
- `import_infantry.py`: the Unreal import (skeleton, sockets, materials M_InfantryBody, M_InfantryKit, M_InfantryCrowd,
  M_InfantryCrowdKit and their faction and kit instances).
- `oracle/`: the reference solver and `squad-motion.js` run under Node against the C++ port (`Client/TCInfantryRig.cpp`):
  bone transforms, skinned vertices and a 400-step squad scenario.
- `ue-check/`: commandlets for the scratch project (rig, skinning and crowd checks, and the render).

Why the crowd path: the reference draws one instanced mesh per figure with posture and gait as morph targets. Unreal's
instanced static meshes cannot morph per instance, so the 29 keyframes of a figure are baked to a bone-matrix texture
(`UTCInfantryRigLibrary::BakeKeyframeTexture`) and the crowd material skins on the GPU from per-instance custom data:
the four heaviest keyframes and their weights, then the pose node (lean, height scale, shift). That is 31 bone matrices
per keyframe instead of 7,580 vertex positions. `ATCUnitRenderer` draws every near soldier this way; the far figure is
the reference's own far-away mesh from the catalog.

Evidence (kept outside the repository with the models slice verification): `ue-check.json`, `ue-render.json`, `render-*.png`.
