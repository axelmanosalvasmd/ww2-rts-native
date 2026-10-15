//! Every integration test of this crate, compiled as one test binary.
//!
//! Each file in `tests/` stays a module of this binary instead of its own
//! executable. One binary links once, which cuts CI build time, mostly on
//! Windows. A new test file needs a `#[path]` module below;
//! `every_test_file_is_compiled` fails until it has one.

#![recursion_limit = "256"]

/// Helpers used by more than one test file, compiled once for the whole binary.
#[path = "../common"]
mod shared {
    pub(crate) mod aviation;
    pub(crate) mod companion_tick;
    pub(crate) mod firing;
    pub(crate) mod staged_observer;
    pub(crate) mod startup;
    pub(crate) mod startup_projection;
    pub(crate) mod world_scene;
}

#[path = "../ability_flights.rs"]
mod ability_flights;
#[path = "../air_firing_context.rs"]
mod air_firing_context;
#[path = "../air_retirement.rs"]
mod air_retirement;
#[path = "../assault_finish.rs"]
mod assault_finish;
#[path = "../assault_start.rs"]
mod assault_start;
#[path = "../base_ordinary_checks.rs"]
mod base_ordinary_checks;
#[path = "../compiled_rules.rs"]
mod compiled_rules;
#[path = "../conquest_finish.rs"]
mod conquest_finish;
#[path = "../conquest_mode_batch.rs"]
mod conquest_mode_batch;
#[path = "../conquest_team_result.rs"]
mod conquest_team_result;
#[path = "../construction_tick.rs"]
mod construction_tick;
#[path = "../engineering_consumers.rs"]
mod engineering_consumers;
#[path = "../engineering_travel.rs"]
mod engineering_travel;
#[path = "../fps_effects.rs"]
mod fps_effects;
#[path = "../fps_runtime.rs"]
mod fps_runtime;
#[path = "../ground_orders.rs"]
mod ground_orders;
#[path = "../ground_support_tick.rs"]
mod ground_support_tick;
#[path = "../legacy_scenario_tick.rs"]
mod legacy_scenario_tick;
#[path = "../map_height_queries.rs"]
mod map_height_queries;
#[path = "../mass_project_tick.rs"]
mod mass_project_tick;
#[path = "../match_aircraft_runner.rs"]
mod match_aircraft_runner;
#[path = "../match_aviation.rs"]
mod match_aviation;
#[path = "../match_carrier_tick.rs"]
mod match_carrier_tick;
#[path = "../match_economy.rs"]
mod match_economy;
#[path = "../match_firing.rs"]
mod match_firing;
#[path = "../match_production.rs"]
mod match_production;
#[path = "../match_recruitment.rs"]
mod match_recruitment;
#[path = "../match_start.rs"]
mod match_start;
#[path = "../match_story.rs"]
mod match_story;
#[path = "../match_supply.rs"]
mod match_supply;
#[path = "../match_support.rs"]
mod match_support;
#[path = "../match_tick.rs"]
mod match_tick;
#[path = "../match_tick_companions.rs"]
mod match_tick_companions;
#[path = "../match_tick_events.rs"]
mod match_tick_events;
#[path = "../mode14_spawn_layout.rs"]
mod mode14_spawn_layout;
#[path = "../mode_predicates.rs"]
mod mode_predicates;
#[path = "../mode_startup_checks.rs"]
mod mode_startup_checks;
#[path = "../mov_vehicle_predicates.rs"]
mod mov_vehicle_predicates;
#[path = "../movement.rs"]
mod movement;
#[path = "../movement_existing_checks.rs"]
mod movement_existing_checks;
#[path = "../movement_routing.rs"]
mod movement_routing;
#[path = "../navigation.rs"]
mod navigation;
#[path = "../ordinary_production_tick.rs"]
mod ordinary_production_tick;
#[path = "../path_requests.rs"]
mod path_requests;
#[path = "../pathfinding.rs"]
mod pathfinding;
#[path = "../recipient_effects.rs"]
mod recipient_effects;
#[path = "../recipient_engineering.rs"]
mod recipient_engineering;
#[path = "../recipient_legacy_say.rs"]
mod recipient_legacy_say;
#[path = "../staged_observer.rs"]
mod staged_observer;
#[path = "../supply_withdrawal.rs"]
mod supply_withdrawal;
#[path = "../support_coherence.rs"]
mod support_coherence;
#[path = "../terrain_contract.rs"]
mod terrain_contract;
#[path = "../terrain_scout.rs"]
mod terrain_scout;
#[path = "../tutorial_steps.rs"]
mod tutorial_steps;
#[path = "../unit_publication.rs"]
mod unit_publication;
#[path = "../unit_spawn.rs"]
mod unit_spawn;
#[path = "../view.rs"]
mod view;
#[path = "../vision.rs"]
mod vision;
#[path = "../world_command_prelude.rs"]
mod world_command_prelude;
#[path = "../world_elimination.rs"]
mod world_elimination;
#[path = "../world_epilogue.rs"]
mod world_epilogue;
#[path = "../world_finish.rs"]
mod world_finish;
#[path = "../world_memory.rs"]
mod world_memory;
#[path = "../world_phases.rs"]
mod world_phases;
#[path = "../world_regions.rs"]
mod world_regions;
#[path = "../world_route_retention.rs"]
mod world_route_retention;
#[path = "../world_start.rs"]
mod world_start;
#[path = "../world_tick.rs"]
mod world_tick;

/// Fails when a `tests/*.rs` file is not compiled into this binary. Cargo's
/// `autotests = false` would otherwise skip it without a word. The check reads
/// this file's own `#[path = "../<name>.rs"] mod <name>;` declarations, so a
/// removed declaration fails as surely as a missing one. It also rejects
/// directory test targets (`tests/<dir>/main.rs`) other than this one.
#[test]
fn every_test_file_is_compiled() {
    let tests = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let main = std::fs::read_to_string(tests.join("it/main.rs")).expect("read tests/it/main.rs");
    let lines: Vec<&str> = main.lines().map(str::trim).collect();
    let mut declared: Vec<String> = lines
        .windows(2)
        .filter_map(|pair| {
            let name = pair[0]
                .strip_prefix("#[path = \"../")?
                .strip_suffix(".rs\"]")?;
            (pair[1] == format!("mod {name};")).then(|| name.to_owned())
        })
        .collect();
    declared.sort();
    let mut on_disk = Vec::new();
    for entry in std::fs::read_dir(&tests).expect("read tests directory") {
        let path = entry.expect("read tests entry").path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if path.is_dir() {
            assert!(
                name == "it" || !path.join("main.rs").exists(),
                "tests/{name}/main.rs is a separate test target; make it a module of tests/it/main.rs"
            );
        } else if let Some(stem) = name.strip_suffix(".rs") {
            on_disk.push(stem.to_owned());
        }
    }
    on_disk.sort();
    assert_eq!(
        on_disk, declared,
        "each tests/*.rs file needs `#[path = \"../<name>.rs\"]` and `mod <name>;` in tests/it/main.rs"
    );
}
