# ROOM-AI qualification blocked on live flight delivery

Baseline: native `44eeb45fa8d572bfd5b077d7bad0faa517f0d6c4`, prototype `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`.

The local candidate activates ordinary room AI through delivered recipient snapshots, the shared scheduler and the existing commander dispatcher. It implements departure, host handover, reconnect reset and level selection. It adds no wire fields, ABI calls or World admission. Horde startup remains guarded on this baseline.

The required match-completion witness is blocked. `room_ai_collects_all_requested_seats_before_planning_and_survives_natural_finish` attaches a normal default-map Conquest match, hands both human armies to AI, advances the real match, and collects both seats' recipient snapshots after every tick. It uses seed `3936341`, the standard army and the normal victory target. It stops at tick 309:

```text
tick 309: native snapshot: Unsupported { unit: 0, capability: ShotsAndFlights };
flights=5, salvos=0, grenades=0, firing_events=0, support_events=0
```

`recipient_snapshot/composition.rs::admit_actors` explicitly refuses nonempty `state.firing.flights`. The candidate keeps that guard. It does not suppress combat, weaken the finish witness or import another stream's implementation. Pending PR #369, head `50a66d3f12a608c590d62f8fed314fba216d6427` when inspected, supplies flight delivery. That PR was read as dependency status only.

The latest focused Rust 1.99.0 run used S2, its matching target directory, a 6 GiB scope with zero swap, and `cargo nextest run -j4 -p ww2-server --lib -E 'test(room_ai)'`. Result: 9 passed, 1 failed. The passing checks cover level authorization and equal wallets, reconnect reset, departure, handover authorization and pause ownership, host succession before disconnect cleanup, actual snapshot beats, and hidden-opponent counterfactuals that exercise admitted AI orders. These component passes do not qualify the runtime for integration or promote any row while its ordinary match faults.

An earlier focused core/server run also exercised the detached packet tests and real WebSocket AI startup. It is diagnostic evidence only. Formatting was applied in the slot. Final format checking, strict Clippy, full-match completion and final-head qualification remain outstanding. No PR was opened or pushed.

All seven assigned rows remain held: MODE-56, AI-62, NET-25 (AI part), NET-48, NET-49, NET-73 and NET-80 (room-AI part). MODE-56 and the Horde part of NET-73 additionally need hosted Horde recipient admission. The save/load and host-pause parts of NET-80 remain separate work. Hermes' exact-head runtime ack remains required after qualification.

The current owner confirmation was read: issue #5 comment 6218552386 confirms A to P, including item L's recipient-view-only rule. It keeps World admission closed and the exact-head ack requirement.
