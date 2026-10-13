//! The ww2-rts rules core. It holds no engine, networking or file-system code; the server and the
//! Unreal client reach it as a library (`AGENTS.md`).

pub mod catalog;
pub mod economy;
pub mod fog;
pub mod formation;
pub mod game;
pub mod grid;
pub mod math;
pub mod movement;
pub mod navigation;
pub mod rules;
pub mod separation;
pub mod sort;
pub mod terrain;
pub mod traffic;
pub mod vehicle;
pub mod vision;
