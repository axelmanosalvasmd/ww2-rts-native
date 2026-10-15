- Mark MOV-14 ported after the reviewed Source-backed movement test passed on the Vostro (1 passed, 0 failed, 0 ignored). It compares 13 compiled vehicle profiles exactly and checks 26 forward/reverse starts while retaining the 636-call replay. MOV-15 through MOV-22 keep their full-check gaps and in-progress statuses. Runtime and fixtures are unchanged.

## Vehicle parity audit uses the existing component recording (2026-10-15)

The MOV-14 to MOV-22 audit keeps `movement.json`, its Source recorder and the runtime unchanged. The existing movement replay test now checks all five compiled vehicle-profile numbers against the recording's independent table as floating-point bits, and applies the pinned Source lab's forward/reverse start assertions to ten named profiles and three fallback kinds (13 profiles, 26 starts). The exact profile-name set and compiled profile count prevent an omitted kind from passing silently. The focused ci-test passed on the Vostro at `84ae5e6b9010606b1e9b928347cdb6730a47b5bc`: 1 passed, 0 failed, 0 ignored and 0 filtered. Separate review and Root inspection confirmed the complete stated MOV-14 check, so this documentation child marks only MOV-14 ported.

The three-tick recording still compares 636 movement calls and 1,362 post-separation/progress states. Its workload disables route searches, reaction, weather/debris evolution, crushing and wear, and uses physical terrain for voluntary routing. The replay restores states at component boundaries. This cannot establish the all-type journeys, completed braking/corner timing, ordered short retreat, remembered-terrain decisions or Horde recovery required by the other eight broad rows. Their statuses stay in progress. No runtime defect is inferred, and the owner decision on whole-match parity remains pending.

## Vehicle component audit (MOV-14 to MOV-22)

The existing `movement_matches_the_reference_call_by_call` test now compares all five numeric profile fields for each of the 13 recorded vehicle kinds against the compiled rules, bit for bit. It also runs the original `test-movement-lab.js:45-53` forward and reverse start checks: ten named profiles and the lorry, train and Kaiju fallbacks, 26 starts in total. The client-only `tracked` flag remains outside this Rust check. At `84ae5e6b9010606b1e9b928347cdb6730a47b5bc`, the exact focused ci-test passed on the Vostro: 1 passed, 0 failed, 0 ignored and 0 filtered. Separate review and Root source inspection confirmed the complete MOV-14 profile/start check. MOV-14 is ported on the Rust side; client acceptance remains separate.

The same unchanged `movement.json` recording supplies the vehicle call evidence below. Its existing replay asserts three ticks, 636 movement calls and 1,362 post-separation/progress unit comparisons. Its vehicle calls cover armoredcar, flaktrack, medium, rocket and tiger, five vehicle kinds. The recorder uses the frozen-workload patches: route searches and reaction are disabled, physical terrain replaces remembered routing, and weather/debris evolution, crushing and wear are disabled. Replay restores recorded state before calls. These are component comparisons, not a completed unmodified Source match or a full movement-lab journey.

| Row | Recorded evidence | Check still needed before `ported` |
|---|---|---|
| `MOV-15` | Wall contacts and a medium hull over the map edge | Delivered hull-footprint precision, all classifications and oriented cell-box boundary cases |
| `MOV-16` | Tiger clearance and flaktrack sideways escape | All 11 required types completing clearance/corner journeys, plus overlap and sampling boundaries |
| `MOV-17` | A medium facing a cliff and a hull over the edge | Three exact diagonal corner-height cases in both directions and the lab cliff checks |
| `MOV-18` | Bend/braking/reverse calls and all 26 profile starts | Stop within the original braking distance and the light-tank corner within 9 seconds |
| `MOV-19` | Contact, stopped-unit and wait calls | Complete opposing and parked journeys for all required types |
| `MOV-20` | Pre-set medium reverse and voluntary yielding | Engaged short-retreat command selection and reverse/reverse-obstacle journeys |
| `MOV-21` | Two-metre clearance, side slide and clearance completion calls | Clearance-stop and the Horde gate check |
| `MOV-22` | A flaktrack approaching a bend at its turning lead | Complete corner/road/queue journeys and remembered-terrain, yielding and escaping branches |

MOV-15 through MOV-22 remain in progress. Their runtime helpers exist; the missing items above are full-check evidence, not newly diagnosed runtime defects. MOV-09 and the pending whole-match parity decision are unchanged.

## Inventory evidence notes

| Row | Evidence note moved from the inventory |
|---|---|
| `MOV-14` | port: compiled profiles and original start checks pass in the [vehicle component audit](#vehicle-component-audit-mov-14-to-mov-22) |
| `MOV-15` | port: [vehicle component audit](#vehicle-component-audit-mov-14-to-mov-22); full row check pending |
| `MOV-16` | port: [vehicle component audit](#vehicle-component-audit-mov-14-to-mov-22); full row check pending |
| `MOV-17` | port: [vehicle component audit](#vehicle-component-audit-mov-14-to-mov-22); full row check pending |
| `MOV-18` | port: [vehicle component audit](#vehicle-component-audit-mov-14-to-mov-22); full row check pending |
| `MOV-19` | port: [vehicle component audit](#vehicle-component-audit-mov-14-to-mov-22); full row check pending |
| `MOV-20` | port: [vehicle component audit](#vehicle-component-audit-mov-14-to-mov-22); full row check pending |
| `MOV-21` | port: [vehicle component audit](#vehicle-component-audit-mov-14-to-mov-22); full row check pending |
| `MOV-22` | port: [vehicle component audit](#vehicle-component-audit-mov-14-to-mov-22); full row check pending |
