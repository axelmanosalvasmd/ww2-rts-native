# MOV03-08 consumer readiness, source only

Private branch starts at fixed main396f and preserves d1e426dd/40229feb.
Published PR256 at6fb63193 is read-only; queue256 then259 remains with Root.
No parity file or status cell is edited. Source, Node syntax checks, Cargo,
rustc, formatting, native tests and test discovery have not executed here.
All generated fixtures remain absent. Five registered functions are authored,
not compiled or passing tests. No provider, guard, terrain implementation,
ABI, reference, runtime policy or expected existing fixture changes.

Read coordination/contracts, pinned e4e579a callbacks and issue5 through
6194245283. That readback retains the accepted row scope and pending hosted
PR256 checks; it is not permission to change outcome rules or publish this work.
The six ABI2 calls remain the agreed boundary. Broader contracts remain drafts.

## Complete original checks and the bounded companion

The original recorder extracts complete callback/helper bodies from the pinned
files. Its wrappers observe real calls; no Source body, assertion or helper is
rewritten. The generate/check interface changes only output handling.

| Original callback | Pinned lines | Static assertion sites | Native scope in this candidate |
|---|---|---:|---|
| Roads and mud | test.js:7503-7534 | 9 | Five distance inequalities: road, partial road, shallow/deep mud, infantry negative control. Four sibling path-selection assertions stay in the complete raw capture. |
| Ground that wears | test.js:7586-7617 | 11 | Original half-churn steady-distance inequality. Remaining wear/client/crater/routing/ford assertions stay in raw capture; no broader ENG completion is claimed. |
| Slope | test.js:7635-7645 | 2 | Both uphill/flat and downhill/flat physical inequalities, unchanged. |
| Wind, weather, dust and fire | test.js:7648-7692 | 17 | Selected wet/dry physical ratio, plus smoke drift, dry/wet dust, rain ramp, soak interval and owner weather row. The weather-off and fire/privacy siblings remain in the full raw capture. MOV09 remains paused. |
| road march: calm units move faster on their own land | test-world-rules.mjs:47-60 | 2 | Original constructor rifle, public admission on both legs,300 warmup plus40 calm plus40 hit-every-step physical ticks, original>1.3 inequality. The command assertion is invoked twice. |

Total41 static assertion sites. This number describes Source text, not an
executed assertion count or41 native tests. The two companion packets require
the separately generated original capture, bind its bytes and callback IDs,
and refuse an original failure. They do not replace original cleared-map
admission. Any first Source error retains the partial packet before throwing;
check mismatch keeps the differing packet beside the original output.

## MOV05/06/07: canonical public construction and purchase

`harness/mov-terrain-scenes.json` declares12 scenes and no expected output.
All use real Source createGame and native Match::new with every constructor
actor, building, footprint and sidecar retained. Source sim.js:159 starts
Conquest with rifle/rifle/MG. The older draft's constructor-engineer lookup
was invalid. Source CFG.fortBuilders at:68 and builderTypes at:379, and native
economy.rs:239-248, admit the retained rifle as a builder in Conquest. Both
companions now check that policy, select that real rifle and issue public Build
for a Motor Pool. No engineer is fabricated or imported. Native construction
admission is match_construction.rs:764 onward; the ordinary full tick advances
actual crews and completion. A2400-tick cap is a setup assertion, not a way to
assign built=1 or bypass a refusal. A real Stop ends the builder's assignment.

Every tank is bought through that actual completed Motor Pool. Every measured
rifle is bought through the retained HQ, whose genuine makes list includes
rifle (Source:350). Commands are executed through sim.command and native
Match::execute_command. Recruitment keeps real maker/rally/production and
Combat/Firing/Ability/Recovery owners (match_recruitment.rs:375-390). Wallet
debits, spending, actual births and maker bindings are checked. No staged birth,
initial_unit call, direct insertion, hidden removal or expected-state restoration
is used. The only pose/stance/route inputs for measured actors are disclosed
counterparts of original put/half-road/wet-reset inputs. Recipient observation
after those inputs uses actual snapshot/grid/vision/routing providers.

The80x96 authored companion keeps original lane coordinates in its top20 rows
and places both real bases far below them. All base actors remain active with
constructor controls; they are not made quiet or stripped. Measurement-band
wear inputs stop above every footprint. Hill heights retain the original
10-zero/10-one step in the top20 rows. This changes map bounds, roster,
constructor terrain, production work and elapsed setup time. It therefore
does not establish literal admission of the original cleared/temporary-maker
fixtures. Root must review this disclosed companion before row eligibility.

| Scene group | Authored physical window and complete selected inequalities |
|---|---|
| grass/road/edge/shallow/deep/rifle-mud/rifle-grass | Each40 acceleration then40 measurement ticks. road/grass within0.1 of roadSpeed; edge/grass strictly>1.03 and<roadSpeed-0.1; mud ratios within0.1 of both mudSpeed values; infantry distance difference<0.2. The single edge scene serves both MOV05/06. |
| churn-reference/half-churn |40 acceleration then20 measurement ticks. Ratio within0.06 of1-churn*0.5. The reference retains the original extra idle tank, bought normally. |
| hill/flat | Both up/down actors share each scene, receive one real Move batch and run40 ticks. Uphill trails flat by>0.5; downhill differs from flat by<0.3. |
| dry/wet |100 smoke ticks;40+20 dry movement; force the original next-rain input and run800 ticks; require rain1 and wet strictly0.15..0.5. Apply original wet1/pose/path/reset inputs, issue real Move, then40+20 wet ticks. Wet/dry ratio remains within0.06 of1-wetGround; dry dust is present and wet dust absent in actual owner rows. |

terrain.rs:246-295 supplies groundMul and five-point speedMul; movement.rs:256-290
supplies bilinear one-metre lookahead, uphill-only clamp and speed composition.
match_tick.rs:611-614 calls routed movement then canonical ground contact;
match_ground_contact.rs:84 onward applies wear. Each selected physical step
compares motion fields against Source without loading them into Match. An
actual constructor, construction, Buy, route, snapshot, contact or tick refusal
must stop. Source and native retain independent random owners; an ambient actor
or RNG difference is a diagnostic, not permission to quiet or remove that actor.

## MOV08: actual World public caller

`mov-world-distance-fixture.mjs` records unmodified generated seed42 geography,
two players, original names/teams/factions/options and all constructor actors.
Only the serialized region-map input is run encoded; the live map is unchanged.
Native expands it into AuthoritativeMap and calls Match::new_world with native
MatchRandom seed29448, without replacing any stream or importing Source state.
The constructor exists at match_world.rs:200-242; it rejects missing geography,
wrong mode, enabled logistics, mismatched native seed, legacy points or triggers.
No generic synthetic World helper or manually inserted actor is used.

The first branch performs all300 idle ticks, then both40-tick legs. Each leg
restores only the original authored start/path/orders/worldGoal inputs and sends
public Move. Fighting sets the actual combat hit timestamp before each physical
step, as the original does. Travel is accumulated with the core's hypot from
actual before/after positions, not from a scalar. The unchanged>1.3 inequality
and exact recorded selected-actor motion are both checked.

A separate branch sets declared last-hit/last-shot times to0 at construction,
runs199 real idle ticks, then measures actual Move displacement on tick200
and, after the same authored reset and public order, on tick201. The strict
calm boundary in match_world_ground.rs:192-238 must act physically. The new
diagnostic requires movement and a1.5 ratio within1e-10 for coordinate
subtraction; it also compares both displacements exactly to recorded Source.
This bound belongs only to the new supplemental diagnostic. It never replaces
or relaxes the original>1.3 check. Root review of that diagnostic remains pending.
Total World branch ticks:380+201=581; this is a plan, not an execution result.

The full runner performs real preflight (match_tick.rs:283 onward,
match_world_tick.rs:64 onward), memory selection and World command prelude.
World tactical request, naval/carrier, construction/gate and active weapon
boundaries remain explicit. A refusal or a divergence of the authentic
constructor/selected physical trajectory is retained as the first failure.
The candidate does not infer that an available constructor proves every tick
of the original map is admitted. World eligibility stays held until full proof.

## MOV03 and MOV04 remain held

MOV03's inherited acceptance asks for600 per-tick diffs of the seeded Six
Fronts Conquest bench. Original tools/bench.mjs:25-26,49-99 uses seed0x3c1055,
six Massive AI seats, step then actual aiTick and periodic recipient snapshots.
The whole Source per-unit block sim.js:5374-5583 contains order routes,
reaction, speed/traffic/replan/mover, velocity, fuel, grid, contact, still/stuck,
facing and targeting. Current ordinary caller order is inspectable at
match_tick.rs:495-620, but no complete600-tick consumer is supplied here.

PARITY's judgment takes precedence over that older whole-match bitwise wording.
Proposed outcome criterion for Root/Axel agreement before a run: same authored
map/seat/faction/army input matrix and seed set; compare winner/end reason,
duration, alive/loss counts by kind, actual spending, route completion and
arrival/stall distributions. Owners must fix metrics, observation windows and
numerical acceptance limits before execution. This is an explicit proposal,
not an adopted tolerance or an approval inferred from a comment. Bounded
recorded-input movement phase comparisons remain separate from a full AI match.
No Source bench outcome or branch activation has been measured here.

MOV04's complete factor matrix still needs the Source fuel/emergency scalar
at sim.js:5486 and consumption at:5509. Native movement.rs:284-290 composes
pace, retreat/sprint/suppression, terrain, slope, weather and march but has no
fuel scalar. match_tick.rs:298-300 rejects enabled logistics/convoys with
`reserve and convoy tick phases are not connected`. No fuel-free witness can
complete the full row. Work on that provider requires declared runtime/LOG
ownership and approval. MOV03 also retains known crushing/mine caller limits
at match_ground_contact.rs:60-78; this is a scope finding, not proof that the
unexecuted seeded bench reaches them. No guard or order is weakened.

## Exact pending commands and counts

No slot is granted by this plan. Use /usr/bin/node24.21.0 in Root's explicit
recorder scope. Preserve the first raw failure and stop before another stage.

```sh
WW2_REFERENCE=/home/judiazm/Projects/ww2-rts-native/reference/ww2-rts /usr/bin/node24.21.0 harness/mov-owned-speed-checks-draft.mjs /tmp/mov-original-speed.json.gz
WW2_REFERENCE=/home/judiazm/Projects/ww2-rts-native/reference/ww2-rts /usr/bin/node24.21.0 harness/mov-owned-speed-checks-draft.mjs /tmp/mov-original-speed.json.gz --check
WW2_REFERENCE=/home/judiazm/Projects/ww2-rts-native/reference/ww2-rts /usr/bin/node24.21.0 harness/mov-terrain-companion-fixture.mjs /tmp/mov-original-speed.json.gz /tmp/mov-terrain-companion.json.gz
WW2_REFERENCE=/home/judiazm/Projects/ww2-rts-native/reference/ww2-rts /usr/bin/node24.21.0 harness/mov-terrain-companion-fixture.mjs /tmp/mov-original-speed.json.gz /tmp/mov-terrain-companion.json.gz --check
WW2_REFERENCE=/home/judiazm/Projects/ww2-rts-native/reference/ww2-rts /usr/bin/node24.21.0 harness/mov-world-distance-fixture.mjs /tmp/mov-original-speed.json.gz /tmp/mov-world-distance.json.gz
WW2_REFERENCE=/home/judiazm/Projects/ww2-rts-native/reference/ww2-rts /usr/bin/node24.21.0 harness/mov-world-distance-fixture.mjs /tmp/mov-original-speed.json.gz /tmp/mov-world-distance.json.gz --check
```

After successful recording and explicit Root fixture freeze, install the two
companion outputs at the native runtime-read paths
`rust/crates/simtest/tests/fixtures/mov-terrain-companion.json.gz` and
`rust/crates/simtest/tests/fixtures/mov-world-distance.json.gz`. Keep the original
raw packet separate. Neither native fixture exists in this checkpoint.

The five unique native functions registered in simtest/tests/it/main.rs are:

- `mov_terrain_companion::original_roads_and_mud_distances_include_partial_road`
- `mov_terrain_companion::original_half_churn_and_wet_ground_distances`
- `mov_terrain_companion::original_uphill_and_downhill_distances`
- `mov_world_distance::original_public_world_300_plus_40_plus_40_distance`
- `mov_world_distance::physical_world_march_calm_boundary_200_and_201`

Future focused selection, only in a separately granted scope, after fixture
freeze and any required format/lint gates:

```sh
cd rust
df -h /
WW2_CARGO_SLOT="${ROOT_SLOT:?Root slot grant required}" CARGO_BUILD_JOBS=4 python3 /home/judiazm/.t3/handoffs/ww2-native-cargo-guard.py +1.99.0 nextest run --locked --cargo-profile ci-test --build-jobs 4 --test-threads 1 -p ww2-core -p ww2-simtest --test it -E 'test(mov_terrain_companion::) | test(mov_world_distance::) | test(=every_test_file_is_compiled)'
```

Expected static selection:5 new behavior instances plus both existing package
registration guards=7. Current new execution count:0. This is neither a grant
nor a claimed nonzero runtime selection. Current guard/slot target,20GiB floor
and exact Root resource caps apply. No Source/full/release/discovery repetitions
or publication are authorized by these commands.

MOV05/06/07 and08 remain unverified companion candidates. MOV03/04 stay held
for the stated criterion/provider gaps, MOV09 paused, MOV72 held. No row is
ported by source preparation; published PR256 and every incoming fixture/row
remain untouched. Integration onto newer main waits for Root and the queue.
