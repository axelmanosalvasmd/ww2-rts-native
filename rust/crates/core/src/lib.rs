//! The ww2-rts rules core. It holds no engine, networking or file-system code; the server and the
//! Unreal client reach it as a library (`AGENTS.md`).

pub mod catalog;
pub mod client_view;
pub mod combat;
pub mod deployment;
pub mod economy;
pub mod economy_state;
pub mod environment;
pub mod environment_debris;
pub mod fog;
pub mod formation;
pub mod game;
pub mod grid;
pub mod legacy_scenario;

pub mod logistics;
pub mod map;
pub mod match_runtime;
pub mod match_tick;
pub mod math;
pub mod movement;
pub mod navigation;
pub mod observed_memory;

pub mod objectives;
pub mod path_requests;

pub mod orders;
pub mod pathfinding;
pub mod produced_units;
pub mod production;
pub mod projectiles;

pub mod random;
pub mod route_knowledge;
pub mod rules;
pub mod separation;
pub mod sort;
pub mod structures;
pub mod supply_convoys;
pub mod terrain;
pub mod traffic;
pub mod unit_spawn;
pub mod vehicle;
mod vehicle_navigation;
pub mod vision;
pub mod weather;
pub mod weather_runtime;
pub mod world;
