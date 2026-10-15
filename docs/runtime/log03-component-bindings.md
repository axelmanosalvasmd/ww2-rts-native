# LOG-03 complete initialization component

Root assigned LOG-03 to BASE_AUDIT for the single logistics batch LOG-03/04/10/15/24. This readback preserves the existing three-row delivery at `c350b9b5` and changes no parity cell. LOG-24 remains held until its real enemy arrival causes the network rate, grace and eventual cut observations and the resulting proof is reviewed.

## Complete selected Source check

At reference `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`, read the full definitions and tests in `test-logistics.mjs:5-11` and `:22-51`, and the actual producer `shared/logistics.js:38-55`. The first selected test checks rifle ammo and maximum285, provisions120, infantry fuel/emergency null, warning null and forced false; tank ordinary fuel180 and emergency60; reinitialization after setting rifle ammo7 preserves7; an unarmed medic has maximum0. The endurance test checks tank30, mortar12, rocket40 and a custom seven-model rifle with interval1.8 has maximum350.

These are pure initialization and identity checks. The Source tests construct component facts and call initializeUnit directly. An additional Classic or World whole-match gate would change their scope.

## Actual native producer and comparisons

`LogisticsState::initialize_unit` in `rust/crates/core/src/logistics.rs:264-320` applies the eligibility guard, then inserts through the canonical unit-ID entry. `Definition::from_value` at `:92-126` reads actual catalog/custom weapon metadata. Initialization uses armed eligibility, then salvo rockets, per-model strength or a single projectile, and stores the complete new reserve record. The existing method is called by Match initialization at `match_logistics.rs:66-79`; that caller is separate from this selected pure component check.

`logistics::modes_initialization_and_weapon_prices_match_javascript` in `core/tests/logistics.rs:123-185` calls this actual producer into a fresh canonical LogisticsState for each of708 stored initialization inputs. All118 catalog/custom/null definitions are represented under six owner/guard/type conditions. It compares the returned full reserve record with original-function output using the existing exact numeric comparator. The initialization branch does not use the fixture helper that decodes expected reserves into native state. Expected records are comparison operands only.

`harness/logistics-fixture.mjs:19-24` records the unchanged original initializeUnit result. Named catalog definitions resolve through the actual compiled catalog on the native side; custom definitions use the existing decoded Catalog path. The fixture records these eligible initial values:

| Definition index | Kind | Ammo and maximum | Provisions | Fuel | Emergency |
| --- | --- | --- | --- | --- | --- |
| 0 | rifle | 285 | 120 | null | null |
| 3 | tank | 30 | 120 | 180 | 60 |
| 4 | rocket | 40 | 120 | 180 | 60 |
| 9 | mortar | 12 | 120 | null | null |
| 19 | medic | 0 | 120 | null | null |
| 26 | conscript | 350 | 120 | null | null |

All these records also compare ammo price/credit, null warning and false forced/stranded/exhausted flags. Full fields remain in the fixture; the table does not replace their checks.

## Custom rifle350 correspondence

The selected original custom rifle is the original rifle definition with models7 and weapon interval1.8. The existing stored350 case is the actual catalog conscript, not a newly executed custom-rifle capture. Both pass the exact same participating initialization branches:

| Producer input | Original custom rifle | Existing catalog conscript |
| --- | --- | --- |
| Owned eligible mobile infantry facts | eligible | eligible |
| Weapon range positive | 28 | 24 |
| Weapon interval positive | 1.8 | 1.8 |
| Some positive infantry or vehicle damage | 3 / 1.5 | 2.2 / 1.1 |
| Salvo | absent/false | absent/false |
| Per-model | true | true |
| Models | 7 | 7 |

Range and damage participate only in armed eligibility here. Their different positive magnitudes do not enter ammo arithmetic. Name and movement speed do not enter it either, after eligibility. Both therefore reach the same actual producer expression: ceil(90 /1.8) times7, producing350. This explains coverage correspondence from the opened Source and native functions. It is not a translated CPU model, a new native execution, or a claim that the fixture contains the literal custom-rifle input.

## Stable identity

`logistics::reserve_constants_and_stable_identity_are_explicit` at `core/tests/logistics.rs:272-324` compares the actual reserve constants with the original export, initializes real native reserve entries for IDs9/2/5, consumes three rounds from ID5 through the actual producer API, then reinitializes that same entry and asserts complete canonical state equality. The Source test uses a remaining ammo value7; the native test uses a real debit from285. Both selected checks prove reinitialization preserves the current record, including partially used ammo. The production entry operation retains the entire existing reserve record rather than replacing its stock. The native test also checks stable ID order and removal.

## Producing-head evidence

Primary hosted run [38740304188](https://github.com/axelmanosalvasmd/ww2-rts-native/actions/runs/38740304188) checked out `396f6d8a7014bd4a5ebe64f0a12ccd66865bf452`. The named initialization, constant/identity and transition tests have actual PASS entries on [Ubuntu](https://github.com/axelmanosalvasmd/ww2-rts-native/actions/runs/38740304188/job/116439969938), [Windows](https://github.com/axelmanosalvasmd/ww2-rts-native/actions/runs/38740304188/job/116439970061) and [macOS](https://github.com/axelmanosalvasmd/ww2-rts-native/actions/runs/38740304188/job/116439970306). The production module, catalog, recorder, native logistics tests and fixture blobs are identical from that producing head to post-G1 main `7c9df2d581a79de48fbec0cb4d7ab8e98b488d43` and the preserved three-row delivery.

This readback executes no Source or native job and assigns no fresh PASS. Root approves the component scope without an extra whole-match gate. LOG-24 now genuinely passes at producing native head9efa. The five-row local status proposal includes LOG-03 and remains subject to independent exact-head review; this readback does not turn that review into owner acceptance. All55 Acceptance cells and every unselected row remain preserved.
