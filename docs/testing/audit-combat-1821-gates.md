# Restored AUDIT_COMBAT gate plan

Base: `379ba4e06c794708623c005ab6052bebd03d8809`. Checkout:
`/home/judiazm/.t3/worktrees/ww2-rts-native/t3-3fa592e8`.
Branch: `audit/combat-1821-3fa592e8`. No execution grant is inferred from a free
ledger slot. Root must bind a frozen head, current guard and one S1/S2/S3 target.

## First source/test checkpoint

Two unique instances in package `ww2-simtest`, consolidated target `it`:

- `match_tick::cmb69_seeded_retirement_rejects_wrong_tick_identity_and_missing_or_extra_deaths`
- `match_tick::connected_tick_phases_match_actual_reference_steps`

The first runs the authentic retirement scene once for 800 actual ticks and
retains all 15 original checkpoint comparisons. Its eight altered traces are
comparison inputs only. The second retains all 19 original scenes. Neither
command regenerates Source or modifies fixtures.

After Root's exact slot grant, the proposed focused command from `rust/` is:

```sh
cargo nextest run --locked -p ww2-simtest --test it --build-jobs 4 --test-threads 1 -E 'test(=match_tick::cmb69_seeded_retirement_rejects_wrong_tick_identity_and_missing_or_extra_deaths) | test(=match_tick::connected_tick_phases_match_actual_reference_steps)'
```

Run the granted guard and resource/disk preflight before each job. Formatting
and strict core/simtest all-target Clippy require explicit inclusion in that
grant. Stop on the first unexpected substantive failure and retain its producing
head/log. No old Source, full workspace tests, release, discovery or CI run is
requested. The current module is already declared in `tests/it/main.rs`, so no
new module registration is needed.

## Batch boundary

Audit all 13 owned original checks against current source and existing hosted
proof before proposing Rust/Status cells. CMB-61 and the ten #262 rows are
already ported and excluded. The first five prospective checks are CMB-40,
CMB-41, CMB-42, CMB-64 and CMB-69; eligibility is pending whole-predicate readback,
not established by this list. Preserve every held row and prior failure.
CMB_IMPL owns the next combat publication lease. Root alone requests independent
actual-head review and publishes. No public action is authorized here.

## First-five successor

The successor adds three focused witnesses. Its exact requested affected scope
is five unique instances, with no Source generation or registration changes:

- `ww2-core::it`: `match_combat::cmb64_actual_retirement_pays_enemy_cost_bounty_and_excludes_friendly_death`
- `ww2-simtest::it`: `vision::cmb39_existing_tower_actor_sees_twelve_then_sixty_when_complete`
- `ww2-simtest::it`: `match_aviation::cmb65_mg_flak_and_fighter_hold_then_resume_permitted_anti_air`
- `ww2-simtest::it`: the new CMB-69 control named above
- `ww2-simtest::it`: the extracted existing connected-tick replay named above

Proposed commands after Root grants the successor head:

```sh
cargo nextest run --locked -p ww2-core --test it --build-jobs 4 --test-threads 1 -E 'test(=match_combat::cmb64_actual_retirement_pays_enemy_cost_bounty_and_excludes_friendly_death)'
cargo nextest run --locked -p ww2-simtest --test it --build-jobs 4 --test-threads 1 -E 'test(=vision::cmb39_existing_tower_actor_sees_twelve_then_sixty_when_complete) | test(=match_aviation::cmb65_mg_flak_and_fighter_hold_then_resume_permitted_anti_air) | test(=match_tick::cmb69_seeded_retirement_rejects_wrong_tick_identity_and_missing_or_extra_deaths) | test(=match_tick::connected_tick_phases_match_actual_reference_steps)'
```

Do not run the earlier two-name command as well as this successor set. Each
unique instance is requested once. Existing unchanged hosted component checks
remain attributed to producing `7a7348cb`; they do not need a new whole-match
gate. Formatting and strict core/simtest all-target Clippy remain conditional
on Root's explicit slot scope. No execution is claimed in this plan.
