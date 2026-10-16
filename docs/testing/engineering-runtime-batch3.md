# Engineering runtime batch 3

Reference: `e4e579a`. These behavior tests use native match state. No Source recorder or captured fixture was added.

- ENG-11: `eng11_concrete_line_becomes_four_finished_buildings_and_survives_match_ticks`, `eng11_occupied_concrete_retries_then_steps_the_crew_four_metres_back`, `eng11_six_second_cells_and_permanent_occupancy_have_a_bounded_lifecycle`, and `eng11_construction_mode_component_requires_engineers_for_concrete`. Check full payment, tier admission, four one-cell finished buildings, six-second work, ownership, rotation, occupied-cell retries, deferred crew movement, ordinary ticks and recipient captures.
- ENG-15: `queued_dig_defers_payment_then_survives_ordinary_ticks_and_recipient_snapshot`, `unaffordable_queued_dig_is_skipped_when_its_turn_arrives`, `dig_prerequisites_use_first_selection_and_queue_skips_only_current_payment`, `queued_bridge_uses_bearing_and_concrete_requires_its_tier`, and `queued_dig_observes_eight_order_limit_without_spending`. Check eligibility, known kinds, direction, delayed payment, refusal and queue lifecycle.
- ENG-24: `eng24_fill_and_wall_line_patterns_keep_material_and_middle_first_order`. Check all eight line materials, trench-only curved patterns, middle-first segment order and ordinary Fill/Wall project completion with recipient captures.

The runtime admission scope stays Default/Conquest. Classic and World field-work rooms remain guarded. The component builder check reserves concrete for Engineers in construction modes.

ENG-46 is deferred because fire-at command admission and its firing/movement progression are absent. ENG-58 is deferred because rail route selection and moving trains have no native runtime consumer, although physical bridge collapse already exists. No rows owned by #294 or #314 are claimed here. #294 was merged into the starting main; #314 is not a dependency.

Validation results are recorded in the pull request after the focused run, formatting and strict workspace Clippy.
