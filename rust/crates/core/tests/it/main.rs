//! Every integration test of this crate, compiled as one test binary.
//!
//! Each file in `tests/` stays a module of this binary instead of its own
//! executable. One binary links once, which cuts CI build time, mostly on
//! Windows. A new test file needs a `#[path]` module below;
//! `every_test_file_is_compiled` fails until it has one.

#[path = "../authoritative_maps.rs"]
mod authoritative_maps;
#[path = "../base_audit_features.rs"]
mod base_audit_features;
#[path = "../client_view.rs"]
mod client_view;
#[path = "../combat.rs"]
mod combat;
#[path = "../commander_ai.rs"]
mod commander_ai;
#[path = "../convoys.rs"]
mod convoys;
#[path = "../deployment.rs"]
mod deployment;
#[path = "../economy.rs"]
mod economy;
#[path = "../economy_state.rs"]
mod economy_state;
#[path = "../engineering_weather.rs"]
mod engineering_weather;
#[path = "../environment.rs"]
mod environment;
#[path = "../environment_debris.rs"]
mod environment_debris;
#[path = "../environment_terrain.rs"]
mod environment_terrain;
#[path = "../field_projects.rs"]
mod field_projects;
#[path = "../field_works.rs"]
mod field_works;
#[path = "../formation.rs"]
mod formation;
#[path = "../fps_intent.rs"]
mod fps_intent;
#[path = "../grid.rs"]
mod grid;
#[path = "../human_commander_inputs.rs"]
mod human_commander_inputs;
#[path = "../human_commander_state.rs"]
mod human_commander_state;
#[path = "../human_commander_timing.rs"]
mod human_commander_timing;
#[path = "../interface_intent.rs"]
mod interface_intent;
#[path = "../legacy_scenario.rs"]
mod legacy_scenario;
#[path = "../logistics.rs"]
mod logistics;
#[path = "../logistics_features.rs"]
mod logistics_features;
#[path = "../map_world_batch2.rs"]
mod map_world_batch2;
#[path = "../match_combat.rs"]
mod match_combat;
#[path = "../match_routing.rs"]
mod match_routing;
#[path = "../math.rs"]
mod math;
#[path = "../mode_intent_batch1.rs"]
mod mode_intent_batch1;
#[path = "../mode_point_services.rs"]
mod mode_point_services;
#[path = "../objectives.rs"]
mod objectives;
#[path = "../observed_memory.rs"]
mod observed_memory;
#[path = "../produced_units.rs"]
mod produced_units;
#[path = "../production.rs"]
mod production;
#[path = "../projectiles.rs"]
mod projectiles;
#[path = "../recruitment.rs"]
mod recruitment;
#[path = "../route_knowledge.rs"]
mod route_knowledge;
#[path = "../runtime.rs"]
mod runtime;
#[path = "../startup_tables.rs"]
mod startup_tables;
#[path = "../structures.rs"]
mod structures;
#[path = "../supply_network.rs"]
mod supply_network;
#[path = "../supply_routes.rs"]
mod supply_routes;
#[path = "../weather.rs"]
mod weather;
#[path = "../weather_runtime.rs"]
mod weather_runtime;
#[path = "../world.rs"]
mod world;
#[path = "../world_generation.rs"]
mod world_generation;
#[path = "../world_maps.rs"]
mod world_maps;

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
