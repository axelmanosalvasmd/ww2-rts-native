//! Authoritative mutable terrain and its ordered change records.
//!
//! `Game` owns the live terrain arrays and section health. The additional layers here never
//! duplicate those arrays. The initial snapshot is intentionally separate and immutable.
//! Rows are authoritative internal data, not recipient-filtered wire messages.
use crate::{
    game::{Game, LAND, MOVE, SHOAL, Section, VBLOCK},
    map::{AuthoritativeMap, Layers, MapError, TerrainState, flags, start_wear},
    math,
    rules::Rules,
    structures::{SectionState, StructuredTerrain},
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorldError {
    InvalidCell,
    InvalidChange,
    InvalidTerrain,
    VersionExhausted,
    InvalidJournalCursor,
}
impl std::fmt::Display for WorldError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::InvalidCell => "cell is outside the world",
                Self::InvalidChange => "invalid terrain change",
                Self::InvalidTerrain => "terrain does not match its authoritative map",
                Self::VersionExhausted => "terrain version capacity exhausted",
                Self::InvalidJournalCursor => "journal cursor is outside the retained range",
            }
        )
    }
}
impl std::error::Error for WorldError {}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CellChange {
    pub ground: Option<u8>,
    pub object: Option<u8>,
    pub mine: Option<bool>,
    pub material: Option<String>,
    pub height: Option<i8>,
}
#[derive(Clone, Debug)]
pub struct InitialTerrain {
    pub chars: Vec<u8>,
    pub ground: Vec<u8>,
    pub objects: Vec<u8>,
    pub mines: Vec<u8>,
    pub height: Option<Vec<i8>>,
    pub trench_front: Option<Vec<f32>>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Appearance {
    Intact,
    Damaged,
    Burning,
}
impl Appearance {
    fn name(self) -> &'static str {
        match self {
            Self::Intact => "intact",
            Self::Damaged => "damaged",
            Self::Burning => "burning",
        }
    }
}
/// Map-like timers with explicit first-insertion order, as required by fire RNG traversal.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OrderedFires {
    order: Vec<usize>,
    timers: BTreeMap<usize, f64>,
}
impl OrderedFires {
    pub fn insert(&mut self, cell: usize, timer: f64) -> Option<f64> {
        if !self.timers.contains_key(&cell) {
            self.order.push(cell);
        }
        self.timers.insert(cell, timer)
    }
    pub fn remove(&mut self, cell: &usize) -> Option<f64> {
        let old = self.timers.remove(cell);
        if old.is_some() {
            self.order.retain(|c| c != cell);
        }
        old
    }
    pub fn contains_key(&self, cell: &usize) -> bool {
        self.timers.contains_key(cell)
    }
    pub fn get(&self, cell: &usize) -> Option<&f64> {
        self.timers.get(cell)
    }
    pub fn len(&self) -> usize {
        self.timers.len()
    }
    pub fn is_empty(&self) -> bool {
        self.timers.is_empty()
    }
    pub fn iter(&self) -> impl Iterator<Item = (&usize, &f64)> {
        self.order.iter().map(|c| (c, &self.timers[c]))
    }
    pub fn clear(&mut self) {
        self.order.clear();
        self.timers.clear();
    }
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FailureMetadata {
    pub impulse: Option<f64>,
    pub failed_at: Option<f64>,
    pub direction: Option<crate::game::Point>,
    pub settled_at: Option<f64>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct SectionMetadata {
    pub id: String,
    pub structure_id: String,
    pub material: String,
    pub anchor: bool,
    pub supports: Vec<String>,
    pub rubble: Vec<usize>,
    pub appearance: Appearance,
    pub failure: FailureMetadata,
}
#[derive(Clone, Debug, PartialEq)]
pub struct DetachedSection {
    pub cell: usize,
    pub metadata: SectionMetadata,
    pub state: Section,
}
#[derive(Clone, Debug, PartialEq)]
pub struct WorldStructure {
    pub id: String,
    pub kind: &'static str,
    pub cells: Vec<usize>,
    pub section_ids: Vec<String>,
    pub entity_id: Option<u32>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct SectionRow {
    pub id: String,
    pub structure_id: String,
    pub state: &'static str,
    pub hp: f64,
    pub max_hp: f64,
    pub material: String,
    pub anchor: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub struct CellData {
    pub ground: u8,
    pub object: u8,
    pub material: String,
    pub ground_material: String,
    pub ground_wear: f32,
    pub trench_front: Option<f32>,
    pub mine: bool,
    pub mine_material: Option<String>,
    pub section: Option<SectionRow>,
    /// Optional World rail facts captured when this row was logged or observed.
    pub rail: Option<u8>,
    pub station: Option<usize>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct CellRow {
    pub cell: usize,
    pub ch: u8,
    pub height: i8,
    pub state: u8,
    pub data: CellData,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Recipient {
    Player(usize),
    Spectators,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RegionVersions {
    pub infantry: u64,
    pub vehicle: u64,
    pub naval: u64,
    pub obstruction: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mutation {
    pub logged_index: Option<usize>,
}

#[derive(Debug)]
pub struct World {
    pub game: Game,
    pub mine_layer: Vec<u8>,
    pub materials: BTreeMap<usize, String>,
    pub base_materials: BTreeMap<usize, String>,
    pub object_materials: BTreeMap<usize, String>,
    pub mine_materials: BTreeMap<usize, String>,
    pub surface_wear: Vec<f32>,
    pub burnt: Vec<u8>,
    pub scar: Vec<f32>,
    pub cell_state: Vec<u8>,
    pub shoal: Option<Vec<u8>>,
    pub roads: bool,
    pub mines: BTreeMap<usize, i32>,
    pub mine_seen: BTreeMap<usize, u32>,
    pub aid: BTreeMap<usize, usize>,
    pub fires: OrderedFires,
    pub structures: Vec<WorldStructure>,
    pub sections: BTreeMap<usize, SectionMetadata>,
    pub detached_sections: BTreeMap<String, DetachedSection>,
    initial: InitialTerrain,
    geography: Option<crate::world_map::WorldDefinition>,
    pub versions: RegionVersions,
    rules: Rules,
    log: Vec<CellRow>,
    log_indexes: BTreeMap<usize, usize>,
    new_cells: Vec<CellRow>,
    pending: BTreeMap<Recipient, BTreeSet<usize>>,
    journal: Vec<usize>,
    journal_base: u64,
    soak: Vec<i64>,
    soak_members: BTreeSet<i64>,
}
/// Deferred recipient effects, checked before an infallible publication.
pub(crate) struct ObservationReceipt {
    frame: Option<ObservationSource>,
    action: ObservationAction,
}
#[derive(PartialEq)]
struct ObservationSource {
    initial: usize,
    shape: (usize, usize),
    key: (i64, Option<i64>, u32, bool),
    weather: (Option<usize>, u64, Option<u64>),
    log: usize,
    journal: (u64, usize),
    units: Vec<ObservationUnit>,
    players: Vec<(usize, i32, Vec<u32>)>,
    smokes: Vec<crate::game::Smoke>,
    strikes: Vec<crate::game::Strike>,
    wrecks: Vec<crate::game::Wreck>,
    fires: Vec<usize>,
    mines: BTreeMap<usize, i32>,
    mine_seen: BTreeMap<usize, u32>,
}
#[derive(PartialEq)]
struct ObservationUnit {
    id: u32,
    kind: usize,
    owner: usize,
    values: [u64; 3],
    built: Option<u64>,
    garrison: Option<usize>,
    air: Option<crate::game::PlaneState>,
    cells: Option<Vec<usize>>,
}
enum ObservationAction {
    None,
    Acknowledge {
        recipient: Recipient,
        indices: Vec<usize>,
        prior: Option<BTreeSet<usize>>,
    },
    Trim {
        prefix: usize,
        at: u64,
    },
}
/// Locks only the source delivery ledger, leaving Game available for unit publication.
pub(crate) struct PreparedObservation<'a> {
    pending: &'a mut BTreeMap<Recipient, BTreeSet<usize>>,
    log_len: usize,
    journal: &'a mut Vec<usize>,
    base: &'a mut u64,
    action: ObservationAction,
}
impl PreparedObservation<'_> {
    pub(crate) fn commit(self) {
        match self.action {
            ObservationAction::None => {}
            ObservationAction::Acknowledge {
                recipient, indices, ..
            } => {
                let pending = self
                    .pending
                    .entry(recipient)
                    .or_insert_with(|| (0..self.log_len).collect());
                for index in indices {
                    pending.remove(&index);
                }
            }
            ObservationAction::Trim { prefix, at } => {
                self.journal.drain(..prefix);
                *self.base = at;
            }
        }
    }
}
impl ObservationReceipt {
    pub(crate) fn prepare(
        self,
        world: &mut World,
    ) -> Result<(&mut Game, PreparedObservation<'_>), String> {
        if self
            .frame
            .as_ref()
            .is_some_and(|frame| *frame != world.observation_source())
        {
            return Err("observation source changed before publication".into());
        }
        if let ObservationAction::Acknowledge {
            recipient, prior, ..
        } = &self.action
            && prior.as_ref() != world.pending(*recipient)
        {
            return Err("observation pending ledger changed before publication".into());
        }
        Ok((
            &mut world.game,
            PreparedObservation {
                pending: &mut world.pending,
                log_len: world.log.len(),
                journal: &mut world.journal,
                base: &mut world.journal_base,
                action: self.action,
            },
        ))
    }
    /// Used only by the ordinary immediate wrappers after their existing validations.
    pub(crate) fn commit(self, world: &mut World) -> Result<(), String> {
        let (_, prepared) = self.prepare(world)?;
        prepared.commit();
        Ok(())
    }
}
