//! Typed authoritative geography exchanged between the native generator and map loader.
//! These records are authoring data, not live ownership, capture progress or a Match.
use crate::map::{Building, Layers, Spawn};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorldSize {
    Huge,
    Massive,
}
impl WorldSize {
    pub fn name(self) -> &'static str {
        match self {
            Self::Huge => "huge",
            Self::Massive => "massive",
        }
    }
    pub fn region_count(self) -> usize {
        match self {
            Self::Huge => 64,
            Self::Massive => 128,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorldRegion {
    pub id: i32,
    pub x: usize,
    pub y: usize,
    pub bounds: [usize; 4],
    pub home: Option<usize>,
    pub kind: Option<String>,
    pub name: Option<String>,
    pub biome: Option<String>,
    /// Original generator hint. Runtime ownership is resolved from actual match players later.
    pub team: Option<i32>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct WorldDefinition {
    pub size: WorldSize,
    pub region_cells: usize,
    pub regions: Vec<WorldRegion>,
    /// Values index the ordered region vector, as in the source. Region IDs are not reindexed.
    pub region_map: Option<Vec<i16>>,
    pub biomes: Option<Vec<u8>>,
    pub seed: Option<u32>,
    pub generator_version: Option<u32>,
    /// Uninterpreted source fields, including waterways and rails. No algorithm consumes these here.
    pub metadata: BTreeMap<String, Value>,
}
#[derive(Clone, Debug)]
pub struct WorldMapInput {
    pub name: String,
    pub width: usize,
    pub height: usize,
    pub chars: Vec<u8>,
    pub heights: Option<Vec<i8>>,
    /// Normalized authoritative layers. None explicitly selects the source's rows-only migration.
    pub layers: Option<Layers>,
    pub spawns: Vec<Spawn>,
    pub buildings: Vec<Building>,
    pub geography: WorldDefinition,
}
