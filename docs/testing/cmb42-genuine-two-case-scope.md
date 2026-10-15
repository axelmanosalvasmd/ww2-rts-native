# CMB42 two-case genuine-input proof scope

Base: `f7c51b1a0ebf850c5c22b560a595e4095ce39fdc` (PR271 merge).
Checkout: `/home/judiazm/.t3/worktrees/ww2-rts-native/t3-3fa592e8`.
Branch: `audit/combat-seven-mainf7c-3fa592e8`.
Status: authored source only. Both Source cases and every recovered/new native
consumer are UNRUN on this head. There is no captured packet or runtime grant.

## Provenance correction

The old candidate is retained at `06bb5d9006f3a13179624595bdcfbc1415217bbd`,
including `docs/testing/audit-combat-seven-protected-consumers.md`, and its
original packet is retained at `dff2b3530b322a6abc0b695ff3cc5fff74780d63`.
The old helper `harness/cmb42-ordinary-scenes.mjs` explicitly assigns
`g.players[owner].mp=5000` and `u.cooldown=1e9`. Its native packet reader and
two simtest witnesses make corresponding assignments. These are declared
component inputs. Their purchases are real, but those wallets and initial
cooldowns are not produced by ordinary construction or income. Any blanket
no-wallet-writes description would be incorrect.

Pinned Source `shared/sim.js:29,723,802` supplies starting MP150 and the army
multiplier; `:1778` supplies actual birth cooldown0. The original named
`test.js:2592-2623` checks explicitly assign MP5000, use a local cooldown1e9
helper, and directly place actors. They check sniper rifle12/MG30 and AT
rifle7/tank28, automatic target selection and rifle survival. The successor
keeps those literal distances and predicates while producing actor positions
through Move. It permits actual firing and ordinary damage. It does not claim
the original component setup was an ordinary match history.

## Exact Source selection

`harness/match-tick-fixture.mjs --cmb42-genuine-only`
loads only `harness/cmb42-genuine-scenes.mjs` and returns before both the old
ordinary candidate and historical corpus. Exactly these two cases are selected:

- `genuine-sniper-12-30-distance-and-recipient`
- `genuine-at-7-28-researched-built-producers`

A future invocation also needs exactly one
`--cmb42-output=/absolute/existing/private/directory`, x64 Node v24.21.0,
and a clean read-only checkout at reference
`e4e579a838aabae3b1d9dadbc17a0a87147bc52d`.
The assigned checkout's submodule is uninitialized. Read-only inspection here
used Git objects from `/home/judiazm/ww2-rts/.git` at the exact pin. Root must
select an existing clean pin via `WW2_REFERENCE` or separately arrange setup
before granting capture. No reference checkout was changed here.

Both retained constructor armies first receive real stance/autocast commands.
Twenty actual quiet ticks establish positive income. Later research, Build and
Buy wait for actual wallet funds; each ordinary tick checks the actual income
addition. Tier jobs, paid motor pools, real builder completion, purchase debit,
producer identity and births remain observed. The helper contains no wallet,
pose, HP, cooldown, ammo, visibility, target or RNG-state assignment. Source
commands and step produce those values. Expected geometry appears only in
Move commands and subsequent assertions.

Sniper selection checks20 ticks at rifle12/MG30, then20 ticks at rifle12/MG40,
then the hidden-MG60 control. AT selection checks20 ticks at rifle7/tank28,
with genuine setup time and front-facing tank movement. Every selection tick
requires actual visibility, canShoot, no explicit Attack, the correct target,
surviving candidates and the actual owner recipient target. Hidden MG must be
absent from that recipient. No assertion is removed to accommodate firing.

Global SplitMix64 starts once before construction. Observation wrappers retain
actual accuracy, dispersion magnitude/sign and shooter/target/round consumers,
with every returned value, index and seed transition. Unexpected consumers stay
explicit. Environment RNG executes its original body and returns its original
value. All raw observations and per-boundary global/environment prefixes are
retained. Repeated large observation arrays use lossless `$observation` pool
references; inputs remain separate. This is private packet format
`cmb42-genuine-tagged-v2`, with no public schema change.

Only both completed cases produce `scenes.json`, created exclusively. The first
scene failure writes `first-failure.json` with partial inputs, observations,
random receipts and completed earlier cases, then throws. No retry, input
repair or automatic check run follows. A later `--check` is another Source run
and requires its own exact grant.

## Exact native selection

`aligned_replay::startup_state::cmb42::genuine::genuine_cmb42_packet_matches_actual_native_boundaries`

This ignored core unit test lives in
`rust/crates/core/src/aligned_replay_cmb42_genuine.rs`. It requires explicit
`WW2_CMB42_PACKET` and a new `WW2_CMB42_NATIVE_RECEIPT` path. The reader accepts
constructor, immediate/queued command and step inputs only. It rejects funding,
cooldown and other state setters. It binds purchases from actual native births
and producer metadata. Expected observation pool entries expand only for strict
comparison and never initialize a Match.

The native reader checks the two full choice windows and hidden-recipient
control independently of expected snapshot state. Every recorded boundary
compares raw state, both actual recipients, actual canShoot probes and all
consumed random prefixes. Default mappings are empty. The receipt retains raw
first differences, explicit unmapped/absence representations, complete native
observer output and partial failure context. Native output pools preserve raw
values losslessly. The first mismatch or runner error stops the test.

The optional mode has a private successor marker and non-default captured tape
capacity. The default Replay constructor, 16,384-value contract and every
draw/receipt/shot/layout/startup body remain unchanged. Default environment
substitution remains unchanged. The optional mode observes and returns actual
environment values. Old v1 recorder and native scene bodies remain preserved;
only its child registration and private mode discriminator gain this successor.

## Seven-row integration and held boundaries

The new branch carries only the three own commits from f7b main to06bb onto
fetched PR271 main. Incoming logistics work, typed entries, all parity cells,
Acceptance and other owners' rows remain unchanged. CMB38's real pillbox
front/rear contacts, CMB63's accuracy/damage/suppression component consumers and
the full funded-depletion B09 body are retained, all UNRUN here.

B09 still declares MP10000 component funding and fixed preconstructor0/0.99
tapes. It has no ammo assignment or running-stream reset. Its first required
enabled-logistics tick is still guarded. This is retained authored preparation,
not a funded native PASS. CMB62 FireAt, CMB67 ground/automatic/area aircraft
weapons, CMB68 wreck ejection/drowning and CMB66 enabled-logistics guards are
unchanged. No unavailable actuator is replaced with a helper outcome.

Local coordination acknowledges issue5 owner decision6201127263, runtime
readback6201618030 and queue readbacks6202564980/6202580526. PR195 still agrees
only to table content/schema. ABI2 remains bounded and broader ABI/wire/privacy
drafts remain unadopted. No public message or contract change is made.

Root retains the queue268,269,270, exact-head/slot/selection grants, independent
review, hosted9 and Hermes runtime readback. PR269 stays at
`2f130c3238f5b0b5db01d26b76af2d60e3475955`; its expected eight-commit rebase
packet is untouched until an explicit post268 main is supplied. The original
`/tmp/audit-combat-2058-handoff-3fa592e8/HANDOFF.md` remains untouched.

This task executes no Source/Node, Cargo/native, formatter, discovery, CI or
release job, and holds no slot or lock. It creates no worktree and makes no
public change. Text inspection and preservation checks are recorded separately
under `/tmp/audit-combat-2152-3fa592e8/`. No row completion is claimed.

## Consumer and queue binding successor (user22:22, unrun)

The next child labels the actual constructor seed and both constructor/tick
wind consumers at their exact pinned expressions. Every global/environment
receipt retains its real value, continuous index and seed transition. Any
unexpected consumer remains in the failure artifact and prevents success.
The original environment generator still executes once and returns its value.

Every Source selection tick now checks the literal distances as well as
visibility, canShoot, survival, target choice and owner recipient. The native
reader independently checks queued command contents, owner, sequence, boundary
clock and actual dispatch identity before comparing its saved snapshot. Each
queued snapshot must bind once, including its global/environment prefix. Signed
Source seeds must match their retained u32 bits; environment consumer labels
must equal the native labels. The hidden control checks the actual owner
shooter target0 as well as hidden-MG omission.

No Source/Node or native execution accompanies this child. The private grant
request and single-job wrapper are in `/tmp/audit-combat-2222-3fa592e8/`.
Generation, check and native qualification are three separately granted jobs.
The wrapper requires a fresh Root-authored exact-head grant and matching live
ledger slot before executing. It never grants a slot or writes Root's ledger.
All runtime guards, default Replay bodies and original component history remain
unchanged. This is literal consumer work; it supplies no completed CMB42 proof.
