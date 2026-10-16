# MAP runtime batch 3

This batch starts at main `09e03d07`. The reference is `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`. PR #310's branch is preserved. No parity-status files are changed.

## Row coverage

| Row | Test | Prototype and coverage |
| --- | --- | --- |
| MAP-34 | `map34_recorded_fill_clears_rubble_and_survives_ticks_and_recipient_snapshots`; `map34_fillability_world_scenery_and_slope_limited_surface_restore` | `shared/sim.js:433,466,2753-2769,5385-5404`. Ordinary paid Fill is admitted through a recorded command. The match runs 160 ticks, captures both recipient snapshots through active work and completion, then retires an active worker and captures again. Component checks cover original World rubble and fords, fresh damage, road restoration and slope-limited height repair. World construction and field-work admission stay closed. |
| MAP-58 | `map58_region_lookup_matches_prototype_membership_grid_and_bounds`; `map58_known_region_matches_prototype_runs_and_legacy_bounds` | `shared/sim.js:1123-1133`; `shared/world-territories.js:132-145`. Exact membership indices, negative and off-map queries, legacy grid and partial columns, bounds fallback, metre conversion and half-open explored runs. Bounds-only geography is a standalone typed query with zero span; valid World documents still require regionCells 64. |
| MAP-24 | `map24_public_sanitizer_preserves_underlying_materials_and_removes_private_mines_and_scripts`; `map24_catalogue_public_projection_matches_every_committed_public_asset` | `shared/world-layers.js:68-79`. Exact authored object-layer mines, separate mine layers, mines above an object, all four material lists, script removal and immutable input. Every committed public asset must equal the runtime projection. The sanitizer is used by HTTP and startup. |
| MAP-104 | `map104_material_startup_uses_public_projection_then_only_delivers_observed_changes` | `shared/sim.js:1610-1614,5842-5860`; `server.js:463`. An arbitrary material-bearing authored map passes the same startup binding and envelope path as catalogue maps. The public startup hides mine materials and scripts. Six ordinary ticks and recipient deliveries expose a visible rubble change while withholding a remote ground change. |
| MAP-60 | `map60_territory_edges_match_prototype_order_and_remove_seams` | `shared/world-territories.js:108-130`. Literal edge lists for adjacent rows, gaps, staggered rows and separated runs, in prototype order. |

## Runtime compatibility

Main gained the shared Fill implementation through PR #294 during this batch's validation. The merge keeps that implementation unchanged and removes this branch's duplicate. MAP-34 now tests main's `FortKind::Fill` and terrain repair. Tick admission (`field_project_tick.rs`), recipient admission (`field_work_snapshot.rs`), work-row projection (`recipient_snapshot/engineering.rs`) and retirement cancellation consume generic fort definitions and jobs. Command admission still refuses World field works. No new stored field, link, event or queue format is introduced by this PR.

Region helpers are pure queries over existing geography and memory. The sanitizer returns a copy and adds no match state. FFI, C headers, wire shapes, contracts and privacy policy remain unchanged.

## Deferred rows and candidates

MAP-79 remains open because actual post-setup barracks placement requires World construction admission. Existing geometry reserves do not prove the public placement acceptance check. MAP-100 remains open because complete World snapshots are refused; internal memory tests cannot prove delivered snapshot or reconnect invariance.

MAP-06 is a differs candidate: retain native field-specific spawn diagnostics with the same rejection intent instead of copying the prototype's wording.

MAP-92 has field-specific diagnostics and stricter numeric, ordered bounds validation. Its remaining matrix needs separate review; the whole row is not classified as a wording-only difference here.

Next rows: MAP-19, MAP-92, MAP-15, MAP-23, MAP-57 and MAP-90. MAP-22, MAP-59 and MAP-62 are outside this stream.

## Validation

At producer `119f53feb87d25e3eaed0d41f2f52f3f4620f15e`, after merging main `d505e6eb`, all eight focused checks pass (Nextest run `2398fc73-6a5e-4f08-84fa-ecf8ff0a73f6`). Formatting and strict workspace/all-target Clippy also pass. The tested Rust tree is `6156de4b77eacf6fc9adc3d05797fae9ad5a47cb`; the final documentation commit preserves it exactly.

The preceding integration through main `2564cdaf` passed 45 focused and regression checks, including shared engineering, public catalogue delivery and real socket startup/reconnect (Nextest run `f987e45a-fb2b-4cfe-ba4e-ed38f161532d`, Rust tree `50a259e8e7f6f3d014424c34d74d85973f9c11fa`). Those results remain attributed to that tree. All Cargo jobs ran on the Vostro in locked S1/S2 slots, with four build jobs, a 6 GiB memory cap and no swap. Slots were released on exit.

Independent source review accepts the five row mappings. The material projection's literal expectations also pass against pinned `mapForClient`. No behavior recorder or captured fixture was added.
