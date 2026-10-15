# MODE-21: all 49 differences, grouped by observed cause

Frozen native producer: `4518bdac03abe5c49718dc2008f64c2263c8e8c9`. Authentic Source: `5139d2188f36d792d04aba3eada96d80bf641790`, pin e4e579a. Original full values and raw ordering remain in `/tmp/mode21-semantic-5139d218-0e8d6d9b/native-report.json`. Native constructor1, ticks0, five startup draws aligned. First run remains1PASS registry/1FAIL startup/144filtered; no runtime defect inferred from the comparator label.

## Group 1: 48 footprint-map iteration-position differences

Source places each building and appends its footprint cells to a JS Map (shared/sim.js:1028-1029). Native stores the same entries in BTreeMap keyed by cell (match_runtime.rs:578,:1095). The raw observer serializes each iterator in its own order. Offline comparison of all36 unique cell keys and complete id/owner/x/z values finds exact numeric bits for every keyed entry. No missing/extra/wrong footprint is observed. Source consumers use keyed get at sim.js:1202 and:5865; no buildingCells iteration consumer appears in shared/*.js. The existing deterministic-keyed-storage choice must be assessed and recorded in DESIGN, without changing runtime ordering merely to match this observer.

| No. | Exact reported path | Source value / bits | Native value / bits |
| --- | --- | --- | --- |
| 1 | `$.buildingCells.$map[3][0]` | 519 / 4080380000000000 | 445.0 / 407bd00000000000 |
| 2 | `$.buildingCells.$map[3][1].id` | 7 / 401c000000000000 | 8.0 / 4020000000000000 |
| 3 | `$.buildingCells.$map[3][1].x` | 81 / 4054400000000000 | 93.0 / 4057400000000000 |
| 4 | `$.buildingCells.$map[4][0]` | 520 / 4080400000000000 | 446.0 / 407be00000000000 |
| 5 | `$.buildingCells.$map[4][1].id` | 7 / 401c000000000000 | 8.0 / 4020000000000000 |
| 6 | `$.buildingCells.$map[4][1].x` | 81 / 4054400000000000 | 93.0 / 4057400000000000 |
| 7 | `$.buildingCells.$map[5][0]` | 521 / 4080480000000000 | 447.0 / 407bf00000000000 |
| 8 | `$.buildingCells.$map[5][1].id` | 7 / 401c000000000000 | 8.0 / 4020000000000000 |
| 9 | `$.buildingCells.$map[5][1].x` | 81 / 4054400000000000 | 93.0 / 4057400000000000 |
| 10 | `$.buildingCells.$map[6][0]` | 599 / 4082b80000000000 | 519.0 / 4080380000000000 |
| 11 | `$.buildingCells.$map[7][0]` | 600 / 4082c00000000000 | 520.0 / 4080400000000000 |
| 12 | `$.buildingCells.$map[8][0]` | 601 / 4082c80000000000 | 521.0 / 4080480000000000 |
| 13 | `$.buildingCells.$map[9][0]` | 445 / 407bd00000000000 | 525.0 / 4080680000000000 |
| 14 | `$.buildingCells.$map[10][0]` | 446 / 407be00000000000 | 526.0 / 4080700000000000 |
| 15 | `$.buildingCells.$map[11][0]` | 447 / 407bf00000000000 | 527.0 / 4080780000000000 |
| 16 | `$.buildingCells.$map[12][0]` | 525 / 4080680000000000 | 599.0 / 4082b80000000000 |
| 17 | `$.buildingCells.$map[12][1].id` | 8 / 4020000000000000 | 7.0 / 401c000000000000 |
| 18 | `$.buildingCells.$map[12][1].x` | 93 / 4057400000000000 | 81.0 / 4054400000000000 |
| 19 | `$.buildingCells.$map[13][0]` | 526 / 4080700000000000 | 600.0 / 4082c00000000000 |
| 20 | `$.buildingCells.$map[13][1].id` | 8 / 4020000000000000 | 7.0 / 401c000000000000 |
| 21 | `$.buildingCells.$map[13][1].x` | 93 / 4057400000000000 | 81.0 / 4054400000000000 |
| 22 | `$.buildingCells.$map[14][0]` | 527 / 4080780000000000 | 601.0 / 4082c80000000000 |
| 23 | `$.buildingCells.$map[14][1].id` | 8 / 4020000000000000 | 7.0 / 401c000000000000 |
| 24 | `$.buildingCells.$map[14][1].x` | 93 / 4057400000000000 | 81.0 / 4054400000000000 |
| 25 | `$.buildingCells.$map[21][0]` | 4628 / 40b2140000000000 | 4554.0 / 40b1ca0000000000 |
| 26 | `$.buildingCells.$map[21][1].id` | 9 / 4022000000000000 | 10.0 / 4024000000000000 |
| 27 | `$.buildingCells.$map[21][1].x` | 139 / 4061600000000000 | 151.0 / 4062e00000000000 |
| 28 | `$.buildingCells.$map[22][0]` | 4629 / 40b2150000000000 | 4555.0 / 40b1cb0000000000 |
| 29 | `$.buildingCells.$map[22][1].id` | 9 / 4022000000000000 | 10.0 / 4024000000000000 |
| 30 | `$.buildingCells.$map[22][1].x` | 139 / 4061600000000000 | 151.0 / 4062e00000000000 |
| 31 | `$.buildingCells.$map[23][0]` | 4630 / 40b2160000000000 | 4556.0 / 40b1cc0000000000 |
| 32 | `$.buildingCells.$map[23][1].id` | 9 / 4022000000000000 | 10.0 / 4024000000000000 |
| 33 | `$.buildingCells.$map[23][1].x` | 139 / 4061600000000000 | 151.0 / 4062e00000000000 |
| 34 | `$.buildingCells.$map[24][0]` | 4708 / 40b2640000000000 | 4628.0 / 40b2140000000000 |
| 35 | `$.buildingCells.$map[25][0]` | 4709 / 40b2650000000000 | 4629.0 / 40b2150000000000 |
| 36 | `$.buildingCells.$map[26][0]` | 4710 / 40b2660000000000 | 4630.0 / 40b2160000000000 |
| 37 | `$.buildingCells.$map[27][0]` | 4554 / 40b1ca0000000000 | 4634.0 / 40b21a0000000000 |
| 38 | `$.buildingCells.$map[28][0]` | 4555 / 40b1cb0000000000 | 4635.0 / 40b21b0000000000 |
| 39 | `$.buildingCells.$map[29][0]` | 4556 / 40b1cc0000000000 | 4636.0 / 40b21c0000000000 |
| 40 | `$.buildingCells.$map[30][0]` | 4634 / 40b21a0000000000 | 4708.0 / 40b2640000000000 |
| 41 | `$.buildingCells.$map[30][1].id` | 10 / 4024000000000000 | 9.0 / 4022000000000000 |
| 42 | `$.buildingCells.$map[30][1].x` | 151 / 4062e00000000000 | 139.0 / 4061600000000000 |
| 43 | `$.buildingCells.$map[31][0]` | 4635 / 40b21b0000000000 | 4709.0 / 40b2650000000000 |
| 44 | `$.buildingCells.$map[31][1].id` | 10 / 4024000000000000 | 9.0 / 4022000000000000 |
| 45 | `$.buildingCells.$map[31][1].x` | 151 / 4062e00000000000 | 139.0 / 4061600000000000 |
| 46 | `$.buildingCells.$map[32][0]` | 4636 / 40b21c0000000000 | 4710.0 / 40b2660000000000 |
| 47 | `$.buildingCells.$map[32][1].id` | 10 / 4024000000000000 | 9.0 / 4022000000000000 |
| 48 | `$.buildingCells.$map[32][1].x` | 151 / 4062e00000000000 | 139.0 / 4061600000000000 |

## Group 2: one typed-array observer wrapper difference

49. `$.house`: Source observer emits `{$typed:"Uint8Array", values:[6400 entries]}`; native observer emits a plain6400-element array. All6400 corresponding numeric values have identical bits. Actual Source houseKinds returns Uint8Array (sim.js:2274-2277); native Game.house and authoritative map house are Vec<u8> (game.rs:324/map.rs:727). Source observer r1-startup-observer.mjs:25-33 retains the typed wrapper; native aligned_replay_startup.rs emits numbers(g.house.iter()) without its u8 storage representation. This is a test-observer representation discrepancy, with no observed house-kind value difference. Preserve raw wrappers and record the exact representation choice in DESIGN.

## Handoff and gates

MODE_IMPL owns both groups. Implement a runtime fix only if current Source/native consumer inspection establishes a real semantic startup defect. Each actual runtime group needs its own PR, a focused test and Hermes exact-head ack after green hosted checks. For the two currently observed representation groups, prepare separate focused test-only observer/comparator corrections, retaining all raw data and explicit map membership/type/length/value checks; no blanket sorting or generic default coercion. Run the MODE21 diagnostic after each group and report its remaining differences. All proposed defaults stay pending Axel, with zero row/status changes. No Source scene or actual startup layout/consumed draw may change.
