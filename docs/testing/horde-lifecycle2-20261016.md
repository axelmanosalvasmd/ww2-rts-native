# HORDE-LIFECYCLE-2

Base: PR #399 at `4f0954eef8b44c52cfb22bbe508e06f5bf00b61d`, stacking
on #357. Prototype: `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`.

## Each row and its literal intent test

- MODE-41: `mode41_horde_maps_select_preserve_ignore_and_reread_start_restart_atomically`.
  Covers all 17 eligible maps with normal ticks and every-seat snapshots,
  preservation of an eligible selection, fallback to the first map, ignored
  invalid selections, and Endless normalization. A real private map file loses
  its `defend` list before Start and Restart. Refusal retains the previous room,
  state, epoch and accounts; restoring the file permits a new match. A deleted
  file emits the exact prototype denial without replacing the running match.
- MODE-44: `mode44_host_nextwave_ignores_guest_pause_and_active_wave_then_ticks_and_reconnects`.
  Real Hub message ingress proves host-only unpaused break control, guest and
  active-wave no-ops, normal next-tick spawning, healthy delivery, and a
  reconnect start that retains the canonical internal player.
- MODE-56: `net73_hosted_horde_and_defender_keep_delivered_views_on_adaptive_room_beats`.
  The hosted internal seat uses the scripted brain with a real emitted and
  accepted command stream, no Buy/Retreat, bunker attack movement and at most
  four ground orders per observation. Twelve initially idle Horde squads make
  the pending queue observable. The unchanged core queue/policy tests cover
  live wave invalidation and retained movement intent. The hosted lifetime
  test below protects terminal state and result serialization.
- NET-73: `net73_hosted_horde_and_defender_keep_delivered_views_on_adaptive_room_beats`;
  `net73_hidden_enemy_changes_do_not_change_delivered_defender_brain_or_orders`.
  Four hundred ordinary Horde ticks exercise the real meter's 2, 3 and 4 tick
  snapshot cadences, persistent human and Horde brains, stale-view retention
  between actual sends, requested refreshes and actual command acceptance.
  A paired 160-tick test changes unseen canonical enemy facts and proves equal
  delivered defender packets, complete human brain state and queued decisions.

## Lifecycle and adapter coverage

`mode56_hosted_horde_runs_boss_combat_to_result_and_complete_hold_with_all_seat_frames`
starts with an injured bunker, wave 9 and a close gate as declared scene inputs.
Normal wave-10 spawning and actual boss combat destroy the bunker. Every
canonical seat is collected throughout the running and terminal ticks. The
server then completes all 120 ending callbacks, sends the 60 advancing hold
frames and publishes the existing result fields with Horde names/teams/story
and territory timeline entries. No winner is assigned by the test.

`paused_scheduled_event_clear_resets_ai_alert_cursor_before_resume` consumes a
real section-hurt event, clears its batch during a host pause, then resumes with
an empty shorter batch. Both a seated send and an empty recipient list reset
the event generation. Current/catch-up frames keep it. The old generation
ownership would refuse the resumed AI alert frame as `InvalidBatch`.

The delivered adapters have focused tests for exact typed Horde facts, owner
plan tuples, queue invalidation, paid-Dig busy state, private fields, remembered
terrain, sighting expiry and transaction rollback. A real sixteen-tick Horde
recipient test proves hidden HP, suppression and cooldown mutations never enter
the policy packet or consume the match random stream.

`hosted_horde_ai_grenade_survives_every_seat_delivery_until_natural_detonation`
uses an actual scripted Horde policy decision and ordinary recorded admission.
It follows the retained grenade flight, throw delivery, natural explosive
contact and ticket cleanup, collecting every canonical seat throughout and
continuing healthy ticks afterward. Exact adapter tests protect flight shape,
ground fog, shooter privacy and rejection before delivery mutation.

`canonical_horde_global_lists_survive_ticks_and_every_seat_delivery` validates
the room's global snapshot-list cache through sixteen ordinary Horde ticks and
every-seat delivery. Mismatched mode tags, missing/corrupt Horde state and
invalid global wreck identity remain refused.

## Limits

No World admission, FFI/header, parity status or reference changes. Existing
snapshot preflights and the aircraft weapon restriction remain. MODE-55 record
persistence, `hordeBest` and `result.horde` are separate work. Spectator and save
admission keep their existing guards. Hermes' exact-head runtime acknowledgement,
hosted gates and merge belong to the orchestrator. No Source recorder ran.

## Validation

The PR records the producing commit, Rust tree, exact focused selection and
actual formatting/lint results. All Cargo runs use Rust 1.99 on the Vostro,
a shared S1/S2/S3 flock with its matching target directory, four build jobs,
MemoryMax=6G and MemorySwapMax=0. This source mapping alone claims no execution.
