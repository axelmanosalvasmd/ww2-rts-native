//! The ww2-rts rules core. It holds no engine, networking or file-system code; the server and the
//! Unreal client reach it as a library (`AGENTS.md`).

pub mod assault_runtime;
pub mod catalog;
pub mod client_view;
pub mod combat;
pub mod command_destinations;
pub mod conquest_end;
pub mod deployment;
pub mod economy;
pub mod economy_state;
pub mod environment;
pub mod environment_debris;
pub mod field_works;
pub mod fog;
pub mod formation;
pub mod game;
pub mod grid;
pub mod legacy_scenario;

pub mod logistics;
pub mod map;
pub mod match_abilities;
pub mod match_combat;
pub mod match_commands;
pub mod match_end;
pub mod match_firing;
pub mod match_logistics;
pub mod match_reactions;
pub mod match_recovery;
pub mod match_routing;
pub mod match_runtime;
pub mod match_story;
pub mod match_support;
pub mod match_tick;
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

pub mod random;
pub mod recipient_snapshot;
pub mod route_knowledge;
pub mod rules;
pub mod separation;
pub mod sort;
pub mod structures;
pub mod supply_convoys;
pub mod supply_network;
pub mod supply_routes;
pub mod terrain;
pub mod traffic;
pub mod unit_publication;
pub mod unit_spawn;
pub mod vehicle;
mod vehicle_navigation;
pub mod vision;
pub mod weather;
pub mod weather_runtime;
pub mod world;
pub mod world_generation;

pub mod world_map;

pub mod match_world;
pub mod match_world_rails;
pub mod match_world_regions;

pub mod match_world_memory;
pub mod world_memory;
pub mod world_recipient_metadata;

pub mod staged_observer;

pub mod world_route_retention;
