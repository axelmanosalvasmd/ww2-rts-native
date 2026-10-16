# MODE-RUNTIME-B3

Branch: `runtime/mode-b3-20261016`, from main `92688084369ac20ef1cf77b9e1a156a2f1f9fbcb`.
Reference: `e4e579a` (read only).

Scope: MODE-45, MODE-52 (core), MODE-54 (core), MODE-91. No parity status edits.

The Horde phase already owns its durable state. This change projects existing state into a typed public mode view, omitting `next_profile` while a wave is active. The core result adapter retains the wave reached and each defender's defeat. It does not persist a record or deliver a server result.

Explicit Horde point economy uses the shared capture/income phase with defender base income and the internal enemy's income exclusion. Death preflight and payment exclude that enemy from bounties; a defender killing a Kaiju receives the existing 20 percent of its 1,500 MP cost, or 300 MP.

Kaiju breath uses canonical damage and terrain owners. A shared damage memory prevents overlapping circles from repeatedly damaging one trench cell or one building terrain pool. The direct unit hit and building terrain debit follow the prototype. Public breath and surfacing effects use the existing effect shapes and remain caller-owned batches.

Refund operations publish existing wallet, production, terrain and story state together. Construction cancellation gives the original 75 percent MP refund. Waiting production cancellation reverses the original MP charge while preserving the active job's progress. Recovery tests exercise the actual charged amounts, free medic healing and failed-charge paths.

## Runtime boundaries

No new durable field, flag, command or link is added to Match or Unit. The new projection/result types are derived views; the breath variants are detached effects. Guards were inspected in `match_tick.rs`, `recipient_snapshot.rs`, `recipient_snapshot/shots.rs`, `match_world_elimination.rs` and `match_commands.rs`.

Full Horde ticks, full recipient snapshots and server hosting remain guarded. Horde-only beam events cannot be produced in ordinary matches. Ordinary regression tests advance through multiple ticks, collect recipient snapshots and continue through match finish. Construction refund tests also advance ordinary ticks and collect both recipients after removing the site. These checks do not claim a complete playable Horde match.

No FFI, C header, wire schema, contract, privacy or server admission change. MODE-83 remains on PR #313. MODE-92/93 and all other excluded rows are untouched.

## Verification

Pending focused nextest, formatting and strict workspace clippy on the Vostro under a shared slot with MemoryMax=6G and MemorySwapMax=0. No Source recorder or provenance bundle is used for behavior rows.

Ground combat and death validation use the existing aircraft metadata validator in observation mode, including the planes spawned on a real boss wave. Aircraft actuation and the full Horde tick retain their existing guards. A real wave-10 regression exercises breath, Kaiju retirement and the subsequent public mode components beside those aircraft.
