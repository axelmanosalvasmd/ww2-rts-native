# MOV-67 caller stack: merge record

## Sandbox merge

Parents: #404 `b8b8b21d9557d8bee7d36e38676a6e822f31c921` and #393 `a7fa4e1de6260e3407c2ab78ca9231c6647111dd`.

Every conflicted path and resolution:

- `match_firing.rs`, four hunks: retain #404's `FlightContact` result and borrowed visibility predicate, including its current-pose selector. Both branches query current poses and visible actors for recipient contact clipping.
- `recipient_snapshot.rs`, one hunk: retain logistics and #404's common retained-flight capture. Sandbox uses that same field and recipient visibility path.
- `recipient_snapshot/composition.rs`, three hunks: retain the common flights field and ordinary ground-flight admission from #404; retain sandbox firing/grenade/salvo validation from #393. Undrained producer, support and aviation event guards remain.
- `recipient_snapshot/shots.rs`, four hunks: retain #404's common-flight documentation, #393's sandbox-only Breath/BreathShell validation and SmallArms refusal, and ordinary Throw support from #404. The incoming sandbox-only throw refusal would remove ordinary grenade support. Outside the conflict hunks, adapt the sandbox test projector to call the common flight capture, preserving every incoming exact-shape test and avoiding two production projectors.
- `server/src/lobby.rs`, one hunk: keep logistics enablement from #404 and sandbox propagation from #393.
- `server/src/main.rs`, one hunk: describe optional sandbox and enabled logistics together.

All unlisted paths merged automatically. No tests or assertions were removed. No World admission guard was opened.
