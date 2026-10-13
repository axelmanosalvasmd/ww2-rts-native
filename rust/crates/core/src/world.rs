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
    pub versions: RegionVersions,
    rules: Rules,
    log: Vec<CellRow>,
    log_indexes: BTreeMap<usize, usize>,
    new_cells: Vec<CellRow>,
    pending: BTreeMap<Recipient, BTreeSet<usize>>,
    journal: Vec<usize>,
    soak: Vec<i64>,
    soak_members: BTreeSet<i64>,
}
impl World {
    pub fn from_map(map: &AuthoritativeMap) -> Result<Self, MapError> {
        Self::from_structured(map, map.initialize_structures()?).map_err(|e| MapError::Invalid {
            field: "world".into(),
            reason: e.to_string(),
        })
    }
    /// Consume the prepared terrain. `Game` takes ownership of every live query array.
    pub fn from_structured(
        map: &AuthoritativeMap,
        input: StructuredTerrain,
    ) -> Result<Self, WorldError> {
        let t = input.terrain;
        let n = t.w.checked_mul(t.h).ok_or(WorldError::InvalidTerrain)?;
        if map.dimensions() != (t.w, t.h)
            || t.chars.len() != n
            || t.flags.len() != n
            || t.house.len() != n
            || t.cell_hp.len() != n
            || t.wear.len() != n
            || t.surface_wear.len() != n
            || t.layers.ground.len() != n
            || t.layers.objects.len() != n
            || t.layers.mines.len() != n
            || t.height.as_ref().is_some_and(|v| v.len() != n)
            || t.trench_front.as_ref().is_some_and(|v| v.len() != n)
            || t.tough.as_ref().is_some_and(|v| v.len() != n)
            || t.shoal.as_ref().is_some_and(|v| v.len() != n)
        {
            return Err(WorldError::InvalidTerrain);
        }
        let initial = InitialTerrain {
            chars: t.chars.clone(),
            ground: t.layers.ground.clone(),
            objects: t.layers.objects.clone(),
            mines: t.layers.mines.clone(),
            height: t.height.clone(),
            trench_front: t.trench_front.clone(),
        };
        let TerrainState {
            w,
            h,
            chars,
            layers,
            flags,
            height,
            house,
            cell_hp,
            tough,
            wear,
            surface_wear,
            trench_front,
            shoal,
            roads,
            naval,
        } = t;
        let Layers {
            ground,
            objects,
            mines: mine_layer,
            materials,
            base_materials,
            object_materials,
            mine_materials,
        } = layers;
        let game = Game {
            w,
            h,
            chars,
            ground: Some(ground),
            objects: Some(objects),
            flags,
            height,
            house,
            cell_hp,
            tough,
            wear: Some(wear),
            trench_front,
            naval,
            wet: None,
            reveal: false,
            wrecks: Vec::new(),
            tick: 0,
            vision_tick: None,
            terrain_version: 0,
            mode: None,
            weather: None,
            rain: 0.0,
            players: Vec::new(),
            units: BTreeMap::new(),
            smokes: Vec::new(),
            strikes: Vec::new(),
            sections: BTreeMap::new(),
        };
        let rules = Rules::compiled().map_err(|_| WorldError::InvalidTerrain)?;
        let mines = mine_layer
            .iter()
            .enumerate()
            .filter(|(_, m)| **m != 0)
            .map(|(c, _)| (c, -1))
            .collect();
        let soak: Vec<_> = (0..n as i64).collect();
        let soak_members = soak.iter().copied().collect();
        let mut world = Self {
            game,
            mine_layer,
            materials,
            base_materials,
            object_materials,
            mine_materials,
            surface_wear,
            burnt: vec![0; n],
            scar: vec![0.0; n],
            cell_state: vec![0; n],
            shoal,
            roads,
            mines,
            mine_seen: BTreeMap::new(),
            aid: BTreeMap::new(),
            fires: OrderedFires::default(),
            structures: Vec::new(),
            sections: BTreeMap::new(),
            detached_sections: BTreeMap::new(),
            initial,
            versions: RegionVersions::default(),
            rules,
            log: Vec::new(),
            log_indexes: BTreeMap::new(),
            new_cells: Vec::new(),
            pending: BTreeMap::new(),
            journal: Vec::new(),
            soak,
            soak_members,
        };
        for s in input.structures.into_structures() {
            let mut cells = Vec::new();
            let mut section_ids = Vec::new();
            for section in s.sections {
                if section.cell >= n || world.sections.contains_key(&section.cell) {
                    return Err(WorldError::InvalidTerrain);
                }
                cells.push(section.cell);
                section_ids.push(section.id.clone());
                world.game.sections.insert(
                    section.cell as u32,
                    Section {
                        failed: section.state == SectionState::Failed,
                        hp: section.hp,
                        max_hp: Some(section.max_hp),
                    },
                );
                world.sections.insert(
                    section.cell,
                    SectionMetadata {
                        id: section.id,
                        structure_id: section.structure_id,
                        material: section.material,
                        anchor: section.anchor,
                        supports: section.supports,
                        rubble: section.rubble,
                        failure: FailureMetadata::default(),
                        appearance: match section.state {
                            SectionState::Damaged => Appearance::Damaged,
                            SectionState::Burning => Appearance::Burning,
                            _ => Appearance::Intact,
                        },
                    },
                );
            }
            world.structures.push(WorldStructure {
                id: s.id,
                kind: s.kind.as_str(),
                cells,
                section_ids,
                entity_id: None,
            });
        }
        for c in 0..n {
            world.cell_state[c] = world.state_of(c);
        }
        Ok(world)
    }
    pub fn section_by_id(&self, id: &str) -> Option<(&SectionMetadata, &Section)> {
        if let Some((cell, metadata)) = self.sections.iter().find(|(_, m)| m.id == id) {
            return self
                .game
                .sections
                .get(&(*cell as u32))
                .map(|state| (metadata, state));
        }
        self.detached_sections
            .get(id)
            .map(|section| (&section.metadata, &section.state))
    }
    pub(crate) fn section_by_id_mut(
        &mut self,
        id: &str,
    ) -> Option<(&mut SectionMetadata, &mut Section)> {
        if let Some(cell) = self
            .sections
            .iter()
            .find_map(|(c, m)| (m.id == id).then_some(*c))
        {
            let meta = self.sections.get_mut(&cell)?;
            return self
                .game
                .sections
                .get_mut(&(cell as u32))
                .map(|state| (meta, state));
        }
        self.detached_sections
            .get_mut(id)
            .map(|s| (&mut s.metadata, &mut s.state))
    }
    pub(crate) fn append_journal(&mut self, cell: usize) {
        self.journal.push(cell);
    }
    pub(crate) fn mutation_capacity(&self, count: u64) -> Result<(), WorldError> {
        self.capacity()?;
        if count > u64::from(u32::MAX - self.game.terrain_version)
            || [
                self.versions.infantry,
                self.versions.vehicle,
                self.versions.naval,
                self.versions.obstruction,
            ]
            .iter()
            .any(|v| v.checked_add(count).is_none())
        {
            return Err(WorldError::VersionExhausted);
        }
        Ok(())
    }
    pub fn initial(&self) -> &InitialTerrain {
        &self.initial
    }
    pub fn cell_log(&self) -> &[CellRow] {
        &self.log
    }
    pub fn new_cells(&self) -> &[CellRow] {
        &self.new_cells
    }
    pub fn take_new_cells(&mut self) -> Vec<CellRow> {
        std::mem::take(&mut self.new_cells)
    }
    pub fn journal(&self) -> &[usize] {
        &self.journal
    }
    pub fn take_journal(&mut self) -> Vec<usize> {
        std::mem::take(&mut self.journal)
    }
    pub fn soak(&self) -> &[i64] {
        &self.soak
    }
    pub fn take_soak(&mut self) -> Vec<i64> {
        self.soak_members.clear();
        std::mem::take(&mut self.soak)
    }
    /// Register internal producers/consumers only. This does not grant terrain visibility.
    pub fn register_recipient(&mut self, id: Recipient) {
        self.pending
            .entry(id)
            .or_insert_with(|| (0..self.log.len()).collect());
    }
    pub fn pending(&self, id: Recipient) -> Option<&BTreeSet<usize>> {
        self.pending.get(&id)
    }
    /// Acknowledge only indices actually consumed; unseen records must stay pending.
    pub fn acknowledge(&mut self, id: Recipient, indices: &[usize]) {
        if let Some(set) = self.pending.get_mut(&id) {
            for i in indices {
                set.remove(i);
            }
        }
    }
    pub fn unregister_recipient(&mut self, id: Recipient) {
        self.pending.remove(&id);
    }
    fn valid_cell(&self, c: usize) -> Result<(), WorldError> {
        if c < self.game.chars.len() {
            Ok(())
        } else {
            Err(WorldError::InvalidCell)
        }
    }
    fn capacity(&self) -> Result<(), WorldError> {
        if self.game.terrain_version == u32::MAX
            || [
                self.versions.infantry,
                self.versions.vehicle,
                self.versions.naval,
                self.versions.obstruction,
            ]
            .contains(&u64::MAX)
        {
            Err(WorldError::VersionExhausted)
        } else {
            Ok(())
        }
    }
    pub fn material_name(&self, c: usize, ground_only: bool) -> Option<&str> {
        let ground = *self.game.ground.as_ref()?.get(c)?;
        if !ground_only
            && let Some(section) = self.game.sections.get(&(c as u32))
            && !section.failed
            && let Some(meta) = self.sections.get(&c)
        {
            return Some(&meta.material);
        }
        if let Some(material) = (if ground_only {
            &self.base_materials
        } else {
            &self.materials
        })
        .get(&c)
        {
            return Some(material);
        }
        let object = if ground_only {
            b'.'
        } else {
            self.game.objects.as_ref()?[c]
        };
        Some(if b"QY".contains(&object) {
            "steel"
        } else if b"#BR".contains(&object) {
            "stone"
        } else if b"HK=TOXA".contains(&object) {
            "wood"
        } else if ground == b'D' {
            "road"
        } else if b"WF".contains(&ground) {
            "water"
        } else {
            "soil"
        })
    }
    pub fn max_hp(&self, c: usize) -> f64 {
        if let Some(section) = self.game.sections.get(&(c as u32))
            && !section.failed
            && let Some(max) = section.max_hp
        {
            return max;
        }
        let ch = self.game.physical(c);
        if ch == b'B' {
            self.rules
                .movement
                .house_hp
                .get(usize::from(self.game.house[c]))
                .copied()
                .unwrap_or(f64::NAN)
        } else {
            self.rules.movement.terrain_hp(ch).unwrap_or(0.0)
                * self.game.tough.as_ref().map_or(1.0, |t| t[c])
        }
    }
    fn state_of(&self, c: usize) -> u8 {
        let hp = f64::from(self.game.cell_hp[c]);
        let max = self.max_hp(c);
        let scar = f64::from(self.scar[c]);
        let wear = self.game.wear.as_ref().unwrap()[c];
        ((wear * 4.0).floor() as u8).min(3)
            | if self.burnt[c] != 0 { 4 } else { 0 }
            | if max > 1.0 && hp < max * 0.67 {
                if hp < max * 0.34 { 16 } else { 8 }
            } else {
                0
            }
            | if scar > 0.6 {
                96
            } else if scar > 0.3 {
                64
            } else if scar > 0.02 {
                32
            } else {
                0
            }
    }
    pub fn cell_row(&self, c: usize) -> Result<CellRow, WorldError> {
        self.valid_cell(c)?;
        let section = self.game.sections.get(&(c as u32)).and_then(|s| {
            self.sections.get(&c).map(|m| SectionRow {
                id: m.id.clone(),
                structure_id: m.structure_id.clone(),
                state: if s.failed {
                    "failed"
                } else {
                    m.appearance.name()
                },
                hp: math::max(0.0, math::round(s.hp * 10.0) / 10.0),
                max_hp: s.max_hp.unwrap_or(0.0),
                material: m.material.clone(),
                anchor: m.anchor,
            })
        });
        let object = self.game.objects.as_ref().unwrap()[c];
        Ok(CellRow {
            cell: c,
            ch: self.game.chars[c],
            height: self.game.height.as_ref().map_or(0, |h| h[c]),
            state: self.cell_state[c],
            data: CellData {
                ground: self.game.ground.as_ref().unwrap()[c],
                object,
                material: self.material_name(c, false).unwrap().into(),
                ground_material: self.material_name(c, true).unwrap().into(),
                ground_wear: if object == b'T' {
                    self.surface_wear[c]
                } else {
                    self.game.wear.as_ref().unwrap()[c]
                },
                trench_front: self
                    .game
                    .trench_front
                    .as_ref()
                    .map(|f| f[c])
                    .filter(|f| f.is_finite()),
                mine: self.mine_layer[c] != 0,
                mine_material: if self.mine_layer[c] != 0 {
                    self.mine_materials.get(&c).cloned()
                } else {
                    None
                },
                section,
            },
        })
    }
    /// Record the current cell. Look-only changes do not invalidate terrain geometry.
    pub fn log_cell(&mut self, c: usize, look: bool) -> Result<usize, WorldError> {
        self.valid_cell(c)?;
        if !look {
            self.capacity()?;
        }
        let row = self.cell_row(c)?;
        let index = if let Some(index) = self.log_indexes.get(&c) {
            *index
        } else {
            let i = self.log.len();
            self.log_indexes.insert(c, i);
            self.log.push(row.clone());
            i
        };
        self.log[index] = row.clone();
        for pending in self.pending.values_mut() {
            pending.insert(index);
        }
        if !look {
            self.game.terrain_version += 1;
        }
        self.new_cells.push(row);
        self.journal.push(c);
        Ok(index)
    }
    /// State changes journal the cell even when its packed state byte stays unchanged.
    pub fn touch(&mut self, c: usize) -> Result<Mutation, WorldError> {
        self.valid_cell(c)?;
        self.journal.push(c);
        let state = self.state_of(c);
        let logged_index = if state != self.cell_state[c] {
            self.cell_state[c] = state;
            Some(self.log_cell(c, true)?)
        } else {
            None
        };
        Ok(Mutation { logged_index })
    }
    /// Register a new base building after its footprint cells have been placed.
    /// Existing cell sections move to historical storage, retaining the old graph's values.
    pub fn register_building_structure(
        &mut self,
        unit_id: u32,
        cells: &[usize],
        total_hp: f64,
        total_max_hp: f64,
    ) -> Result<(), WorldError> {
        self.capacity()?;
        if cells.is_empty()
            || !total_hp.is_finite()
            || total_hp < 0.0
            || !total_max_hp.is_finite()
            || total_max_hp <= 0.0
        {
            return Err(WorldError::InvalidChange);
        }
        let id = format!("building:{unit_id}");
        if self.structures.iter().any(|s| s.id == id) {
            return Err(WorldError::InvalidChange);
        }
        let local: BTreeSet<_> = cells.iter().copied().collect();
        if local.len() != cells.len() || cells.iter().any(|c| *c >= self.game.chars.len()) {
            return Err(WorldError::InvalidCell);
        }
        if cells.len() > u32::MAX as usize
            || self
                .game
                .terrain_version
                .checked_add(cells.len() as u32)
                .is_none()
        {
            return Err(WorldError::VersionExhausted);
        }
        if cells.iter().any(|cell| {
            self.sections.contains_key(cell) != self.game.sections.contains_key(&(*cell as u32))
        }) {
            return Err(WorldError::InvalidTerrain);
        }
        let hp = total_hp / cells.len() as f64;
        let max_hp = total_max_hp / cells.len() as f64;
        let mut section_ids = Vec::with_capacity(cells.len());
        for &cell in cells {
            if let Some(metadata) = self.sections.remove(&cell) {
                let state = self
                    .game
                    .sections
                    .remove(&(cell as u32))
                    .ok_or(WorldError::InvalidTerrain)?;
                self.detached_sections.insert(
                    metadata.id.clone(),
                    DetachedSection {
                        cell,
                        metadata,
                        state,
                    },
                );
            }
            let section_id = format!("{id}:{cell}");
            section_ids.push(section_id.clone());
            let w = self.game.w;
            let neighbors = [
                cell.checked_sub(w),
                (cell + w < self.game.chars.len()).then_some(cell + w),
                cell.checked_sub(1).filter(|_| !cell.is_multiple_of(w)),
                (cell % w + 1 < w).then_some(cell + 1),
            ];
            let supports = neighbors
                .into_iter()
                .flatten()
                .filter(|c| local.contains(c))
                .map(|c| format!("{id}:{c}"))
                .collect();
            self.game.sections.insert(
                cell as u32,
                Section {
                    failed: false,
                    hp,
                    max_hp: Some(max_hp),
                },
            );
            self.sections.insert(
                cell,
                SectionMetadata {
                    id: section_id,
                    structure_id: id.clone(),
                    material: "wood".into(),
                    anchor: true,
                    supports,
                    rubble: Vec::new(),
                    failure: FailureMetadata::default(),
                    appearance: Appearance::Intact,
                },
            );
            self.game.cell_hp[cell] = hp as f32;
            self.log_cell(cell, false)?;
        }
        self.structures.push(WorldStructure {
            id,
            kind: "building",
            cells: cells.to_vec(),
            section_ids,
            entity_id: Some(unit_id),
        });
        Ok(())
    }
    pub fn set_level(&mut self, c: usize, height: i8) -> Result<Mutation, WorldError> {
        self.mutate_cell(
            c,
            CellChange {
                height: Some(height),
                ..CellChange::default()
            },
        )
    }
    pub fn set_cell(&mut self, c: usize, ch: u8) -> Result<Mutation, WorldError> {
        self.valid_cell(c)?;
        if ch == b'.' && self.mine_layer[c] != 0 {
            return self.mutate_cell(
                c,
                CellChange {
                    mine: Some(false),
                    ..CellChange::default()
                },
            );
        }
        if !b".D+MWFBH#TXY=RNAQKO".contains(&ch) {
            return Err(WorldError::InvalidChange);
        }
        if b".D+MWF".contains(&ch) {
            let clear = ch == b'.' && self.game.objects.as_ref().unwrap()[c] != b'.';
            self.mutate_cell(
                c,
                CellChange {
                    ground: if clear { None } else { Some(ch) },
                    object: Some(b'.'),
                    ..CellChange::default()
                },
            )
        } else {
            self.capacity()?;
            if ch == b'='
                && let Some(tough) = self.game.tough.as_mut()
            {
                tough[c] = 1.0;
            }
            self.mutate_cell(
                c,
                CellChange {
                    object: Some(ch),
                    ..CellChange::default()
                },
            )
        }
    }
    pub fn mutate_cell(&mut self, c: usize, change: CellChange) -> Result<Mutation, WorldError> {
        self.valid_cell(c)?;
        self.capacity()?;
        if change.ground.is_some_and(|c| !b".D+MWF".contains(&c))
            || change
                .object
                .is_some_and(|c| !b".BH#TXY=RNAQKO".contains(&c))
            || change.height.is_some_and(|h| !(-2..=4).contains(&h))
            || change.material.as_deref().is_some_and(|m| {
                ![
                    "soil", "road", "wood", "stone", "concrete", "steel", "water",
                ]
                .contains(&m)
            })
        {
            return Err(WorldError::InvalidChange);
        }
        let old = self.game.flags[c];
        let old_object = self.game.objects.as_ref().unwrap()[c];
        let old_ground = self.game.ground.as_ref().unwrap()[c];
        let old_mine = self.mine_layer[c] != 0;
        if old_object != b'T' {
            self.surface_wear[c] = self.game.wear.as_ref().unwrap()[c];
        }
        if let Some(ground) = change.ground
            && ground != old_ground
        {
            self.game.ground.as_mut().unwrap()[c] = ground;
            self.base_materials.remove(&c);
            self.surface_wear[c] = start_wear(ground, c);
        }
        if change.object == Some(b'N') {
            self.mine_layer[c] = 1;
        } else if let Some(object) = change.object {
            self.game.objects.as_mut().unwrap()[c] = object;
        }
        if let Some(mine) = change.mine {
            self.mine_layer[c] = u8::from(mine);
        }
        if old_mine && change.object == Some(b'.') && old_object == b'.' && change.mine.is_none() {
            self.mine_layer[c] = 0;
        }
        let object = self.game.objects.as_ref().unwrap()[c];
        let ground = self.game.ground.as_ref().unwrap()[c];
        let object_changed = object != old_object;
        let ground_changed = ground != old_ground;
        let mine_changed = old_mine != (self.mine_layer[c] != 0);
        if object_changed {
            self.object_materials.remove(&c);
        }
        if let Some(material) = &change.material {
            if change.object == Some(b'N') || change.mine == Some(true) && change.object.is_none() {
                self.mine_materials.insert(c, material.clone());
            } else if object != b'.' {
                self.object_materials.insert(c, material.clone());
            } else {
                self.base_materials.insert(c, material.clone());
            }
        }
        if let Some(material) = (if object == b'.' {
            &self.base_materials
        } else {
            &self.object_materials
        })
        .get(&c)
        {
            self.materials.insert(c, material.clone());
        } else {
            self.materials.remove(&c);
        }
        if let Some(height) = change.height {
            self.game
                .height
                .get_or_insert_with(|| vec![0; self.game.w * self.game.h])[c] = height;
        }
        let ch = if self.mine_layer[c] != 0 {
            b'N'
        } else if object != b'.' {
            object
        } else {
            ground
        };
        let next = flags(if object != b'.' { object } else { ground })
            | if self.shoal.as_ref().is_some_and(|s| s[c] != 0) {
                SHOAL
            } else {
                0
            };
        if old_mine && self.mine_layer[c] == 0 {
            self.mines.remove(&c);
            self.mine_seen.remove(&c);
            self.mine_materials.remove(&c);
        }
        if !old_mine && self.mine_layer[c] != 0 {
            self.mines.entry(c).or_insert(-1);
        }
        if old_object == b'A' && object != b'A' {
            self.aid.remove(&c);
        }
        if old_object == b'Q' && object != b'Q' {
            self.game.wrecks.retain(|w| w.cell != Some(c));
        }
        self.game.flags[c] = next;
        self.game.chars[c] = ch;
        if object_changed || ground_changed {
            self.game.cell_hp[c] = self
                .game
                .sections
                .get(&(c as u32))
                .filter(|s| !s.failed)
                .map_or_else(|| self.max_hp(c), |s| s.hp) as f32;
            if object_changed && let Some(front) = self.game.trench_front.as_mut() {
                front[c] = f32::NAN;
            }
            if object_changed || b"WF".contains(&ground) {
                self.fires.remove(&c);
            }
            if object == b'T' {
                if object_changed {
                    self.game.wear.as_mut().unwrap()[c] = 0.0;
                }
            } else {
                self.game.wear.as_mut().unwrap()[c] = self.surface_wear[c];
            }
        }
        self.cell_state[c] = self.state_of(c);
        if object_changed || ground_changed || change.height.is_some() {
            for cell in [
                c as i64,
                c as i64 - 1,
                c as i64 + 1,
                c as i64 - self.game.w as i64,
                c as i64 + self.game.w as i64,
            ] {
                if self.soak_members.insert(cell) {
                    self.soak.push(cell);
                }
            }
        }
        if object_changed {
            self.versions.obstruction += 1;
        }
        if change.height.is_some() || (old ^ next) & MOVE != 0 {
            self.versions.infantry += 1;
        }
        if change.height.is_some() || (old ^ next) & (MOVE | VBLOCK) != 0 {
            self.versions.vehicle += 1;
        }
        if change.height.is_some() || (old ^ next) & (LAND | SHOAL) != 0 {
            self.versions.naval += 1;
        }
        let logged_index = if object_changed
            || ground_changed
            || mine_changed
            || change.material.is_some()
            || change.height.is_some()
        {
            Some(self.log_cell(c, false)?)
        } else {
            None
        };
        Ok(Mutation { logged_index })
    }
}
