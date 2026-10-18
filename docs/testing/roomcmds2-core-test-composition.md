# Core test composition after broad qualification

The broad run at `036d5abb4723c06a010229d86eb956d82849181d` recorded two
obsolete core assertions in `/tmp/roomcmds2-broad.log`: Recon was admitted where
the old test expected `Unsupported`, and Horde snapshots succeeded where the
mode-row test expected `UnsupportedMode`. Current source admits all eight
support kinds and complete Conquest, Classic and Horde snapshots. This follow-up
changes tests only; execution of the replacements is pending the parent run.

`hosted_recorded_support_rejects_undeliverable_effects_before_any_payment_or_publication`
is replaced by these registered tests in
`match_commands/support_capability_tests.rs`:

- `hosted_recorded_plane_support_pays_once_and_delivers_every_seat_through_retirement`
  records Recon, Strafe, Bombing, Dive and Para separately. Each real match tick
  accepts the first command, charges the canonical MP price once and increments
  spending and support-call story fields once. A second recorded call fails on
  cooldown. Both seats receive full snapshots on every tick, exactly one actual
  arrival event, the active strike row and five further ticks after retirement.
  Para uses a currently visible allied position and retains population admission.
- `hosted_unknown_support_and_unsupported_command_reject_without_payment_or_publication`
  keeps the hosted unknown-support and native Unsupported-command guards.
  Rejection preserves economy, cooldowns, strikes, support effects, story,
  randomness and the actor allocator while consuming the recorded command.
- `unrestricted_recorded_recon_retains_independent_component_admission` keeps
  the separate component path. Recon is paid and published without the hosted
  restriction flag; unknown support remains `Blocked` and creates no extra call.

`mode01_non_world_mode_rows_match_the_prototype_public_fields` in
`mode_startup_tests.rs` retains its literal Conquest, Assault, Annihilation,
Classic, Horde and Tutorial rows. It checks current admitted modes by projecting
the initial mode fields for every seat, then collecting full snapshots for every
seat through six real ticks, including Horde's internal seat. Assault,
Annihilation and Tutorial still require `UnsupportedMode` at complete snapshot
admission. The sibling invalid mode-pair and timer tests remain unchanged.

The corresponding production owners are
`recipient_snapshot/support.rs::SUPPORTED_SUPPORT_KINDS`,
`match_commands.rs::execute_command`, `match_support.rs::admit_support` and
`recipient_snapshot.rs::validate_snapshot`. These replacements preserve their
current boundaries and add no World room, public mode or interface admission.
