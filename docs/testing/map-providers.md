# Map providers

MAP-92 is a pure geography check against `reference/ww2-rts` at `e4e579a`, `shared/sim.js:1139-1162`. `world_map::validate_geography` accepts raw geography plus validated terrain, heights and spawn count, and returns the same success or diagnostic. `AuthoritativeMap` invokes it before constructing typed geography. The six diagnostics are `invalid world regions`, `invalid region membership`, `invalid world region`, `invalid world home`, `each player needs one home` and `world regions need connected ground routes`.

The validator preserves the prototype's four-element bounds check, unrestricted integer IDs, falsy optional membership, per-region home check order and four-way ground walk. Typed map construction still enforces its existing numeric storage and metadata restrictions. Validation success therefore does not promise that every raw authoring value can be materialized as a native map. This adds no match state or running World room.

MAP-92 maps to these registered unit tests in `map_world_validation_tests.rs`:

- `map92_region_sizes_counts_and_spans_match_prototype_diagnostics`
- `map92_membership_is_optional_or_one_bounded_integer_per_cell`
- `map92_objectives_require_unique_integer_ids_open_sites_and_four_bounds`
- `map92_each_spawn_has_exactly_one_valid_home`
- `map92_objective_routes_require_four_way_unblocked_one_level_steps`
- `map92_raw_authoring_validation_preserves_prototype_acceptance_and_error_order`

The existing tests now assert the prototype's complete diagnostic instead of a native field name. They also exercise the importer, so removing its validation call breaks the tests. The last test separately checks raw authoring acceptance where typed storage remains stricter.

Dropped rows:

- MAP-100: its two named acceptance checks require running World reconnect and hidden-state recipient envelopes (`test-world-acceptance.js:222,300`). World room admission is deferred. Ordinary Conquest component coverage cannot close that mixed row.
- IF-04: requires a new C query `placement_check(recipient_view, kind, x, z, dir, team) -> {ok, reason, cells?, x, z, c?, node?}`, using only recipient-known terrain, heights, layers, visible units, smoke, nodes, regions and remembered rail yards (`client/availability.js:187-198`, `shared/sim.js:3261-3305`). ABI 2 exposes no placement query.
- IF-12: requires C authoring calls `validate_map(map) -> error?`, `migrate_scenario/map(data) -> data`, `spawns_for(map, mode) -> indices`, `create_preview(map, match_options) -> setup_view`, `find_path(map, from, to) -> waypoints`, `house_kinds() -> metadata`, and `play_match(map, match_options, seed, max_ticks) -> winner, ticks, player_vp`. These cover `client/editor.js:60-168` and `client/fairness.js:5-20`; ABI 2 exposes none of them. These signatures identify missing queries, not approved interfaces.

No parity status, FFI, C header, wire shape or recipient privacy change is included. Runtime acknowledgment at the final PR head and hosted checks remain with the orchestrator.
