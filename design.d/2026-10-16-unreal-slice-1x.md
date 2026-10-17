## Round 23 HUD loss stacking, docking and source alerts (2026-10-16)

Retain the reference's source-sized loss positions, including its UI150 score/economy overlap, and its .97 loss background alpha. Economy labels are deferred until alert bounds are known, then glyph quads are clipped at foreground cards so costs and call-in labels cannot remain readable through them. Category captions keep their full original text and crop at the panel edge, including a partial glyph; no ellipsis is substituted. Selection width follows the physical viewport media query (248px, 268px at 1600x860), independent of UI zoom. Recruitment is centered between the source selection and minimap anchors without the native 720px cap. Orders retain source 50px rows and 3px gaps. Sandbox uses the original centered bottom:230px row, retaining deliberate crowded overlaps. Integrated images still differ in score/Sandbox paint order, as recorded below. BottomLayout's earlier collision assertions are updated for that behavior.

Selection portraits continue using the imported battlefield models and owner/faction material data. The private preview now sets the original warm 1.9 key, cool .7 rim and .85 hemisphere, flushes the non-ticking world's material collection, and reproduces the source floor as a transparent black .38 shadow using model/floor comparison passes. Recruitment retains the source silhouettes. No model, texture, sound, voice or Spanish text is invented. Complete type/faction portrait coverage and renderer shadow/AA equivalence remain unverified.

The pinned client/alerts.js:164-218 groups damaged units within 30m, suppresses a watched fight, and throttles under-attack notices for 20s per area. Damage within 40m of home or to an own structure uses Our base is under attack! when home exists. Unit hits precede point changes and losses; the last routine push is displayed and Space prioritizes the separate loss list. The native pipeline now keeps that order instead of sending points in an earlier separate pass. Projection uses source ground height +1m and inclusive viewport edges. The GameMode integration patch and owned TCAlerts changes must land together. The optional held HUD alert clock also applies to the effects caller of Frame; production timing is unchanged.

The required-FFI private Linux UE 5.8.3 editor build succeeds. All 25 focused HUD/Alerts/Locale/Stats/Minimap/Recruitment automation cases report Success, including eight SourceDocking language/scale/resolution combinations and a same-event SourceOrder test. The headless TestExit hook returns process code 1 after the complete successful queue; rendered native lanes exit 0. An executable oracle runs the unchanged original alerts.js with the same offscreen/on-screen damage, capture and loss fixture, including duplicate suppression and Space priority. The Spanish exporter check passes; HudStrings walks 4,652 source/header literals, 606 exact es.js keys and the full dictionary glyph set. The fault notice has no source key and stays English; other unkeyed user strings are listed in the evidence.

Eight browser-left/native-right loss pairs cover EN/ES, UI100/UI150 and 1280x720/1920x1080, with an additional EN100/1280 Menu pair. Graphics is explicitly High, guest1 owns selected rifle4, and both cameras settle at x122,z117,distance70,heading0 before releasing the unchanged archived guest payloads through tick1932. Eight added loss events and the alert-clock hold are explicit presentation fixtures on both clients. They are not fresh simulation, Rust-host or whole-match parity evidence. Pair pixels are unchanged and SHA256/provenance is recorded. The staged replay gives only capture tick162 and genuine MG loss tick1932 on both, eliminating the earlier camera-startup base-warning mismatch. Exact pixel parity and exhaustive menu/tooltips, purchases, aircraft states, World/minimap overlays, packaging and other platforms remain unverified. Native settings-menu widgets still differ visibly from the browser. No parity row is accepted and no benchmark is run. Evidence: round-23/hudpanel/REPORT.md and capture-manifest.json.

## Round 23 camp fog, billboards and visibility (2026-10-16)

The original 472-bag HQ ring keeps its pinned structures.js geometry, outward normals, reflected winding, instance tint and three-course layout. Its dark-band defect came from TCCampModels uploading the old clear=255 brightness into the round 22 clear=0 fog-alpha shader. Camps now upload the same raw [0,120,185] clear/remembered/unseen bytes as other world surfaces, use source bilinear filtering, and perform the first all-zero upload. The existing camp test exercises the actual texture update and retains its built-face winding checks; the pinned camp exporter reproduces all data unchanged.

Unit badges apply Three r186 smoothstep(fogNear,fogFar,viewDepth) to each source texture pixel, retaining alpha. High blends haze in linear light before ACES; Low blends encoded output after ACES. FCanvas material tiles write directly into the encoded HUD backbuffer, so the generated UI material applies the source sRGB output transfer itself. Simple colour tiles retain the existing Canvas transfer. Badge/star depth is recovered from the unclamped projected metre scale, while health bars use their world position. The HUD adapter removes its old 4..30 pixel clamp and minimum-height rectangles, using the source world dimensions. The generator, generated M_TCMarkerBillboard asset and HUD adapter patch must integrate with the owned shader and marker code.

The private required-FFI Linux Editor build and 29/29 focused real-Vulkan tests pass with zero test warnings/errors on loiko-linux (i7-12700K, RTX 4060). The new material test reads actual rendered High/Low encoded RGB at five fog factors, within 0.003 normalized RGB. Seven independent curve points pin smoothstep. At matched close mist poses, source/native badge luma is 198.78/198.78 and 197.78/197.78 High, 178.24/178.35 and 175.35/175.43 Low. The reconstructed native float32 fog differs from source by at most 1.1e-7 at the sampled bar depths. This reconstruction is not runtime uniform readback. Low wide health/star differences reach 6.85 encoded luma bytes, including AA/upscaling.

An unmodified world-state.js decode of 2,451 archived snapshot packets ends at tick 1932, 14 held units, held [14,13], zero held-set mismatches. The sparse left blue ring is flakpos 8 at x102/z122, hp1066/1500. Source has a twin gun and sandbag ring, no crew. Actual native component reads find its visible mesh with 2,762 vertices, 2,609 above terrain and 214.667 cm maximum clearance. Units 9..11 are also submitted. All eight held infantry squads match source living counts: 3, six times 15, and 21. A sprite-only control exposes the complete original gun without changing logical state, component transforms, terrain, materials or damage. Ordinary smoke remains a visual mismatch; rapidly replaying history does not align FX age or consumed random values. No invented soldiers or arbitrary smoke-opacity correction is applied.

Fourteen fixed source-selected High/Low region probes retain east-roof errors +11.15/+17.92 luma bytes and left-bag errors -19.30/-11.11; soldier interior errors are +1.76/+1.59 High and +0.30/+0.25 Low. Source mast/flag shadow shape and native shadow/AO/backend differences remain visible. No global exposure, art, AA or shadow setting is retuned to hide these residuals. Evidence, all numeric RGB/masks, immutable drivers, failed material tries, native/browser PNGs and lead patches are under the private round-23 look report and the private look screenshots. These are archived renderer diagnostics, with no fresh match, whole-match RNG alignment, cooked/platform verification, row promotion or human visual acceptance claim.

## Round 23 session and current-main Rust host (2026-10-16)

The ordinary CLI is built from a git archive of main b113bb959f95499f04b47213bd27efb805f18e95, with at most eight Cargo jobs and a 6 GiB user-scope limit. This includes #373, #381 and #382 and closes the round 22 compile blocker. All nine executable trusted-room TCP/WebSocket tests pass. Fresh High two-native lanes verify move, attack-move, in-flight Stop, holdFire, foreign ownership rejection, same-token reconnect, healthy Restart and absent-seat auto-pause/resume. All 33 public maps start for both native seats, including No Man's Land (49 host and 50 guest adopted/agreeing). Ordinary lifecycle and map totals are 5,875 adopted / 0 refused / 5,875 agreeing, with zero apply errors, resyncs, not-fed or disagreements. All actual native/server/proxy map and lifecycle children exit 0. A 17-map outer launcher returns 143 after recording all maps and clean child exits; its signal source is unestablished. Decoder agreement does not establish aligned whole-match rules parity.

El Alamein's old natural fault trigger is fixed on this main: mine cell 5507 detonates at tick 544, leaving rifle 4 at health 62 and suppression 60, with snapshots continuing and healthy Restart succeeding. Faulted Restart is checked separately using a reproduction-only cfg(test) fixture that calls the existing runtime fault latch at tick 200. Two actual native clients adopt/agree 100 snapshots each, then stop receiving snapshots. The actual Menu/Restart handler receives scenario, keeps identity 1 and displays one dedicated terminal notice. This verifies the existing terminal policy and UI under a controlled fault; a naturally faulting ordinary CLI is not demonstrated and no production fault ingress is added.

The current host's 33 public HTTP map payloads equal its embedded public copies and return Cache-Control: no-store. An immediate Default to No Man's Land selection exposed a blank native preview because the previous map's five-second retry deadline applied globally. TCMatchOptions now keeps independent deadlines per map, retaining throttling when revisiting a failed map. One focused PreviewRetry test protects this behavior. The three MatchOptions tests, including all 165 disk fixture cases, pass in an isolated required-FFI editor build; the three-second live checkpoint draws the selected 120x90 preview and its backdrop. Cancellation of an old in-flight request remains unchanged.

The existing slice22 Linux Development package, without a package rebuild, passes actual XTest click, Shift remove/add, eight-pixel box and ground-clear selection, Ctrl+A/Ctrl+1, four-edge Always confinement and Esc release. A fresh process plus same-token socket reconnect restores exact camera, selection and group state. These separate package lanes add 1,268 adopted/agreeing with zero decoder errors. Writer, reader, server and proxy exit 0; the held guest is deliberately stopped with SIGINT (130). These are automated OS-input checks, not human acceptance or complete fullscreen/Off/preference coverage. Thirteen focused installed native tests pass before the preview fix; the three rebuilt MatchOptions tests pass afterward. Structured results are clean even though the engine TestExit processes return 1, retained separately from test outcomes.

Since fd714e36, #373 only documents the private save-format v1 elimination owner and adds its missing serde support; #381 and #382 add tests. No socket, recipient-view ABI or header change occurs. The installed and main header SHA256 is bf203f16eca88562cfc6e53efccc48bf260d3a44750d7300f00694723ec69499. FFI remains ABI 2 at 9e618217de2ac1d0a806c6804eda50c1c9da497f. Additive IF-04/IF-12 calls await their reviewed contract PR and C03 fog is still unavailable. No ingress widening, World admission, recovery-policy change, original art/audio/translation change or public hosting. No parity row is accepted; other platforms, human review, dynamic Spanish and the broader deferred matrices remain open.

## Unreal client: frontier, hidden-HUD rings and water (round 23, 2026-10-16)

The pinned `fog.js` frontier is a distance texture, not a polygon contour. For the nearest unknown cell at squared
distance d, it rounds `255 * (1 - t*t*(3 - 2*t))`, where `t = min(1, (sqrt(d) - 0.5) / 4)`, then linearly filters the
cell-centre alpha texture. Keep that source algorithm. The native staircase had two other causes: territory borders
were sorted after fog instead of before it, and unknown paint was already dark instead of the source's public open
ground. `TCFogFrontier` shares the terrain paint material on its unknown opaque plane, and `TCGroundPaint` normalizes
unknown inputs before using the ordinary source painter. The source knowledge mask remains strict for decals.

The Low concentric-ring defect was stale camera-dependent geometry. TourCheck sets `AHUD::bShowHUD=false`, so
Unreal skips `DrawHUD`, formerly the only caller rebuilding the world marker mesh. The opening overview's minimum-pixel
ring widths survived later close camera poses. The lead marker patch refreshes HQ camps, resource nodes and World
regions and authored capture points in `PostRender` while the interface is hidden. Ribbon fragments evaluate `overlay.js`'s original dark rim,
opacity and derivative-based smoothsteps directly. This replaces the CPU strips, without changing the source geometry
or inventing marker art. High/Low replay frames show the intended HQ rings and open resource brackets. This is an
editor-game replay of the genuine cooked-Low recording; that scoped child check did not verify a newly cooked package. The integrated package coverage and remaining cooked camp defect are recorded below. The same refresh also
retains authored point rings and resource/VP labels while the screen interface is hidden.

Water keeps the original opacity, shoreline mask, foam, ripples and source indirect-light/DFG lookup. Its direct term
uses pinned three 0.186.1 `RE_Direct_Physical`: dielectric Fresnel attenuates diffuse, correlated Smith/GGX supplies
specular and DFG supplies multiple-scattering compensation. High uses the original five-sample, per-pixel-rotated
Vogel PCF with bilinear comparison, reading the native directional shadow atlas; Low retains a point comparison.
The same source disk radius is converted to native atlas texels using the view's shadow coverage. The native
receiver comparison is `1 - Z + bias.x < sampledDepth`, matching the engine's `1 - Z < sampledDepth - bias.x`; strict
boundary and source world-radius conservation contracts protect both adapter formulas. Native casters,
depth bias, atlas quantization and opaque shadow filtering still differ, especially on delivery-truck wrecks.

The native global haze pass reads opaque riverbed/apron depth behind translucent water. The source fog uses the water
fragment's view depth. This caused much of the positive water brightness offset and the extra Islands shade triangle.
On the High linear composer path, compensate only water radiance C before the common post blend:

`C_corrected = (C * (1 - f_water) + H * (f_water - f_bed)) / (1 - f_bed)`.

Here H is the source linear haze colour and each f is the original near/far smoothstep at that surface's view depth.
Compositing `alpha * C_corrected + (1 - alpha) * C_bed` and applying the common bed-depth haze recovers the source's
separately fogged alpha composite, including the original bed contribution. No constant colour subtraction is used.
For `f_bed >= 0.9999` the shader keeps C because the common saturated blend is not invertible. All source Standard
lighting is evaluated manually in an unlit carrier with negative emissive enabled. This preserves the signed
intermediate through blending and avoids the stock lit-material clamp when inversion needs negative values. The
checked truck pose was already restored by the receiver-bias fix; signed versus lit means differ by only 0.02 luma
bytes there. A live
material contract checks both flags. The saturated-bed limit and existing Low output-space fog path remain open;
the arithmetic contract proves the invertible case, not every engine rendering case.

On loiko-linux, fixed-clock, exact-camera encoded-image diagnostics give these native-minus-browser mean luma bytes.
These are image measurements, not an idle benchmark, shader identity proof or human visual acceptance.

| Map | High interior | High mean absolute | Interior pixels | Low interior | High bank | Low bank |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Ardennes Crossing | -0.10 | 0.28 | 1,361 | +0.84 | -0.29 | +1.04 |
| The Great Bridge | +0.13 | 0.34 | 3,513 | +0.74 | -0.22 | +0.52 |
| Three Islands | +0.29 | 0.54 | 4,880 | +0.31 | -0.25 | +0.70 |

Round 22 High interior means were +1.53/+2.20/+1.73. Source-only region selection, RGB, mean absolute errors and
outer 6-to-32 m distance bands are in the private `round-23/world/water-regions.md` and JSON. No image registration,
exposure or histogram adjustment is used. All 24 final PNG pairs audit exact composition, matching logged camera eyes,
equivalent FOV, integer delivered ticks, finite source men, explicit High/Low and actual `--block-maps` commands.
Fog/water/look presentation clocks are 0/37/0. These lanes replay unchanged recipient streams; they are not fresh
simulation runs. The private engine build, eight standalone formula groups, pinned-three lighting oracle and all 36
Terrain/World/Markers real-Vulkan automation tests pass. The lead must merge the Client/generator patches, regenerate
or copy the supplied generated assets, rebuild and check the integrated cooked client. No parity row is accepted.

## Round 23 integrated verification

Required-FFI real Build.sh succeeds. All 169 Vulkan automation outcomes succeed (167 clean, two expected successful-warning outcomes), plus eight Node tests, 165 source preview cases and four Great Bridge streams of 4,200 adopted/agreeing snapshots. All sixteen reference-host mode rows complete with explicit Graphics (fifteen High, one Low), including 11 natural decisions; host-ended caps remain separate. Final ordinary live/package counters total 142,593 adopted/agreeing across 109 native logs, zero refused, apply errors, resync or disagreements. All -try paths, console duplicates, driver logs, recorded presentations and the separate controlled fault are excluded from this ordinary total; agreeing means shadow agree.

The integrated two-native R1, absent-seat and complete 33-map sweep use ordinary Rust CLI b113bb959f95499f04b47213bd27efb805f18e95 and total 5,357 adopted / 0 refused / 5,357 shadow agreeing. No Man's Land starts for both seats. Moves, foreign ownership rejection, same-token reconnect, healthy Restart and absent-seat return/resume pass. A separate cfg(test) host calls the existing fault latch at tick 200: 200/0/200, no later snapshots, Restart refused with one dedicated terminal notice. This is controlled fault evidence, not a naturally faulting ordinary CLI. The child's nine executable TCP tests and existing slice22 OS-input checks remain separately attributed. Newer main runtime changes are not covered by the b113bb95 lifecycle lanes. The approved ABI 2 FFI pin 9e618217de2ac1d0a806c6804eda50c1c9da497f and header SHA256 bf203f16eca88562cfc6e53efccc48bf260d3a44750d7300f00694723ec69499 remain unchanged.

A new Linux Development slice23 package passes the 20 GB gate, static asset/library checks and SHA256SUMS before and after runtime. Fresh headless and High rendered seated pairs total 2,008 adopted/agreeing, all actual clients exit 0. A fresh-browser cooked Low World frontier, wreck and Barracks replay adds 85 separately and five exact-camera pairs. The old extra concentric HQ rings are absent in that close cooked Barracks pose. Cooked High/Low HQ sandbags remain dark grey instead of source tan, and the cooked Low resource crate is grey instead of brown; editor captures do not reproduce this defect. All 31 changed compiled sources and six generated assets match the build inputs copied into the package project; generated assets remain build outputs. Runtime success does not establish visual acceptance.

Fifty-one canonical browser-left/native-right pairs cover selecting-guest EN/ES UI100/UI150 at both resolutions, camp and dense mist, ordinary/sprite-control left-ring views, World frontier, Ardennes/Great Bridge/Islands water and the five cooked Low poses. Eight earlier nominal HUD pairs remain diagnostic duplicates. The audits retain exact image halves, recipient ticks, native eyes/directions/FOV and explicit Graphics, without image registration or colour adjustment. The eight-loss/held-clock HUD and health/star mist fixtures are labelled presentation evidence. All eight infantry squads match the source living counts, 114 submitted body slots; the blue-ring emplacement has no source crew and is visible with only the sprite atlas hidden. This does not prove complete renderer or whole-match rules parity. Bridge replay still differs in the historical weather part (16 of 17 state parts agree).

Remaining integrated image differences include UI150 Sandbox/score paint order, cost fragments in card gaps, portrait/font/docking, extra Off controls, Logistics/veteran layouts, an extra camp-wide MP/s plate and minimap view-outline differences. Camp shading, pole/shadows, historical smoke/fire, ring state/coverage and Low AA/upscaling differ. Dark cooked HQ bags/resource crates are an unresolved package rendering defect. The Low land frontier retains a native tan line at the upper-left edge; native hidden-HUD unit badges are absent. Water caster/atlas filtering, a faint offshore shade patch and saturated-bed fog inversion remain open. Original art, sounds, voices and keyed Spanish are reused; missing keys, including the fault sentence, remain English. Human acceptance, other platforms, exhaustive dynamic Spanish/input/menu/air matrices and long-term World memory remain unverified. No parity row is accepted. No benchmark is run. No Rust ingress widening, World admission, fault policy, public hosting, FPS or editor port.

The timed Great Bridge spectator retains one Unreal Dataflow Date-initialization finding, logged twice before joining. It completes all four tour captures and exits 0 with zero LogTC/decoder errors. No engine source or test is changed and the lane is not retried.
