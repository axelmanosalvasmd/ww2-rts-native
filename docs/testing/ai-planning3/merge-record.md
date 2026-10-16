# AI-PLANNING-3 dependency merges

Starting parent: #407 `0fe23576`, stacking on #391 `b3492aa2`.
The full automatic-tree deltas for conflicted merges are committed beside this file.
They include conflict markers in the removed side so each resolution is reviewable.

- #414 `1c2bc074`, branch `runtime/mode-hosting-20261016`: merge `2f7d9ecd`, automatic tree `beea64e9f8a38cf783f8c9ff89f3705d36e7746a`. No manual hunks.
- #413 `a1be2f74`, branch `runtime/horde-lifecycle2-20261016`: merge `d7e845b4`, automatic conflicted tree `bc01030976a57986c3de9cc838e5943197554a2d`. Seven paths, 12 conflicted hunks. Keep depot claim release and Horde construction validation; union ordinary and Horde list admission; retain Horde wave-zero field omission; union snapshot mode pairs and both validation blocks; use the shared mode-row projection; admit ordinary and Horde results with mode-specific timeline checks; retain both test and host module registrations; combine CLI descriptions; union startup pairs and constructor plans. See `merge413-resolutions.patch`.
- #369 `161bbe38`, branch `runtime/flights-events-20261016` (includes #349 and #337): merge `fad2cf81`, automatic conflicted tree `23ac56e98e88958b5e24f3b2a8da3c9e34c6bac7`. Four conflicted paths, nine conflicted hunks, plus one deliberate compatibility hunk in `projectiles.rs`. Keep the qualified aircraft-purchase admission, both modes/flights modules, Classic nodes, default empty-flight output, ground projectile validation, complete flight/salvo validation, Horde events and breath payloads, aviation payloads, and both invalid-event test inputs. Remove the old projectile validator's blanket aircraft-flight refusal because #369 supplies its complete receiver. See `merge369-resolutions.patch`.

Source review compared both patch files against the automatic-tree deltas and checked module/path registrations against both merge parents. No registrations were removed. Existing replacement tests remain registered, including the aircraft/support delivery successors from #369 and the Horde lifecycle successors from #413.

Post-merge compatibility changes are in the runtime commit: remove the obsolete queued-aircraft attachment refusal from `restrict_to_ordinary_snapshots`, retain its full preflight, and remove one call to the aircraft weapon restriction method deleted by #369 from the Horde projectile lifecycle test. Every assertion in that test remains intact. The AI adapters now enable the receiver capability and derive mode roles from recipient rows.
