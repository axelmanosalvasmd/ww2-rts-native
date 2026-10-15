# Classic base constructor and runner proposal

Status: private implementation proposal, not an agreed transport contract or a runtime admission change. IMPL_BASES owns BASE-20/21/22/23/24/26/29/30/48/56. Frozen main is `379ba4e06c794708623c005ab6052bebd03d8809`; Source is `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`. The existing `cde9dde3` runtime review remains separate from its recipient/test successor.

Root has authorized private internal production, construction, training, rally and cancellation implementation. No further routine core permission is requested. Ack [Hermes 151](https://github.com/axelmanosalvasmd/ww2-rts-native/issues/5#issuecomment-6192532713): the named Classic Depot, other-mode construction and concrete checks remain incomplete. That acknowledgment does not claim owner readback of a future implementation. Root obtains exact-head Hermes readback before any runtime merge.

## Declared future Rust scope

The next constructor provider would add private `StartupProfile::Classic`, crate-private `StartPlan::prepare_classic`, a crate-private `Match::new_classic`, and private `classic_runtime.rs` state. A stored optional Classic state on `Match` would change the Rust struct's source shape. Declare that exact field and its visibility before editing it, update every actual constructor, and review the resulting Rust source compatibility. The six ABI 2 calls and header remain outside this proposal.

Keep the public `StartPlan::prepare` and `Match::new` admission unchanged while constructing and checking the internal provider. Exposing Classic through either public method, adding a public constructor, admitting it in the server StartPlan, or expanding recorded-command or runner admission must be declared as a separate concrete scope before editing those paths. No WebSocket command, room lifecycle, recipient envelope, spectator view, privacy rule, FFI, reference pin or terrain policy is proposed here.

## Constructor implementation

`StartPlan::prepare_mode` currently selects Conquest, explicit World or Assault profiles. `Match::skirmish_shell` spawns the mobile force before `complete_skirmish_start` places skirmish bases. Reusing that result and changing its mode cannot reproduce Classic's natural identity order or terrain history.

Split allocation of the empty canonical match from mode-specific publication. Preserve the exact Conquest/World/Assault order through their existing callers. Classic must execute this Source order from `shared/sim.js::setupClassic`:

1. Select the authored map, player layout and weather with their recorded deterministic inputs. Create canonical players and terrain without importing an expected frame.
2. Create Classic state with catalog time, distinct-team count and no Sudden Death. Set each player's initial Classic MP and zero Munitions/Fuel, then apply the constructor's later army-income scaling after force setup.
3. Clear each HQ's actual 3-by-3 footprint and publish each HQ in player order through the real structure and metadata consumers. HQ identities precede mobile units.
4. Build the ordered resource requests: two home MP flank nodes per HQ, then contested Fuel requests from each unique nearest-enemy pair. Retain Source's nearest-enemy ordering, pair order, site search order and strict less-than score replacement. Fuel placement compares actual walking distances from HQ doorsteps. Do not infer extra point nodes from the stale Source comment.
5. Publish the actual Classic start force, Engineer then Rifle per player, using the existing placer and canonical controls appropriate to construction-mode ability costs. Initialize combat, firing, recovery, production, rallies, grid and routing metadata from those actual units.

A private `ClassicRuntime` should own the remaining time, initial team count, Sudden Death flag and canonical ordered resource nodes, including each node's live depot identity. Do not create an independent queue authority. Paid jobs remain in `ProductionState`; sites remain canonical building units and footprints; activity remains `UnitControl::build_target` plus its validated projection.

## Commands and phase composition

Connect complete providers before broadening their admission guards. The existing Conquest construction validator and production context validator deliberately refuse Classic. Their refusal is evidence of missing composition, not a reason to relabel a fixture or skip admission.

The declared implementation must supply Classic contexts to existing paid queue, research, placement, crew construction, Assist, trained-unit publication and rally providers. A Depot additionally requires the chosen node, finite site admission, paid wallet/story preflight, a bound canonical footprint and node identity, followed by atomic publication. Site cancellation must restore that node binding and ordinary footprint cleanup with the Source refund. Unsupported Shipyard, supply-cache and Wall/Gate providers remain explicit until their own consumers exist.

Wire the actual command boundary and tick phases in Source order. At the building unit turn, train only a living finished queued producer while Sudden Death is false. A trained unit is placed and published at its real producer and obtains the building rally, with no personal Classic fallback. Construction and repair use captured crews and actual movement, including the unfinished Pillbox exclusion from firing and prerequisites. Do not add guard exceptions solely to admit a test scene.

Implement Classic income and its late end phase from `shared/sim.js:5737-5767`. Only an undecided match decrements remaining time. Crossing zero sets Sudden Death once, empties every production queue and paid-job row without refund, and resets progress. Every live producer then loses catalog max HP times the Classic decay rate per second. Ordinary wreck/death, elimination and finish consumers must run in their actual order. Build, technology, Buy and cancellation must receive the canonical Sudden Death context. World retains its separate income/end provider and never acquires Classic state.

Before the runner admits Classic, its read-only preflight must validate this exact phase composition and future births. Preserve unrelated actor, logistics, scenario, aircraft and transport exclusions until represented. A provider-only fixture can qualify a complete component predicate under PARITY without adding a new whole-match gate. It cannot claim that an absent constructor or runner is connected.

## Focused proof plan

Keep original Source bodies and expected native state separate. After a future exact Source slot, record actual constructors and commands, or the unmodified named Source component, using authored inputs only. The native side constructs its own state and consumes original commands. Failures retain their producing head and original logs; no RNG resets, HP assignment, teleported actors, imported expected snapshots or synthetic Classic deadline.

- BASE-20/21: original Classic production and training-separation outcomes, dynamic maker input forms, five paid slots and queued population, natural rifle/MG births, placement and building rally. Preserve the literal `train` boundary cases already covered by complete component tests.
- BASE-22: distinct identical-job identities, active/waiting promotion, waiting index 2 cancellation, actual stored MP/Fuel refund once, unchanged active progress, foreign ownership refusal, repeated readback and duplicate/old-head refusal. The new Conquest/World private recipient checks add original cached/uncached owner rows. They do not establish the selected acceptance check's ally/spectator/reconnect transport behavior or approve a spectator policy. Existing complete component evidence still counts.
- BASE-26/29/30: actual paid Classic Depot binding/cancel, one natural Engineer's construction, ordinary production after completion, unfinished Pillbox no-fire/prerequisite controls, and paid producer section damage followed by real Assist and supported-section restoration. Preserve the original queued-MG intent. Earlier Conquest construction and repair witnesses remain independent evidence, with no mode relabel.
- BASE-23/24/48: retain the named Assault/Horde building/personal/shared-access and Wall/Gate T1/T2 checks as separate missing consumers. Do not infer them from Classic or World.
- BASE-56: run the real Classic clock from canonical startup through its catalog deadline when full-clock proof is granted. Compare the transition, no-refund wallet/jobs, stopped production/construction and producer decay. The complete existing Sudden Death component remains valid evidence; no artificial clock proves deadline integration. World crossing retains its actual World constructor and runner and must still permit its normal construction/training/delivery afterward.

This proposal contains no new runtime implementation, execution result, row promotion or public interface adoption. It specifies the next provider rather than asking for routine implementation permission.


## Implementation follow-up under the 19:21 instruction

The provider now has source-written implementation, with no execution result. Before editing, IMPL_BASES declared two additive public Rust-only component entries: `Match::new_classic(map, options)` and `Match::advance_classic_base_tick()`. This intentionally revises the earlier crate-private entry proposal so actual component consumers can call the provider without a test-only constructor or unused code. The private `ClassicRuntime` field is stored on Match and initialized by every actual constructor. Ordinary public StartPlan/Match admission and external server/FFI remain unchanged.

The base caller handles the actual recorded command boundary, fixed-duration clock, research, production, income and late deadline clear/no-refund/decay. Its name and documentation explicitly limit it to base components. It refuses unfinished construction and imminent producer death because the full construction/retirement composition is still missing; it preserves a real post-mutation fault. It is not the full Classic runner proposed above.

Classic's observed recipient routing was separately declared before editing. Source selects observed routing for every non-World mode. The existing provider now admits exact Classic options/Game context with canonical runtime validation at the Match entry. The new constructor initializes that provider; full recipient snapshots and transport still retain their original admission boundaries. The private owner queue/job fragment includes Classic without inventing a spectator view.

Four new tests and `harness/classic-base-components-fixture.mjs` are source drafts. A separate future exact slot is required for their original Source recording and compiled replay. The first source/native failure must retain its producing head. No Source generation, native test, format/lint, row promotion or PR has occurred at this implementation boundary.


Source inspection correction: `ab8c06ec` incorrectly treated setupClassic's starting MP assignment as final. Pinned createGame later multiplies it by the selected army income at lines801-802. The corrected draft retains that complete order and changes the Large precondition accordingly. This is an actual source-level finding with no runtime execution; keep the producing checkpoint and receipt separate from future test results.
