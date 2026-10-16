# IF-RUNTIME-2: recipient cover and deterministic state

This branch starts at main `992633f820d3d9f7708d1135638f18ca69d7c752`,
with prototype `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`. It imports
nothing from PR #312. Row claims cover the server/core portion only.
Unreal adoption and additional C queries remain separate work.

## Candidate row checks

- IF-07: `if07_cover_classes_house_edges_and_exploration_match_prototype_rules`,
  `if07_directional_cover_uses_delivered_damage_hulls_and_settled_wrecks`,
  `if07_preview_survives_ordinary_ticks_and_complete_recipient_snapshots`.
  The internal query classifies explored terrain and garrison entry edges,
  then uses the existing simulation cover calculation plus delivered live
  ground hulls. Dead units, infantry, aircraft, distant hulls, flanking hulls
  and moving wrecks do not provide directional cover. The test checks damaged
  terrain, both shooter directions, unknown World cells and unexplored ground.
  The prototype rules are in `client/cover-preview.js:13-58` and
  `shared/sim.js:2308-2340`; its behavioral cases start at `test.js:3572`.
- IF-14: `if14_recipient_view_cannot_recover_undelivered_enemy_rows`.
  Thirty-two variants of hidden authoritative enemy positions and health
  remain absent after projection and ClientView decoding. Full replacement
  and match restart remove prior rows. Direct review of the unchanged
  `rust/crates/ffi/include/ww2_core.h` finds only a recipient-view handle and
  caller-supplied formation data, with no authoritative game handle.
- IF-20: `if20_match_streams_replay_across_rooms_and_unrelated_ai_draws`.
  Equal seeds produce equal startup units and forty ordinary combat ticks.
  Recorded Hold Fire orders then stop firing, eight real ticks settle the
  flights, and complete recipient snapshots compare equally. Another room and unrelated AI
  draws cannot perturb that simulation. The test requires a real Combat
  stream draw, compares all other used purpose states and resumes each
  defined purpose from its recorded state.
  `interface_stream_consumers::if20_smoke_scatter_consumes_only_the_off_map_stream`
  exercises actual support admission and smoke scatter, checking exact OffMap
  consumption after perturbing all other purposes. The existing
  `commander_ai::production_randomness_is_confined_to_one_ai_seat` checks the
  actual per-seat planner consumer. Existing
  `weather_runtime::live_weather_matches_original_steps_and_order` and
  `weather_runtime::environment_random_matches_original_and_resumes` check
  environment draw order and continuity. This checks the approved native
  purpose-stream design in PARITY.md, rather than the superseded shared
  JavaScript random stream. Existing `runtime::purpose_streams_resume_and_do_not_depend_on_other_draws`
  checks all purpose streams and bounded integer draws directly.

The new cover query uses the simulation's damage-dependent strength, as
IF-07 permits. The browser mirror approximates that strength from damage
stages and cannot distinguish every health value within stage 1. The Rust
query uses only the strength already reconstructed in recipient terrain.
It does not obtain exact health from an authoritative world. House entry
checks require a known walkable neighbor; an unknown World neighbor cannot
prove an entry edge. Glyph colors, rings, cursor selection and drawing are
client work and are not claimed here.

## Runtime state audit

The query borrows `RouteKnowledge` and explicit explored bits, bodies and
wrecks. It writes no Match field, unit flag, command, event or link. Its
result is temporary. Searches of tick admission, recipient projection,
elimination and command guards found no new persistent state to admit.
The ordinary-tick test still runs eight real ticks and collects complete
recipient snapshots before and after each query, asserting no fault and
no snapshot change. The combat replay test independently crosses forty
combat ticks and eight settling ticks before complete snapshot collection.
Persistent-flight snapshot admission stays closed; this test does not
claim that active flights can be delivered.

The FFI, C header, wire format, contracts, privacy rules, reference pin and
parity status files retain their existing content. This batch adds no Source
recorder, captured fixture or provenance bundle.

## Rows left out of this batch

- IF-03: the recipient availability query, including all action denials,
  remains missing. Existing stateless economics alone is partial evidence.
- IF-04: recipient building/fort preview coverage is incomplete; ordinary
  fort admission still rejects bridge, fill, wall and gate.
- IF-08: complete map, layer, road, territory and weather helper coverage
  is broader than the cover query.
- IF-12: editor authoring and seeded whole-AI fairness execution are not
  complete.
- IF-22: complete ordered collection behavior is unproved. In particular,
  native aid fields traverse a BTreeMap, while the prototype uses insertion
  order. Navigation and contact iteration also need dedicated checks.
- IF-25: the V8 sort helper is insufficient to qualify all callers. Some
  AI and convoy paths use `total_cmp`, with different NaN and signed-zero
  semantics; those consumers need admission and tie checks.
- IF-26: signed-zero folding in destination grouping does not prove all
  construction, handoff and JSON equality consumers.
- IF-27: server save/load admission and complete runtime restoration remain
  unavailable.
- IF-28: the recorded tick queue does not cover operative server ingress
  throttling or the complete away/pause replay log.
- IF-29: a timing meter is not an integrated host AI view/act driver.
- IF-30: seeded six-AI whole-match execution and agreed outcome checks are
  not established. The old exact JavaScript hash check conflicts with the
  approved purpose-stream and whole-match outcome rules.

No assigned row is purely Unreal presentation, so CLIENT ROWS is empty.
The mixed rows above remain unproven for their incomplete core/tool parts.

## Validation

Pending focused native tests, formatting and strict workspace Clippy in a
shared Cargo slot on the Vostro. No row is proven until its checks pass.
