# BASES-RUNTIME-2

Shipyards and Supply Caches previously returned `unsupported` from ordinary construction. They now use the existing paid site, footprint, builder and completion phases. Shipyards require a naval map and boat water within two cells of the footprint. Supply Caches require logistics. The internal `coast` denial uses the prototype's existing reason; no FFI, header, wire schema, contract or privacy code changes.

The prototype was inspected at `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`: `shared/sim.js:350-371`, `:3280-3315`, `:3689-3709`, `:3723-3736`, `:3781-3811`, `shared/tech.js`, and `test-audit-logistics.mjs:42-73`. These tests exercise intent directly and add no recorders or captured fixtures.

All five tests live under `match_construction::native_intent::runtime2`:

| Row | Test | Coverage |
| --- | --- | --- |
| BASE-17 | `base17_buy_denial_order_and_queued_unique_limits` | Shared purchase validation order, unchanged wallets and jobs on refusal, construction-only recruitment, successful purchases, and queued Tiger/Churchill limits. |
| BASE-39 | `base39_coastal_shipyard_builds_and_launches_its_boats` | Exact definition and maker list, naval/coastal and T2 admission, paid unfinished site, real ordinary ticks to completion, and all three boat purchases in water. |
| BASE-45 | `base45_paid_cache_requires_logistics_and_serves_only_when_finished` | Exact definition, logistics gate, paid site, unchanged truck and wallet state on unfinished-cache refusal, selected construction phases to completion, then immediate manual supply before another store sync. |
| BASE-49 | `base49_tiers_charge_pause_resume_and_survive_hq_loss` | Both tier prices and durations, construction-mode Munitions, finished own HQ requirement, duplicate refusal, retained pause/resume and tier persistence, plus actual Conquest recruitment from the researching HQ. |
| BASE-50 | `base50_armory_lines_tier_capacity_prices_and_retained_progress` | All three lines and levels, exact prices and durations, one job per available own finished Armory, shortages retaining progress, restored capacity, maximum levels and both tier boundaries. |

BASE-17 and the construction-mode research checks use the existing canonical production component over explicit actor facts. They do not admit Classic or World matches. BASE-45 uses the actual construction and convoy components; the full logistics tick guard remains in place and is asserted. BASE-49 and BASE-50 already have `ported` entries; this batch adds direct intent checks without editing their status.

FIX-308 addresses the rejected construction snapshot state: the adapter now projects the prototype's existing unit flag bit `128` and owner-only plan kind `8` with site coordinates (`shared/sim.js:5818-5835`, `:5883`). It still rejects a build flag without a matching nonzero control target. No row width, field, privacy filter or command admission changes. `shipyard_construction_survives_ticks_and_all_seat_snapshot_delivery` runs ordinary ticks through construction and worker cleanup, collecting complete snapshots for every seat each tick. BASE-45 additionally collects every seat's snapshot projection during the selected cache construction phases. Those cache projections retain their missing-capability list; complete logistics snapshots and full logistics ticks remain guarded.

FIX-308 validation on the Vostro: all 59 focused core library tests selected by `test(match_construction) | test(recipient_snapshot)` passed, as did `cargo fmt --all --check` and `cargo clippy -j4 --workspace --all-targets -- -D warnings`. Cargo ran in locked slot S1 with a 6 GB memory cap and no swap. Logs are in `/tmp/ww2-fix308-verification/`. No full workspace test suite or Source recorder ran.

The first focused run compiled and selected all five tests: three passed and two failed. Its 16-cell sea lay entirely inside the 10-cell shore margin, leaving no legal Destroyer cell; the revised arena has a 40-cell sea and explicitly requires deep water. The cache setup expected an automatic truck without creating supply demand; the revised test calls the canonical `add_truck` producer through a test-only helper, as the prototype audit does. The existing nearest-free fallback can still return a blocked cell when no suitable cell exists. This batch leaves that behavior unchanged.

Dropped from this batch: BASE-01 needs contract work for owner currency snapshots; BASE-02 needs the remaining mode startup paths; BASE-03 needs raw army-key normalization and those all-mode starting-wallet paths. The remaining assigned rows continue in the next batch.
