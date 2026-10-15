# MOV original capture streaming successor, source only

Parent: clean `9e424204ffdd7354889d5c2aa81716696b8746e9`.
The original branch `private/mov-speed-companion-396f` and explicit ref
`held/mov-speed-original-9e424-20261015` retain that checkpoint.
This successor lives on `private/mov-speed-stream-capture-9e424`.
No Source, Node, Cargo, native, formatter, test or discovery command runs here.
S3 is released. A separate capture-semantics review and a new Root grant must
precede execution. No row status or publication changes.

## Observed failure and source retention cause

The two raw attempts remain separately attributed to parent 9e424:

- `/tmp/mov-speed-9e424-S3-20261015/01-original-generate.log`: exit 134,
  V8 heap failure near 2045MiB, scope peak2,310,701,056 bytes.
- `/tmp/mov-speed-9e424-S3-20261015/02-original-generate-heap4096.log`:
  sole authorized 4096MiB retry, exit 134 near 4076MiB, scope peak4,515,262,464
  bytes under 6GiB. No Source assertion or cgroup OOM cause is inferred.

Neither attempt wrote an original packet or failure packet. Callback and
assertion completion counts remain unknown. Successful Source commands:0.
The actual terminal receipt is `REPORT-RECOVERY.md` in that same directory.
These files are not rewritten or used to infer which callback failed.

At parent 9e424, `harness/mov-owned-speed-checks-draft.mjs:55-61` makes a
fresh encoded full witness for every observation: units, selected player data,
motion bits, characters, flags, height, wear, World regions and winner, plus
tick/seed/weather/wind. Lines66-78 append each event to `active.calls`, keep
its before witness, then append its encoded result or error and after witness.
Lines83-85 also retain the entire encoded constructor state. Lines107-108 put
every case and its calls in `packet.cases`, so finishing a case releases none
of that evidence. Retained evidence therefore grows with observed call count
and the full state size at those calls. Lines128-129 additionally materialize
one whole-packet JSON string and gzip buffer at finalization. The logs do not
show that finalization was reached; this additional allocation risk is a source
finding, not the measured crash location.

Both old companion readers also read, gunzip and JSON.parse the whole original
packet merely to obtain its identity and completed callback metadata:
`mov-terrain-companion-fixture.mjs:19-20` and
`mov-world-distance-fixture.mjs:17`. Fixing only the writer would leave that
second whole-capture allocation in the serial plan.

## Exact record representation

The original output becomes gzip-compressed JSONL with explicit format
`mov-original-jsonl-gzip-v1`. Each line is compressed as one complete gzip
member at level 9 and written synchronously before capture proceeds. This
makes compression boundaries deterministic record boundaries and supplies
backpressure without promises or queued records in synchronous Source calls.
The output grows on disk with every observation; it is never truncated,
sampled, field-filtered or deduplicated. A single unusually large state can
still exhaust resources. Memory and disk success have not been measured.

| Record | Exact retained data and reconstruction |
| --- | --- |
| header | Prior packet reference, Node/architecture, scope, global RNG description and Source hashes; explicit format and selected-case count 5. |
| case-start | Prior case name, file, complete helpers/body, combined SHA256 and initial completed=false; zero-based case index. |
| create | Prior complete createGame event: op, game ID, encoded argument array and entire encoded initial state; per-case call index. |
| action-start | Prior op, game ID, encoded input and full before witness; per-case call index allocated in the old calls-array insertion order. |
| action-end | Matching case/call/op/game, return-or-throw envelope and prior encoded result or genuine name/message/stack error plus full after witness. |
| case-end | Actual observed call count and completed flag. A callback assertion failure emits completed=false, never a completed claim. |
| footer | Actual started case count, original callback failure and Source-readback error fields if present. |

Every record also has a consecutive stream sequence number. Joining each
start/end by case and call ID reconstructs the former event fields and call
order, including nesting. No prior state field is inferred from another frame.
`witness`, `bits` and `encode` are byte-identical source to parent 9e424.
The existing codec retains typed-array kinds/runs, Map/Set order, signed zero,
nonfinite wrappers and present undefined object fields. A bare undefined
result was omitted by JSON.stringify in the old event and is still omitted;
the new explicit return envelope distinguishes it from a thrown error.
Genuine error stacks are written verbatim from the new capture invocation;
wrapper paths/line numbers can differ from the old recorder, with no
normalization or historical stack substitution.

The synchronous writer retains only the current encoded record, its JSON
string and compressed member. Before records are fully written before the
underlying call; after records follow that call. No calls array or history of
witnesses is kept. The live Source game, original callback locals and normal
Source execution remain untouched. The capture has no new RNG operation.
This is an asymptotic retention correction, not an executed peak-memory claim.

## Partial evidence, check equality and readers

Capture opens `<output>.partial.gz` exclusively. A stale partial file refuses
a new capture instead of being overwritten. Completed members already written
survive a fatal process exit; the last write may be incomplete. There is no
claim that a fatal exit writes its error/footer. An incomplete prefix cannot
be used as a completed original capture. A caught callback error writes its
raw failure footer and retains the partial evidence using the prior output
convention: output on generation, `<output>.failed.gz` on check. Successful
generation renames the partial to output. Successful check compares both
compressed files in bounded 65536-byte positional reads; a mismatch retains
`<output>.different.gz` and fails. Equal check output removes only its own
new temporary file. No old log or packet is cleaned.

`readOriginalCapture` incrementally decompresses all gzip members and parses
every JSONL record with an incremental UTF-8 decoder. It checks sequence,
case/call ordering, matched action before/after records, case helper/body
hashes, five completed callbacks and a final failure-free footer. It retains
only header/case metadata and currently open action IDs, not witnesses or
completed actions. It hashes all compressed bytes as they are read. Both
companions use that digest for their existing originalCapture.sha256 field
and retain their existing completed-callback requirements. A corrupt stream,
unclosed action, missing footer, failed callback or old monolithic JSON
packet refuses before companion construction.

This is an explicit original-evidence format change, not old/new compressed
byte equality. `--check` requires exact bytes between two new-format runs.
The original default output name below ends in `.jsonl.gz`; both companion
CLI argument counts stay unchanged. Their output schemas, full scene logic,
measurements, assertions and RNG setup remain unchanged; the original digest
necessarily names new-format bytes. Native readers still consume the same
terrain/World companion JSON schemas and require no code change.

Static preservation must be reviewed against parent 9e424: extraction/pin
checks, complete callback/helper text, all 41 static Source assertion sites,
all actual raw call expressions and the constant 0.5 RNG scope/restoration.
Static readback in this source-only turn confirmed byte-identical extraction,
pin checks, witness/bit code, raw-call adapters and callback invocation block;
both companions' entire trace/finalization bodies are identical to parent 9e424.
The unchanged pinned callbacks have 9/11/2/17/2 static assertion sites, total 41.
Only the eight paths listed below change. All other tracked Git entries,
including every Rust source, fixture, parity table and reference gitlink,
remain unchanged. These are textual comparisons, not executed capture proof.

The five authored native behavior functions, both registration guards, all
fixture bytes and every parity row retain their objects. No runtime/provider,
terrain/guard/ABI policy, companion setup or predicate is changed.

Exact changed paths:

- `harness/mov-original-capture-stream.mjs` (new)
- `harness/mov-owned-speed-checks-draft.mjs`
- `harness/mov-terrain-companion-fixture.mjs`
- `harness/mov-world-distance-fixture.mjs`
- `docs/checkpoints/mov-original-stream-capture-design.md` (new)
- `docs/checkpoints/mov-speed-consumer-readiness.md`
- `CHANGELOG.md` (direct edit explicitly requested for this checkpoint)
- `DESIGN.md`

Locally read issue5 through Axel readback159, comment6195398155. The R1
mapping and #259/#260 delivery queue remain separate. This capture-only
successor addresses Root's retention request without a contract, outcome or
publication decision. No public acknowledgment is sent.

## Mandatory-field repair after review of 5669

Parent `5669c6b5d1e18668e1e9066ad61f82aaf90bbcc8` is preserved at
`held/mov-speed-stream-5669-20261015`. Full review read from
`/tmp/review-mov5669-stream-capture/REVIEW.md`: REVISE for P2, missing
mandatory input validation. Its preservation findings otherwise accept the
Source bodies, 41 assertions, witness, codec and RNG handling. The review's
remove-input mutation is source-derived. Neither reviewer nor this repair
executed that mutation, and no passing mutation-test result is claimed.

The repair changes only this design, the streaming reader, direct CHANGELOG
and the owned DESIGN note. Writer/finishCapture, all three recorders, codec,
callbacks, helpers, inputs, raw before/after production, companions, Rust,
registrations, fixtures, rows and reference remain byte-identical to 5669.
There is no format-version change or new capture normalization. Existing
valid v1 data remains the intended input, with stronger structural rejection.

Validation follows actual producer shapes: state-codec.mjs:9-23;
sim.js createGame:709-824, setupWorld:1167-1171, spawnUnit:1776-1807 and
setupStructures:2465-2466; world-layers.js:53-66; and the recorder's own
witness and action adapters. The temporary fixture maker in test-fixtures.js
is inserted/removed within the wrapped fixtureCommand call, so its abbreviated
actor is not a before/after witness. The reader never fabricates that actor
or changes the original fixture helper.

Static lexical readback confirmed the reader's complete 98-key actor field
set equals the pinned spawnUnit declaration, and all 46 createGame literal
keys are mandatory alongside subsequent terrain/setup fields. The writer,
comparison/finalization code, gzip/UTF-8 pipeline and digest return are
byte-identical to 5669. Exactly four owned paths change. No JavaScript was
imported, parsed by Node or executed to obtain these source comparisons.

Every record is recursively checked as encoded JSON before discarded evidence
can be admitted: exact codec wrapper structure, ordered map pairs/sets, typed
arrays or value/count runs, positive run counts and encoded numeric forms.
Runs are counted without expansion. Each fixed constructor/actor field from
the pinned declarations must be an own field. All supplied conditional fields
and nested values are validated too; no input or state field is stripped or
replaced. Witnesses require every named top-level field, complete selected
player rows, declared actor fields, motion-bit row identity/shape, matching
terrain lengths, nullable height/weather and explicit non-World undefined
regions. Initial constructor arrays require map/name argument shapes, the
full fixed game envelope, typed terrain, players, maps/sets, initialTerrain,
and the conditional World envelope. Schema checks read values only.

| Source-derived counterexample | Rejection in the repair |
| --- | --- |
| Delete create event.input, or replace its argument array with null/object | own(op/game/input/initial), array and constructor-argument shape checks fail. |
| Delete action-start event.input | own(op/game/input/before) fails before the record can be discarded. |
| Keep step input null | Legal. Input is present and the step adapter's exact null shape passes; no undefined default is inserted. |
| Missing/malformed operation or game ID, reuse a constructor ID, or reference an uncreated game | Required own fields, integer identities, exact createGame operation, action operation switch and case-local consecutive created-game map reject. |
| Missing/mismatched/repeated call ID or end operation/game | Consecutive start IDs and the open-call map require a matching begun pair, consumed once in the same case. |
| Delete a required initial/before/after witness field, nested selected player field or declared actor field | Own-field schemas reject rather than supplying an empty/default value. |
| Replace a terrain codec tag, remove its payload, corrupt RLE pairs/counts or mismatch cell lengths | Recursive codec, typed container and length checks reject without expanding the evidence. |
| Remove or mistype thrown failure.name/message/stack | errorShape requires all three own string fields; throw also forbids result. |
| Omit success result because Source returned undefined | Still legal only in the existing return envelope. Required input/after are independently checked; missing input cannot use this exception. |
| Remove footer, leave an open action, or mark a case failed | Existing completion/failure checks still reject. |

The reader retains at most current-record validation temporaries, five case
metadata objects, unresolved call identities and case-local created-game
metadata (cell count, player count and World flag). It never stores witnesses
in those maps. The writer, fatal partial-file behavior, short-write loop,
compression and exact check comparison are unchanged. A single record can
still exhaust memory; neither bounded-run resource success nor I/O-error
recovery is proved here. Both old exit134 logs remain untouched.

MOV11/49 have transferred to ALIGNED_REPLAY by Root's 15:51 instruction.
This capture repair edits no parity file or targeted aligned scenario. Other
owned and held scopes stay intact. Issue5 was read through Axel readback162,
source comment6196151342; no public acknowledgment or
contract change is made. Future commands/counts below remain preparation,
not a grant. A fresh independent reader review must precede Source execution.

## Commands prepared for a future grant only

Read back the actual successor SHA and clean state before any execution.
The earlier verified executable is `/usr/bin/node` (v24.21.0, x64), not the
absent `/usr/bin/node24.21.0`. The 4096MiB flag below requests no increase over
the last authorized cap. All six commands require a fresh explicit grant,
its exclusive slot flock and scoped 6GiB/swap0/CPU8-11/400% limits. Stop at the
first failure; no retry is authorized by this source-only plan.

```sh
WW2_REFERENCE=/home/judiazm/Projects/ww2-rts-native/reference/ww2-rts /usr/bin/node --max-old-space-size=4096 harness/mov-owned-speed-checks-draft.mjs /tmp/mov-original-speed-stream.jsonl.gz
WW2_REFERENCE=/home/judiazm/Projects/ww2-rts-native/reference/ww2-rts /usr/bin/node --max-old-space-size=4096 harness/mov-owned-speed-checks-draft.mjs /tmp/mov-original-speed-stream.jsonl.gz --check
WW2_REFERENCE=/home/judiazm/Projects/ww2-rts-native/reference/ww2-rts /usr/bin/node --max-old-space-size=4096 harness/mov-terrain-companion-fixture.mjs /tmp/mov-original-speed-stream.jsonl.gz /tmp/mov-terrain-companion-stream.json.gz
WW2_REFERENCE=/home/judiazm/Projects/ww2-rts-native/reference/ww2-rts /usr/bin/node --max-old-space-size=4096 harness/mov-terrain-companion-fixture.mjs /tmp/mov-original-speed-stream.jsonl.gz /tmp/mov-terrain-companion-stream.json.gz --check
WW2_REFERENCE=/home/judiazm/Projects/ww2-rts-native/reference/ww2-rts /usr/bin/node --max-old-space-size=4096 harness/mov-world-distance-fixture.mjs /tmp/mov-original-speed-stream.jsonl.gz /tmp/mov-world-distance-stream.json.gz
WW2_REFERENCE=/home/judiazm/Projects/ww2-rts-native/reference/ww2-rts /usr/bin/node --max-old-space-size=4096 harness/mov-world-distance-fixture.mjs /tmp/mov-original-speed-stream.jsonl.gz /tmp/mov-world-distance-stream.json.gz --check
```

Expected, not observed: six serial Source exits 0, five complete original
callbacks with 41 static assertion sites, twelve terrain scenes and two World
branches. Only then can authentic companion fixtures be frozen for a new
Cargo grant. Native selection remains exactly 7 instances:

- `mov_terrain_companion::original_roads_and_mud_distances_include_partial_road`
- `mov_terrain_companion::original_half_churn_and_wet_ground_distances`
- `mov_terrain_companion::original_uphill_and_downhill_distances`
- `mov_world_distance::original_public_world_300_plus_40_plus_40_distance`
- `mov_world_distance::physical_world_march_calm_boundary_200_and_201`
- core `every_test_file_is_compiled`
- simtest `every_test_file_is_compiled`

From rust/, only within that future guard/slot scope, each Cargo invocation
must have an immediate `df -h /` and the 20GiB floor:

```sh
df -h /
WW2_CARGO_SLOT="${ROOT_SLOT:?fresh Root grant required}" CARGO_BUILD_JOBS=4 python3 /home/judiazm/.t3/handoffs/ww2-native-cargo-guard.py +1.99.0 fmt --all -- --check
df -h /
WW2_CARGO_SLOT="${ROOT_SLOT:?fresh Root grant required}" CARGO_BUILD_JOBS=4 python3 /home/judiazm/.t3/handoffs/ww2-native-cargo-guard.py +1.99.0 nextest run --locked --cargo-profile ci-test --build-jobs 4 --test-threads 1 -p ww2-core -p ww2-simtest --test it -E 'test(mov_terrain_companion::) | test(mov_world_distance::) | test(=every_test_file_is_compiled)'
df -h /
WW2_CARGO_SLOT="${ROOT_SLOT:?fresh Root grant required}" CARGO_BUILD_JOBS=4 python3 /home/judiazm/.t3/handoffs/ww2-native-cargo-guard.py +1.99.0 clippy --locked --workspace --all-targets --profile ci-test -- -D warnings
```

Clippy remains conditional on the native selection passing. No completed gate
is repeated here. Current successor execution count: 0. MOV03 outcome agreement,
MOV04 fuel/provider gap, MOV09 pause, MOV72 purchase/cargo boundary and all
other held row limits remain unchanged. Companion success alone cannot promote
MOV05/06/07/08. This design awaits Root and independent capture review.
