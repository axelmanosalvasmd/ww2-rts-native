# Prepared unit publication

`StagedUnit::ordinary` reuses the source scatter and unit initializer, then optionally replaces the final point. It accepts the canonical player's spawn, explicit ID and scatter index so a caller can prepare a complete source-ordered respawn batch. Mutable drafts allow HP and control overrides before publication validation. Produced units can supply their already placed Unit and control instead of repeating scatter or door logic.

`prepare_publication` receives disjoint mutable references to units, grid, controls, recruitment metadata and the allocator. It checks the whole contiguous batch, ID headroom, canonical existing identity/order, owner/kind, finite geometry/HP and control metadata before returning a guard. The guard holds those fields exclusively, while wallets, story, firing and reserves remain independently borrowable. Dropping it publishes nothing. An existing squad's post-payment HP can be validated and staged through the guard; HP changes wait until publication.

Publication consumes the guard, applies staged HP, inserts each unit in source order, maintains the grid, inserts canonical controls and optional recruitment facts, then advances the allocator. An infallible callback publishes already prepared metadata owned by firing, combat or reserves. Those owners must complete fallible validation before payment and must not duplicate unit position or HP. This primitive implements no purchase price, FPS eligibility or game-rule override itself.

The source-only prerequisite commit is intended to unblock the FPS consumer. It remains pending Rust verification and focused regression/review at this stage; no build or merge completion is claimed.
