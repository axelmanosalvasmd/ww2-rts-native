# SANDBOX runtime intent mapping

Prototype pin: `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`.
Scope: optional sandbox in the private, two-human Conquest R1 profile, plus the
existing internal Horde component. World and hosted Horde remain guarded.
No FFI/header change is required. Prototype messages and recipient rules are in
[the sandbox contract](../contracts/sandbox.md).

## eachrowliteraltestmapping

- MODE-10: `match_sandbox::tests::sandbox_enemy_uses_first_nonallied_roster_seat_without_payment`,
  `match_sandbox::tests::sandbox_flood_guard_allows_spawn_at_2000_then_blocks_at_2001`,
  `match_sandbox::tests::sandbox_horde_enemy_activates_wave_zero_and_attacks_bunker`.
  These exercise free self/enemy spawning, first opposing roster seat, no-enemy
  refusal, the exact flood boundary, internal Horde bunker attack, wave-zero
  activation, boss projection and omission of competitive sandbox results.
- NET-95: `lobby::sandbox_runtime_tests::sandbox_start_flag_host_authority_restart_and_every_seat_snapshots`,
  `lobby::sandbox_runtime_tests::sandbox_guest_spawn_dispatch_is_free_and_retains_recipient_fog_and_owner_fields`.
  These use actual R1 start and guest ingress, verify the exact start keys and
  flag, host authority, queue identity, recipient fog and owner-only fields,
  restart and victory through all 120 ending callbacks.

Both rows also depend on:

- `lobby::sandbox_runtime_tests::sandbox_every_non_structure_kind_survives_ticks_and_both_recipient_snapshots`:
  all 32 non-structure types, each through six ticks and both seat views.
- `lobby::sandbox_runtime_tests::sandbox_spawn_refusals_and_pause_never_publish_partial_state`:
  ordinary-game refusal, structures, unknown types, malformed coordinates,
  exact parser denial and spawn suppression during an internal pause.
- `lobby::sandbox_runtime_tests::sandbox_kaiju_beam_damages_enemy_and_delivers_events_without_horde_state`:
  actual ordinary sandbox beam damage and recipient delivery, then six more ticks.
- `recipient_snapshot::shots::tests::sandbox_breath_is_public_but_hidden_actor_ids_are_removed`:
  exact public beam/shell shapes, hidden actor identity redaction and the
  nonsandbox event guard.

The Horde checks use its bounded component, not a hosted Horde match. Sandbox
saves remain refused. Neither these tests nor this document claim Unreal client
adoption or owner acceptance. Actual execution results belong to the producing
PR head and are reported in that PR.
