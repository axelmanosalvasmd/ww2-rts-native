//! The ww2-rts rules core. It holds no engine, networking or file-system code; the server and the
//! Unreal client reach it as a library (`AGENTS.md`).

pub mod catalog;
pub mod client_view;
pub mod combat;
pub mod match_logistics;
