# Post process and light: project settings (notes for the lead)

The look of the battlefield is set in code by `Client/TCLighting.h` (`ATCLighting`), not by these ini files. Its
unbound post process volume already locks every setting below at run time. The ini lines make the project's
defaults agree, so editor viewports, captures without `ATCLighting` and any later camera start from the same place.
**The slice 11 lead applied every line below to `DefaultEngine.ini` (`[/Script/Engine.RendererSettings]`) at integration.**

## What the reference does (three.js r186, `client/light.js`, `client/moods.js`)

| Reference | Unreal (`ATCLighting`, `M_TCLookPost`) |
| --- | --- |
| `renderer.toneMapping = ACESFilmicToneMapping`, `toneMappingExposure = mood.exposure` (day 0.9) | `M_TCLookPost`, a post process material at `BL_ReplacingTonemapper`: three's own ACES fit (exposure / 0.6, the RRT and ODT fit, its input and output matrices), then `sRGBTransferOETF` |
| no automatic exposure | `AutoExposureMethod = AEM_Manual`, `AutoExposureBias = 0`, physical camera exposure off: the engine's exposure is exactly 1 (`PostProcessEyeAdaptation.cpp CalculateManualAutoExposure`), and the material takes the pre-exposure off |
| no bloom, vignette, grain, chromatic aberration, lens flare or motion blur | all at 0 (the replacing tonemapper also skips the engine tonemapper's vignette, grain and local exposure) |
| `outputColorSpace = SRGBColorSpace`, linear working color | the same: linear scene color, the sRGB transfer in the material |
| `scene.fog = Fog(haze, dist x 0.6 / hazeK, dist x 3.8 / hazeK)`, mixed after tone mapping in output space | the material mixes the haze (its sRGB bytes) after tone mapping by `smoothstep(near, far, view depth)` |
| `scene.background = haze` (clear color, not tone mapped) | pixels with no geometry (scene depth past `SkyMetres`) take the haze's bytes |
| no dynamic GI; `scene.environment` = the PMREM of `outdoorEnvironment()` | `DynamicGlobalIlluminationMethod = None`, `ReflectionMethod = None`; the environment is in the sky light (diffuse) and in the model materials (`TCLookIndirect`, specular) |
| GTAO on High (`AO.radius 1.6`, `AO.mix 0.85`) | `AmbientOcclusionIntensity 0.85`, `AmbientOcclusionRadius 160` in the game (`0` in the comparison, whose reference draws no AO pass). The engine's SSAO only darkens sky light, and the model materials take none, so this is partial |

## `DefaultEngine.ini` lines (`[/Script/Engine.RendererSettings]`)

```
r.DefaultFeature.AutoExposure=False
r.DefaultFeature.AutoExposure.Method=2
r.DefaultFeature.AutoExposure.Bias=0
r.DefaultFeature.Bloom=False
r.DefaultFeature.LensFlare=False
r.DefaultFeature.MotionBlur=False
r.DynamicGlobalIlluminationMethod=0
r.ReflectionMethod=0
r.SkyAtmosphere=False
r.AllowStaticLighting=False
```

- `r.AllowStaticLighting=False` is **required** for the model materials:
  - With static lighting allowed, the desktop GBuffer has no AO channel: `GBufferC.a` holds `IndirectIrradiance x AO`, and the deferred decode sets `GBufferAO = 1` (`DeferredShadingCommon.ush`). So the engine's sky light (diffuse and reflection) reaches the `M_TCModel*` pixels despite their AO of 0. It is then counted twice, on top of their `TCLookIndirect` node, and painted metal reads up to 15 levels too bright.
  - This client has no lightmaps (everything is spawned movable from C++), so nothing is lost.
  - It changes the GBuffer layout, so every shader recompiles once.

- `r.DynamicGlobalIlluminationMethod=0` is already the engine default; writing it down keeps a later template merge from turning Lumen on.
- `r.SkyAtmosphere=False`: `ATCGameMode::SpawnScene` no longer spawns a sky atmosphere once the `lead-TCGameMode.patch` is in. The reference's sky is the flat haze.
- **Not proposed:** `r.Substrate` (it stays off, the project default). The model materials and the look materials are legacy shading models, and the sky light's diffuse was verified under it.

## Cooking

- The look assets are under `/Game/Models/Materials/Look`, already covered by the `+DirectoriesToAlwaysCook=(Path="/Game/Models")` line from the models slice.
- `ATCLighting` builds its sky cube at run time (`UTextureCube::CreateTransient`), so nothing more needs cooking.

## Output encoding

- `M_TCLookPost` writes display-encoded color, as the engine's tonemapper does into the viewport's 8-bit UNORM target.
- A target that sRGB-encodes in hardware (for example a scene capture into `RTF_RGBA8_SRGB`) would encode it twice. For such a target, set the material's `OutputLinear` to 1.
- The comparison captures use `RTF_RGBA8` and were checked patch by patch against three (`slice-11/look`).
- The game viewport's path was not checked byte for byte against the haze color (`#b9bdbc` with the day mood); the live
  screenshots of slice 11 (verification evidence kept outside the repository) were reviewed by eye only.
