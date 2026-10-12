//! The ww2-rts rules core. It holds no engine, networking or file-system code; the server and the
//! Unreal client reach it as a library (`AGENTS.md`).

pub mod fog;
pub mod formation;
pub mod game;
pub mod grid;
pub mod math;
pub mod rules;
pub mod sort;
pub mod vision;
