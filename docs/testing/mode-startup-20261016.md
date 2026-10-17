# MODE-STARTUP intent checks

Prototype: `reference/ww2-rts` at `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`.
This batch covers MODE-06, BASE-02 and the non-World parts of BASE-52.

| Row | Registered tests | Prototype readback |
| --- | --- | --- |
| MODE-06 | `base03_army_keys_scale_population_income_and_starting_wallets`; `base02_mode06_non_world_constructors_scale_wallets_and_population`; `mode03_invalid_settings_and_mode_specific_restrictions_preserve_the_lobby`; `mode06_trusted_armies_keep_lobby_choice_wallets_receipts_and_both_seat_frames`; `mode06_binary_named_armies_start_and_deliver_to_both_sockets` | `shared/sim.js:66,801-802`; `server.js:669,674`; `test.js:1117-1143` |
| BASE-02, non-World | `base02_mode06_non_world_constructors_scale_wallets_and_population` | `shared/sim.js:29-35,793,802,837,845,1177,1682` |
| BASE-52, non-World | `base52_non_world_starting_bases_and_forces_follow_each_mode`; `base52_cramped_start_clears_fallback_pads_and_relocates_trapped_force`; `base52_first_fitting_site_moves_away_from_a_nearby_income_point` | `shared/sim.js:159,164,783-784,797,1108-1121,1575-1600,1679-1686` |

The constructor matrix uses actual Match constructors for Conquest, Assault,
Annihilation, Classic, Horde and Tutorial. It checks each seat's initial wallet
against literal prototype values and every named army's population multiplier.
Horde rejects Endless before adding its enemy. Other modes advance a real tick
and retain finite wallets and income. The existing BASE-03 test retains unknown
key normalization and literal Conquest income checks. The lobby test retains
host authority, invalid-value refusal and the reset from Endless when selecting
Horde.

The server previously allowed named army settings in the lobby but required
Standard at Start. Startup now passes the selected named setting into Match and
records it in the private host receipt. Both seated recipients receive the
existing army object. Tests cover initial wallets, both Start messages, six
ordinary ticks, scheduled frames for both seats, no room fault, and restart
preservation of the setting. A real server process also delivers three snapshots
per socket for every named army. Its old unsupported-setting test no longer
expects Large, Massive or Endless to fail.

Base tests check finished 3x3 HQs and Barracks, each mode's starting force,
Classic's centered HQ and Engineer, Horde's base owned only by player zero,
and Tutorial's human-only force without base buildings. An income point at the
spawn forces the first legal base sites to move away, with literal expected
positions; the open-map check also pins spacing from an earlier structure.
The causeway map has
no legal 3x3 footprint. Actual construction clears fallback pads; at least one
real starting squad intersects a new footprint and must move before the first
tick. Both recipients continue receiving snapshots for six ticks.

The former Conquest-only BASE-52 test and partial StartPlan wallet test are
replaced by these constructor checks. They are registered through the existing
`base_native_intent_tests` module despite `autotests = false`.

World admission and World clauses remain deferred. Non-Conquest room startup
and recipient adapters remain separate work. MODE-01, MODE-02, MODE-04 and MAP-06
remain next in this stream. This batch adds no persistent match state, command,
wire field, recipient rule, FFI or header change. No Source recorder runs.
