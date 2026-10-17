//! Host stamps for bounded ordinary commander input over the sole core queue.
use serde_json::Value;
use std::collections::BTreeMap;
use ww2_core::{
    match_commands::{Command, CommandResult, Denial, Stance},
    match_runtime::Match,
    orders::{OrderKey, QueueError},
};

#[derive(Default, Debug)]
pub(crate) struct CommanderIngress {
    sequence: u64,
    // Feedback names only. Canonical payloads live exclusively in Match's queue.
    feedback: BTreeMap<OrderKey, &'static str>,
}
impl CommanderIngress {
    pub fn recognizes(t: &str) -> bool {
        matches!(
            t,
            "move"
                | "amove"
                | "stop"
                | "stance"
                | "garrison"
                | "dig"
                | "entrench"
                | "cover"
                | "assist"
                | "retreat"
                | "rally"
                | "tech"
                | "cancel"
                | "cancelProduction"
                | "destroy"
                | Command::Buy { .. }
                | Command::Support { .. }
