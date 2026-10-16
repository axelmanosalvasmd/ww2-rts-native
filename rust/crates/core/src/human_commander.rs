//! Detached facts for the ordinary human commander, independently of Match and transport.
//! The observation producer owns strategic memory. Human state owns camera attention.

use crate::{game::Point, human_commander_inputs};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq)]
pub struct HumanUnit {
    pub motion: human_commander_inputs::Unit,
    pub owner: u32,
    pub kind: String,
    pub hp: f64,
    pub air: bool,
    pub build: bool,
    pub dig: bool,
    pub entrench: bool,
    pub garrison: bool,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct HumanSighting {
    pub id: u32,
    pub payload: Value,
}

/// P is the complete detached strategic packet, never an authoritative Game borrow.
#[derive(Clone, Debug, PartialEq)]
pub struct HumanView<P> {
    pub tick: u64,
    pub spawn: Option<Point>,
    pub units: Vec<HumanUnit>,
    /// Original delivered visibility order, rather than a sorted replacement.
    pub visible: Vec<u32>,
    pub sightings: Vec<HumanSighting>,
    pub packet: P,
}

/// The planner rebuilds its input from these camera-filtered rows and detached packet.
pub struct ScreenView<'a, P> {
    pub tick: u64,
    pub spawn: Option<Point>,
    pub units: Vec<&'a HumanUnit>,
    pub visible: Vec<u32>,
    pub sightings: Vec<&'a HumanSighting>,
    pub packet: &'a P,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hurt {
    /// Stable ordinal in the original complete event batch, including skipped rows.
    pub ordinal: u64,
    pub id: u32,
    pub at: Point,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OwnerHurtBatch {
    /// Changes when the Source shot array is replaced, not when it is appended.
    pub generation: u64,
    /// Exclusive raw batch length. Filtering does not renumber events.
    pub through_ordinal: u64,
    /// Only actual owner hurt events with a victim and finite coordinates.
    pub events: Vec<Hurt>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TickControl<'a> {
    pub now: u64,
    pub live_owner_out: bool,
    pub winner: bool,
    pub level: &'a str,
}
