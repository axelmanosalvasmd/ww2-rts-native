# R1 whole-match differential prototype

This diagnostic runs a 600-tick Conquest prefix in the unchanged pinned JavaScript prototype and the canonical Rust Match. It compares startup and every tenth tick. Private per-tick frames also locate the first difference precisely. A completed run is not a completed victory, a socket test or a parity pass.

The first measured run completed 600 actual ticks in each engine, with movement during 352 ticks in each and 132 emitted native events. Both accepted all ten recorded orders. The first real difference was at startup, `$.units[0].x`: Source 27 metres (`403b000000000000`) versus Rust 85 metres (`4055400000000000`). Zero of 61 checkpoints matched; zero actual ticks matched. MODE-22 owns the initial unit position-list check. MODE-21, IF-20 and IF-21 are related investigation areas, without a claim about the exact cause. No row status changes.

The earlier first run exposed a comparator defect: Source JSON `100` and native JSON `100.0` had identical IEEE bits but unequal serde JSON storage variants. Its original red log and source snapshot were preserved privately. Root independently verified the defect before the numeric-wrapper fix. The focused comparator check now compares validated exact IEEE bits, retains signed zero and rejects inconsistent decimal renderings. This fixes diagnostic representation, with no tolerance or simulation-state change.

## Inputs and boundaries

The fixed prototype profile is Default's committed authoring, Conquest, Standard army, clear weather, sandbox off, two opposing human commanders, shuffle/tech/supply/living weather enabled and logistics disabled under owner decision 097. Weather seed is omitted in Source and None in Rust's existing constructor. The explicit match seed is a decimal u64 and stays in the private trace. Both engines receive that same seed and the same absolute order stream.

Source has no recorded match-seed constructor argument. The recorder supplies one global SplitMix64 stream to its original Math.random calls from before createGame through all 600 original step calls. Rust retains its existing independent purpose streams. No state, wind, environment seed, actor, footprint, terrain, random draw, clock or expected checkpoint is copied between engines. No Source import hook or runtime patch is used. Equal input seed is not a claim of equal draw ownership or constructor state.

Canonical starting force IDs are verified on the Source side. Ten orders use existing Stance, Move, Stop and attack-move commands for both owners. Hold Position is a legal stance; it preserves explicit destinations while allowing the scheduled Moves. The six initial troops and all starting structures remain. There is no actor deletion, teleport, synthetic map or empty/no-op substitute.

The projection compares unit ID, kind, owner, x/z and health; player team and MP/Munitions/fuel/VP/income; ordinary runtime mode; winner and VP scores; and objective owner/capper/progress/cut state. Floating values carry exact IEEE bits and readable decimals. Source resource-field absence is represented explicitly, rather than changed to native zero. This is a declared projection, not every internal field or every broad parity-row variant.

## Private reproduction

Use x64 Node v24.21.0 and a clean reference checkout at e4e579a838aabae3b1d9dadbc17a0a87147bc52d. The recorder checks its Git pin and tracked cleanliness before and after execution. Choose a private output directory and set R1_MATCH_SEED privately.

```sh
/usr/bin/node harness/r1-whole-match-differential.mjs \
  --reference "$WW2_REFERENCE" --seed "$R1_MATCH_SEED" \
  --output "$R1_PRIVATE_DIR/source-trace.json"

cargo +1.99.0 run --manifest-path rust/Cargo.toml --locked --profile ci-test \
  -p ww2-simtest --bin r1-differential -j4 -- "$R1_PRIVATE_DIR/source-trace.json"
```

Native commands use the coordinator's allocated systemd scope, memory/swap limits, CPU quota and verified taskset. The shown Cargo command is its payload. Keep the full trace and raw logs private. Published summaries contain the observed counts and first differing values, not the seed, full option receipt or full state trace.

The default comparator exits 0 only for a complete match of this projection, 1 for a complete but different run, and 2 for invalid/incomplete input or a runner fault. It continues every actual tick after a difference. A genuine native runner error stops native execution, records the failed boundary and internal tick, and leaves the prefix intact. Source exceptions likewise retain completed Source frames. Constructors and guards are never weakened to reach tick 600.

`--report-only` explicitly exits 0 for a complete diagnostic run with actual movement and nonempty orders. Its JSON still says `differentialVerdict: MISMATCH` when states differ. This checks harness execution, not equivalence. It never converts incomplete/faulted execution into success. Keep the default red result separately from that diagnostic success.

The named focused comparator test is:

```sh
cargo +1.99.0 test --manifest-path rust/Cargo.toml --locked --profile ci-test \
  -p ww2-simtest --bin r1-differential -j4 -- --nocapture
```

The owner proposal at issue #5 comment 6181987121 remains pending Axel. Neither report-only success nor one 600-tick prefix permits a row to move to ported. Full matches, victory, other maps/weather/modes/rosters, logistics on, unit variants absent from the trace, network delivery and Unreal adoption need their separate evidence and decisions.

## Candidate row limits from Hermes 105

This table names the observable diagnostic probe for each proposed row. It grants no row coverage: the run differs at tick 0, and none of these rows receives its complete approved check from this prefix. Tick ranges describe recorded projections or submitted commands, not instrumentation proving every internal branch executed. N10 means ticks 0,10,...,600; private frames also contain every intervening tick.

| Candidate row | Probe and ticks | Missing full-check branches or assertions |
| --- | --- | --- |
| MODE-12 | Opposing team/owner fields, 0..600 | Allied cooperation, self-alliance edge cases, World locals, shared victory |
| MODE-14 | Initial unit position lists, tick 0 | Layout score/distance calculation, tie sets, 3%/97% selection, shared spawns |
| MODE-19 | Initial rifle/rifle/MG IDs/kinds, tick 0 | Other modes, shared HQ offsets, empty enemy, MP variants |
| MODE-22 | Initial IDs/kinds/owners/positions, tick 0; first mismatch assigned here | Complete constructor order and every mode; this run fails its position-list equality |
| MODE-23 | Objective owner/capper/progress/cut, 0..600 | Every eligibility, neutralization, ally, contest and prerequisite case |
| MODE-25 | Player VP/income and point ownership, 0..600 | Away/uncut/winner branches and exact causal income assertions |
| MODE-26 | VP projection, 0..600 | Goal itself and 2v2/2400 VP are not projected or reached |
| MODE-27 | Winner projection, 0..600 | Win threshold, slot-order tie and end spot are not asserted |
| MODE-91 | None | Story counters are not projected |
| MODE-93 | None | Timeline and terminal sample are not projected |
| CMB-43 | Initial health and unit identity, tick 0 | Suppression/XP/targets/autocast/auto-retreat/facing defaults are not projected |
| CMB-44 | Health/position endpoints, 0..600 | canShoot eligibility, visibility, hull, ammo, salvo and LOS branches are not asserted |
| CMB-46 | Health/position endpoints, 0..600 | Readiness/cooldown/setup/low-ammo/area-weapon fields are not projected |
| CMB-47 | Health endpoints, 0..600 | Direct-fire per-round draws/aim/accuracy are not aligned or independently asserted |
| CMB-51 | Health endpoints, 0..600 | Damage scaling, armor/tech/cover variants, hit memory and XP are not projected |
| CMB-52 | None | No moving vehicle or Kaiju; armor-facing full check absent |
| CMB-54 | Position/health endpoints, 0..600 | Suppression bands, once-per-volley policy and immunity are not projected |
| CMB-55 | Position endpoints, 0..600 | Suppression, threshold, reload, accuracy and trench recovery are not projected |
| CMB-83 | holdPos orders at tick 1 for both owners | Invalid keys/types/foreign selections and recipient-only bit privacy are not asserted |
| CMB-91 | attack-move orders at tick 140, position/health thereafter | Target eligibility/halt/resume/arrival subcases are not separately asserted |
| CMB-124 | None | Vision range, rain, elevation, tower, garrison and plane fields are not projected |
| CMB-125 | None | Enemy seeing, LOS, camouflage, dust, recon and ship-hull policy are not projected |
| CMB-126 | None | No planes; visibility branches absent |
| CMB-127 | None | Bunker/carrier/Horde/World visibility and recipient filtering are not projected |
| MOV-03 | Move tick 2, Stop 120/400, attack-move 140; positions 0..600 | Internal phase order and other handlers are not instrumented |
| MOV-04 | Infantry position endpoints, 0..600 | Exact speed factors and vehicle/logistics/retreat/sprint branches are not asserted |
| MOV-05 | Infantry position endpoints, 0..600 | Per-ground factors and vehicle/ford/depth/wetness branches are not asserted |
| MOV-06 | Infantry position endpoints, 0..600 | Five-point samples, blocked-cell skips and boat branch are not projected |
| MOV-07 | Infantry position endpoints, 0..600 | Exact sampled grade and vehicle/downhill branch cases are not projected |
| MOV-10 | None | No together=true order or mixed-speed group |
| MOV-11 | Infantry Move/Stop/attack-move endpoints, 0..600 | Route budgets, backward movement, yielding, blockage and retry clocks are not projected |
| MOV-12 | Infantry position endpoints, 0..600 | Waypoint/corner traversal and end-spot exactness are not separately asserted |
| MOV-13 | Position endpoints, 0..600 | Ray sets, cliff handling and grid-walker limits are not independently checked |
| MOV-24 | Position endpoints, 0..600 | Stuck counters, recovery budget and vehicle replans are not projected |
| MOV-27 | None | No moving vehicle; radius-clear click placement is absent |
| MOV-29 | Position endpoints, 0..600 | Remembered-vs-hidden routing policy and World team memory are not projected |
| MOV-31 | Position endpoints, 0..600 | Fine A* corners, mine and level cases, coarse-leg membership are not independently checked |
| MOV-32 | Position endpoints, 0..600 | Path cost terms are not projected or independently checked |
| MOV-33 | Position endpoints, 0..600 | Heuristic, heap ties and vehicle-road cases are not projected |
