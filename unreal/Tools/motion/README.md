# Unit motion for Unreal

The pinned browser client's presentation-only motion, ported for the native client and checked against the
reference under Node. Code: `Source/ThreeCrossroads/Client/TCUnitMotion.h/.cpp` (vehicles, boats, kaiju, planes,
tracks, the soldier crowd and its corpses) and `Client/TCInfantryRig.h/.cpp` (the pose solver, and now the whole
`animate()` of a squad: `FTCInfantrySquadAnimation`).

```
bash unreal/Tools/motion/run.sh            # oracles + far-figure export into unreal/Content/Models/Motion
bash unreal/Tools/motion/run.sh --unreal   # also: the crowd materials, a scratch C++ project (copies of the client
                                           # files + ue-check/), its automation tests and the offscreen render
```

| Reference | Port | Checked by |
| --- | --- | --- |
| `infantry.js` `rigOf`/`solve` (joints, frames, skin matrices, muzzle) | `TCInfantryRig.cpp` (unchanged solver) | `unreal/Tools/models/infantry/oracle` (5,241 cases, 1e-9) |
| `unit-models.js` `animate()` for squads: LOD swap, posture blend, aim, dig, suspension, `moveSquad`, keyframe weights, pose node, `u.muzzle` | `TCInfantryRigCore::AnimateSquad`, `FigureMuzzles`; `FTCInfantrySquadAnimation` | `oracle/motion-*` (14 squads x 420 frames, 1e-9) |
| `model-motion.js` `moveModel` (land settle, sea swell) | `TCUnitMotionCore::MoveModel`; `FTCUnitMotion::Update` | same (8 vehicles and boats x 360 frames) |
| `wheel-motion.js` `moveWheels`, `wheelAngle` | `MoveWheels`, `WheelAngle` | same (travel and turn; every Sherman wheel) |
| `track-motion.js` path table, `trackVertex` | `TrackRegister`, `TrackFrame`, `TrackVertex`; `FTCTrackPaths` | same (the Sherman's own track links) |
| `unit-models.js` `stride` (kaiju) | `Stride` | same |
| `aircraft.js` `tick` (bank, wobble, propellers) | `PlaneTick` | same (fighter, bomber) |
| `unit-models.js` `createBodies` (cap 200, soft 184, life 25 s, fade 1.5 s, sink 0.45 m, toss) | `BeforeAdd`, `StepBody`; `UTCSoldierCrowd` | same (2,400 frames, 200 bodies, a blast) |
| far figure and its morph targets (`unit-models.js` `poseMorphs` on `infantry.js` `farParts`) | `export-far-figures.mjs` -> `FarFigures.bin`; drawn by `UTCSoldierCrowd` | `ThreeCrossroads.Motion.Crowd` (all 161 figures, every key) |
| `main.js` camouflage (opacity 0.45), `corpse()` | `FTCSoldierDraw::Fade`, `UTCSoldierCrowd::SquadAlive/SquadGone` | tests and render |

## Files

- `register.mjs`: the infantry tool's module hooks plus `../models/shims.mjs`, so `unit-models.js` and friends load
  under Node unmodified.
- `export-far-figures.mjs`: builds every squad of every faction with the reference's own `buildModel` (three probe
  owner colors), takes each man's baked far mesh: the standing build, its 23 or 27 morph targets (stored undone by
  their pose node, as `poseMorphs` does) and the fallen build, plus the near and far fallen builds' lowest point
  (corpses rest on it). Owner color is linear (`base + tint * owner`, checked on a third color). Writes
  `Content/Models/Motion/FarFigures.bin` (zlib; layout in `TCUnitMotion.cpp`) and `FarFigures.json` (provenance,
  hashes, figure list). Deterministic: `run.sh` exports twice and compares.
- `make-materials.py`: `/Game/Models/Motion/M_InfantryCrowdFaded`, `M_InfantryCrowdKitFaded` (the infantry crowd
  looks with a dithered fade), `M_InfantryFar`, `M_InfantryFarFaded` (the far figure: vertex colors at full precision in
  UV0/5/6, the owner band from custom data, every vertex skinned by its own keyframe column). The look and skinning
  HLSL is copied from `../models/infantry/import_infantry.py`, the dither from `../models/import-models.py`.
- `oracle/`: `motion-reference.mjs` (reference side, writes `scenario.txt` and `reference.jsonl`; `Math.random` and
  `performance.now` recorded), `motion-harness.cpp` (C++ side, both client files compiled standalone),
  `compare.mjs` (tolerances in its header).
- `ue-check/`: `TCMotionCheck` commandlet for the scratch project: material compile, the crowd in every posture near
  and far, camouflage, bodies, vehicles from the catalog in motion, the catalog's far figure beside the crowd's.
- `make-crowd-look.py` (with `crowd-shared.py`): the near crowd in the model masters' look (round 14). The two HLSL
  nodes and every parameter they read are taken from the patched `M_TCModel` at run time; the crowd's own data feed
  them as `infantry-authored.js` builds a soldier (tunic tint over the cloth mask, wool or plain, the atlas with
  `modelUV.z = 1 + cloth`) plus the grime `unit-models.js mergeParts` bakes per vertex (`LOOKS.soldier`), computed on
  the standing build (keyframe 0). Writes `M_InfantryCrowdLook(Faded)`, `M_InfantryCrowdKitLook(Faded)` and their
  instances. `UTCSoldierCrowd` prefers them (`-TCCrowdAtlasLook` keeps the old ones, for side by sides).
- `make-wheel-tracks.py`: the tracked hulls again with the reference's wheel and track data (round 14; parity row
  CLI-104: wheel spin and track scroll). For each catalog hull whose reference glb has
  `_WHEELPIVOT`, it copies the importer's normalized glb (same vertices, checked position by position and by sha256)
  with UV4-7 = wheel pivot, (wheel side, radius), (track phase, table row), (link side, floor), imports it with the
  importer's Interchange settings to `/Game/Models/Motion/Tracks/`, copies the patched `M_TCModel` to
  `M_TCModelTracks` with the world position offset and turned normal of `wheelMaterial`/`trackShader`, and writes
  `Content/Models/Motion/WheelTracks.json` (the loops in their table rows, the hull copies). 21 hulls, 11 loops.
- `wheels/shoot-wheels.mjs`: the reference half of the wheel comparison: `../look`'s page on a copy that sets every
  wheel mesh's `wheelTravel` to a given travel and turn; the Unreal half is `TCLookCompare` with `-TCWheelTravel=m,rad`.

## Custom data the crowd components carry (`TCCrowdData`)

0-11 the four heaviest keyframes, their weights, lean, height scale, shift (`UTCInfantryRigLibrary::CrowdCustomData`),
12 the kit bone, 13 the fade (faded materials only), 14-16 the owner color (far figures). Solid near body 12 floats,
solid kit 13, faded near 14, far 17.

## Adoption in `TCUnitModels.cpp`

Adopted by the slice 11 lead as below, with two differences: the squad's `animate()` runs before the unit's parts are
chosen (`ATCUnitRenderer::AnimateSquad`), so a crew gun and its crew swap level of detail in the same frame, and
`SquadGone`'s `bKilled` comes from the effects' kill shots (`FTCCombatEffects::TakeKilledGone`, main.js `v.killed`).
`BlastNear` (the toss) is still not passed. The snippet that was handed over:

`ATCUnitRenderer` keeps one `FTCUnitMotion` and one `UTCSoldierCrowd`, and one `FTCInfantrySquadAnimation` per squad
(in `FSquad`). Nothing else changes in its public API.

```cpp
// TCUnitRenderer.h
#include "Client/TCUnitMotion.h"
FTCUnitMotion UnitMotion;
UPROPERTY() TObjectPtr<UTCSoldierCrowd> Crowd;
// in FSquad: FTCInfantrySquadAnimation Anim;   (replaces Motion, W, AimBlend, Work, WorkT, Pose*)

// EnsureModels(), after the library loads
Crowd = NewObject<UTCSoldierCrowd>(this);
Crowd->SetupAttachment(RootComponent);
Crowd->RegisterComponent();
FString CrowdError;
Crowd->Load(CrowdError);                     // logs; near or far men it cannot draw fall back to the catalog

// Draw(), before the units: Crowd->BeginFrame();
// after them (FlushModelFrame):
Crowd->EndFrame(Dt, Eye, [Terrain](double X, double Y) { return Terrain ? Terrain->HeightAt(X / 100.0, Y / 100.0) * 100.0 : 0.0; });
UnitMotion.Retain(IdsDrawnThisFrame);
// a squad that left the snapshot: Crowd->SquadGone(Id, bKilled) (bKilled: a kill shot hit it, main.js v.killed)

// DrawModel(), vehicles, boats, kaiju, planes (not squads):
const FTCPartMotion& Motion = UnitMotion.Update(U.Id, U.Type, Root.GetLocation(), S.Rot, true, Dt, GetWorld()->GetTimeSeconds());
Parts.Add({ Ism, FTCUnitMotion::PartTransform(*Entry, I, Root, Turns, Motion) });   // instead of FTCModelLibrary::PartTransform

// DrawSquadMen(): the squad's animate() and its men
FTCInfantrySquadAnimation& Anim = Sq.Anim;
if (!Anim.IsSetUp(U.Type, Faction, Men)) Anim.Setup(U.Type, Faction, SlotsFromNearRests, NearRestScale);
Anim.SetAlive(Alive);
FTCInfantrySquadFrame F;
F.Root = Root.GetLocation(); F.Yaw = -S.Rot; F.Eye = Eye; F.Id = U.Id; F.Flags = U.Flags; F.Cover = U.Cover;
F.Suppression = U.Supp; F.bHasTarget = U.Target != 0; F.bMoveFire = MovesFiring(U.Type); F.Aim = S.Aim;
F.bGarrisoned = (U.Flags & TCFlags::Garrisoned) != 0;
F.GroundCm = [Terrain](double X, double Y) { return Terrain ? Terrain->HeightAt(X / 100.0, Y / 100.0) * 100.0 : 0.0; };
Anim.Animate(F, Dt);
S.bFar = Anim.IsFar();                       // the squad's and its crew gun's level of detail
for (int32 K = 0; K < Men; K++)
{
	if (!Anim.Men()[K].bVisible) continue;
	FTCSoldierDraw D;
	D.FigureType = FigureType; D.Faction = Faction; D.Slot = K;
	D.Man = Anim.ManTransform(K, FTransform(Root.GetRotation(), Root.GetLocation()));
	D.Blend = Anim.Men()[K].Blend; D.Owner = Library.SeatColor(U.Owner);
	D.Fade = Camouflages(U.Type) && (U.Flags & 256) ? Library.CamouflageFade : 1.f;
	D.bFar = Anim.IsFar(); D.UnitId = U.Id; D.ManIndex = K;
	if (!Crowd->AddMan(D)) { /* the existing catalog part fallback */ }
}
Crowd->SquadAlive(U.Id, Alive);
// muzzle flashes: Anim.MuzzleWorld(K, Root, Out) is the reference's u.muzzle in the world
```

Then the renderer's own `Figure()`, `KitFor()`, `Figures`, `Kits` and the far branch of `DrawSquadMen` can go: the
crowd draws near men exactly as `Figure()` did (same assets, same keyframe texture), plus faded and far ones.

## Wheels and tracks (round 14)

`ATCUnitRenderer::ApplyWheelTracks` gives a catalog hull listed in `WheelTracks.json` its copy, `M_TCModelTracks`'s
instance as a dynamic instance with `TrackPaths` = `FTCTrackPaths::CreateTexture()` (the loops registered in the
file's rows; a row that does not land where the tool put it switches the copies off) and 10 floats of custom data;
`AppendPartData` adds `FTCPartMotion::WheelTravel` and `WheelTurn` as floats 8 and 9. Checked by
`ThreeCrossroads.Motion.WheelTracks` (every copy: the catalog hull's triangles and bounds, 8 texture channels in the
mesh and its render data, wheel and link vertices, rows) and by pictures against the reference at the same travel.
`-TCNoWheelTracks` draws the catalog hulls.

## Not drawn yet, and why

- **The far figure's surface layers.** `M_InfantryFar` paints the reference's vertex colors (shape and colors match
  the catalog's far mesh exactly: same bounds, side by side in `render-far-compare.png`) with one roughness (0.9, the
  reference's `INFANTRY_PAINT`), without the wool/steel texture layers and metalness of `M_TCModel`.
- **The trench sink** waits for carved trenches (`FTCInfantrySquadFrame::bCarvedTrenches`).
- **A blast tossing fresh bodies** needs `fx.blastNear` from `TCEffects`: `EndFrame`'s `BlastNear` argument.
- Four keyframes at most per man in the crowd material (as before); `animate()` can weigh more during a posture blend.
