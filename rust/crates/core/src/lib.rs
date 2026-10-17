//! The ww2-rts rules core. It holds no engine, networking or file-system code; the server and the
//! Unreal client reach it as a library (`AGENTS.md`).

#![cfg_attr(test, recursion_limit = "256")]

// Test-only adapters vanish, including their unused arguments, in ordinary builds.
macro_rules! aligned_environment_draw {
    ($seed:expr, $replay:expr, $consumer:expr) => {{
        #[cfg(not(test))]
        {
            crate::weather_runtime::environment_random($seed)
        }
        #[cfg(test)]
        {
            crate::aligned_replay::environment_draw($seed, $replay, $consumer)
        }
    }};
}
#[cfg(test)]
extern crate self as ww2_core;
#[cfg(test)]
mod aligned_replay;
#[cfg(test)]
mod mov_runtime_batch;
#[cfg(test)]
mod native_order_intent_tests;

pub mod ai_schedule;
pub mod ai_view;
pub mod assault_runtime;
pub mod catalog;
pub mod client_view;
pub mod combat;
pub mod command_destinations;
pub mod command_feedback;
mod commander_actions;
pub mod commander_ai;
pub mod commander_assault;
pub mod commander_classic_rules;
mod commander_construction;
#[cfg(test)]
mod commander_engineering_tests;
mod commander_fortification;
mod commander_forward_hq;
pub mod commander_mind;
mod commander_mirror;
mod commander_shopping;
pub mod commander_tactics;
pub mod companion_ray;
pub mod companions;
pub mod conquest_end;
pub mod deployment;
pub mod economy;
pub mod economy_state;
pub mod elimination_modes;
#[cfg(test)]
mod engineering_runtime_assertion_tests;
pub mod environment;
pub mod environment_debris;
pub mod field_works;
pub mod fog;
pub mod formation;
pub mod game;
pub mod grid;
pub mod horde_commander;
pub mod horde_policy;
pub mod human_commander;
pub mod human_commander_inputs;
pub mod human_commander_policy;
pub mod human_commander_state;
pub mod human_commander_timing;
pub mod match_horde;
pub mod recipient_availability;

#[cfg(test)]
use crate as core_api;
#[cfg(test)]
#[path = "../tests/support/human_commander_facts.rs"]
mod human_commander_test_facts;
pub mod legacy_scenario;
pub mod legacy_scenario_runtime;
pub mod match_companion_effects;
mod match_companion_tick;
pub mod match_companions;
pub mod scenario_validation;
pub mod seat_commander;

pub(crate) mod existing_observers;
pub mod logistics;
pub mod map;
pub mod map_authoring;
pub mod map_save;
#[cfg(test)]
mod map_world_validation_tests;
pub mod match_abilities;
pub mod match_actor;
pub mod match_aircraft_commands;
pub mod match_aircraft_ticks;
pub mod match_aviation;
#[cfg(test)]
mod match_base_commands;
mod match_carrier_tick;
pub mod match_carriers;
pub mod match_combat;
pub mod match_commander;
pub mod match_commands;
pub mod match_construction;
pub mod match_cover;
mod match_destroy;
pub mod match_end;
pub mod match_firing;
pub mod match_garrison;

mod match_gates;
pub mod match_ground_contact;
pub mod match_ground_orders;
pub mod match_ground_support;
pub mod match_human_observation;
pub mod match_logistics;
mod match_rally;
pub mod match_reactions;
pub mod match_recovery;
pub mod match_routing;
pub mod match_runtime;
pub mod match_story;
pub mod match_support;
pub mod match_tick;
pub mod match_tutorial;
pub mod match_withdrawal;
pub mod match_world_elimination;
pub mod match_world_end;
pub mod math;
pub mod movement;
pub mod navigation;
pub mod observed_memory;
pub mod recipient_lists;

pub mod objectives;
pub mod path_requests;

pub mod orders;
pub mod pathfinding;
pub mod produced_units;
pub mod production;
pub mod projectiles;
pub mod recorded_input;

pub mod random;
pub mod recipient_snapshot;
pub mod route_knowledge;
pub mod rules;
pub mod scenario_conditions;
pub mod separation;
pub mod sort;
pub mod squad_men;
pub mod structures;
pub mod supply_convoys;
pub mod supply_network;
pub mod supply_routes;
pub mod terrain;
pub mod terrain_surface;
pub mod traffic;
pub mod traffic_knowledge;
pub mod tutorial_steps;
mod unit_placement;
pub mod unit_publication;
pub mod unit_spawn;
pub mod vehicle;
mod vehicle_navigation;
pub mod vision;
pub mod weather;
pub mod weather_runtime;
pub mod world;
mod world_bridge;
mod world_fill;
pub mod world_generation;

pub mod world_map;

pub mod match_world;
pub mod match_world_ground;
pub mod match_world_phases;
pub mod match_world_rails;
pub mod match_world_regions;
pub mod match_world_tick;

pub mod match_world_memory;
pub mod world_memory;
pub mod world_placement;
pub mod world_recipient_metadata;
pub mod world_supply;
pub mod world_territories;

pub mod staged_observer;

pub mod world_route_retention;

pub mod startup_tables;

#[cfg(test)]
mod cmb5_ability_tests;
#[cfg(test)]
mod cmb5_base_dispatch_tests;
#[cfg(test)]
mod cmb5_match_tests;
#[cfg(test)]
mod combat_feature_tests;

#[cfg(test)]
mod mov_audit_test_support;

#[cfg(test)]
mod engineering_intent_tests;

#[cfg(test)]
mod mov_intent_batch;

#[cfg(test)]
mod bases_runtime_construction_tests;

#[cfg(test)]
mod misc_bridge_tests;

#[cfg(test)]
mod misc_retreat_support_tests;

#[cfg(test)]
mod misc_map_save_tests;

#[cfg(test)]
#[path = "../../server/src/authoritative-map-data.rs"]
pub(crate) mod shipped_map_fixtures;

#[cfg(test)]
mod point_supply_intent_tests;

#[cfg(test)]
mod mov_followup_speed_tests;

#[cfg(test)]
mod mov_followup_garrison_tests;

mod save_codec;
mod save_state;

pub mod commander_field_policy;
pub mod commander_world_policy;
mod numeric_keys;
