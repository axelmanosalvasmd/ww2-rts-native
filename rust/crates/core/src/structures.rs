//! Validated authored structures and initial house/bridge support graphs.
//!
//! Structure and section vectors preserve the reference's insertion order. Ordered indexes
//! provide lookup without making that order depend on a hash table. Damage, falling bodies,
//! repairs and collapse execution remain outside this module.
use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use crate::{
    game,
    map::{Layers, MapError, TerrainState},
};

pub const MAX_STRUCTURES: usize = 2048;
pub const MAX_SECTIONS: usize = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StructureKind {
    House,
    Bridge,
}
impl StructureKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::House => "house",
            Self::Bridge => "bridge",
        }
    }
    fn cell(self) -> u8 {
        match self {
            Self::House => b'B',
            Self::Bridge => b'=',
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct SectionDefinition {
    pub id: String,
    pub cell: usize,
    pub hp: f64,
    pub material: String,
    pub anchor: bool,
    pub supports: Vec<String>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct StructureDefinition {
    pub id: String,
    pub kind: StructureKind,
    pub sections: Vec<SectionDefinition>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SectionState {
    Intact,
    Damaged,
    Burning,
    Failed,
}
impl SectionState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Intact => "intact",
            Self::Damaged => "damaged",
            Self::Burning => "burning",
            Self::Failed => "failed",
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct StructuralSection {
    pub id: String,
    pub structure_id: String,
    pub cell: usize,
    pub hp: f64,
    pub max_hp: f64,
    pub material: String,
    pub anchor: bool,
    pub supports: Vec<String>,
    pub state: SectionState,
    pub rubble: Vec<usize>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Structure {
    pub id: String,
    pub kind: StructureKind,
    pub sections: Vec<StructuralSection>,
}
/// Initial graph storage, separate from the reduced section data read by existing queries.
#[derive(Clone, Debug)]
pub struct StructureState {
    structures: Vec<Structure>,
    by_id: BTreeMap<String, usize>,
    by_cell: BTreeMap<usize, (usize, usize)>,
}
impl StructureState {
    pub fn structures(&self) -> &[Structure] {
        &self.structures
    }
    pub fn structure(&self, id: &str) -> Option<&Structure> {
        self.by_id.get(id).map(|i| &self.structures[*i])
    }
    pub fn section_at(&self, cell: usize) -> Option<&StructuralSection> {
        self.by_cell
            .get(&cell)
            .map(|(s, c)| &self.structures[*s].sections[*c])
    }
    pub(crate) fn into_structures(self) -> Vec<Structure> {
        self.structures
    }
    fn add(&mut self, structure: Structure) {
        let index = self.structures.len();
        self.by_id.insert(structure.id.clone(), index);
        for (i, section) in structure.sections.iter().enumerate() {
            self.by_cell.insert(section.cell, (index, i));
        }
        self.structures.push(structure);
    }
}
/// Terrain after applying authored section health, with both full graph and query projections.
#[derive(Clone, Debug)]
pub struct StructuredTerrain {
    pub terrain: TerrainState,
    pub structures: StructureState,
}
impl StructuredTerrain {
    /// Material used by the current static section, its layer override or its ground.
    /// Ground-only queries ignore structural and object materials.
    pub fn material_name(&self, cell: usize, ground_only: bool) -> Option<&str> {
        let layers = &self.terrain.layers;
        let ground = *layers.ground.get(cell)?;
        if !ground_only
            && let Some(section) = self.structures.section_at(cell)
            && section.state != SectionState::Failed
        {
            return Some(&section.material);
        }
        let overrides = if ground_only {
            &layers.base_materials
        } else {
            &layers.materials
        };
        if let Some(material) = overrides.get(&cell) {
            return Some(material);
        }
        let object = if ground_only {
            b'.'
        } else {
            layers.objects[cell]
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
    /// Static query state only. No players, units, mode or destruction execution is created.
    pub fn query_game(&self) -> game::Game {
        let mut game = self.terrain.query_game();
        for structure in self.structures.structures() {
            for section in &structure.sections {
                game.sections.insert(
                    section.cell as u32,
                    game::Section {
                        failed: section.state == SectionState::Failed,
                        hp: section.hp,
                        max_hp: Some(section.max_hp),
                    },
                );
            }
        }
        game
    }
}

struct SupportNode<'a> {
    id: &'a str,
    hp: f64,
    failed: bool,
    anchor: bool,
    supports: &'a [String],
}
fn supported_order(nodes: &[SupportNode<'_>]) -> Vec<usize> {
    let live: BTreeMap<_, _> = nodes
        .iter()
        .enumerate()
        .filter(|(_, s)| !s.failed && s.hp > 0.0)
        .map(|(i, s)| (s.id, i))
        .collect();
    let mut links = vec![Vec::new(); nodes.len()];
    for (i, node) in nodes
        .iter()
        .enumerate()
        .filter(|(_, s)| live.contains_key(s.id))
    {
        for id in node.supports {
            if let Some(&j) = live.get(id.as_str()) {
                if !links[i].contains(&j) {
                    links[i].push(j);
                }
                if !links[j].contains(&i) {
                    links[j].push(i);
                }
            }
        }
    }
    let mut seen = vec![false; nodes.len()];
    let mut queue = Vec::new();
    for (i, node) in nodes.iter().enumerate() {
        if live.contains_key(node.id) && node.anchor {
            seen[i] = true;
            queue.push(i);
        }
    }
    let mut i = 0;
    while i < queue.len() {
        for &next in &links[queue[i]] {
            if !seen[next] {
                seen[next] = true;
                queue.push(next);
            }
        }
        i += 1;
    }
    queue
}
/// Live sections connected to a live anchor, in the reference's breadth-first insertion order.
/// Support references are undirected. A failed or zero-health section cannot carry support.
pub fn supported_sections(structure: &Structure) -> Vec<&str> {
    let nodes: Vec<_> = structure
        .sections
        .iter()
        .map(|s| SupportNode {
            id: &s.id,
            hp: s.hp,
            failed: s.state == SectionState::Failed,
            anchor: s.anchor,
            supports: &s.supports,
        })
        .collect();
    supported_order(&nodes)
        .into_iter()
        .map(|i| structure.sections[i].id.as_str())
        .collect()
}
fn invalid(reason: &str) -> MapError {
    MapError::Invalid {
        field: "structures".into(),
        reason: reason.into(),
    }
}
fn identifier(v: &Value) -> Result<String, MapError> {
    v.as_str()
        .filter(|s| {
            !s.is_empty()
                && s.len() <= 48
                && s.bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c))
        })
        .map(str::to_owned)
        .ok_or_else(|| invalid("ids must be short ASCII identifiers"))
}
fn physical(layers: &Layers, c: usize) -> u8 {
    if layers.objects[c] == b'.' {
        layers.ground[c]
    } else {
        layers.objects[c]
    }
}
fn neighbors(c: usize, w: usize, n: usize) -> impl Iterator<Item = usize> {
    [
        c.checked_sub(w),
        (c + w < n).then_some(c + w),
        c.checked_sub(1).filter(|_| !c.is_multiple_of(w)),
        (c % w + 1 < w).then_some(c + 1),
    ]
    .into_iter()
    .flatten()
}

pub(crate) fn parse(
    value: Option<&Value>,
    w: usize,
    h: usize,
    layers: &Layers,
) -> Result<Vec<StructureDefinition>, MapError> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let list = value
        .as_array()
        .filter(|a| a.len() <= MAX_STRUCTURES)
        .ok_or_else(|| invalid("structures must be a bounded list"))?;
    let mut ids = BTreeSet::new();
    let mut cells = BTreeSet::new();
    let mut result = Vec::with_capacity(list.len());
    for value in list {
        let id = identifier(&value["id"])?;
        if !ids.insert(id.clone()) {
            return Err(invalid("structure ids must be unique"));
        }
        let kind = match value["kind"].as_str() {
            Some("house") => StructureKind::House,
            Some("bridge") => StructureKind::Bridge,
            _ => return Err(invalid("unknown structure kind")),
        };
        let input = value["sections"]
            .as_array()
            .filter(|s| !s.is_empty() && s.len() <= MAX_SECTIONS)
            .ok_or_else(|| invalid("a structure needs 1-128 sections"))?;
        let mut local = BTreeMap::new();
        let mut sections = Vec::with_capacity(input.len());
        for value in input {
            let id = identifier(&value["id"])?;
            if local.contains_key(&id) {
                return Err(invalid("section ids must be unique in their structure"));
            }
            let cell = value["c"]
                .as_f64()
                .filter(|n| n.fract() == 0.0 && *n >= 0.0 && *n < (w * h) as f64)
                .ok_or_else(|| invalid("section cell must be inside the map"))?
                as usize;
            if !cells.insert(cell) {
                return Err(invalid("section cells must be unique"));
            }
            if physical(layers, cell) != kind.cell() {
                return Err(invalid("section footprint must match its structure"));
            }
            let hp = value["hp"]
                .as_f64()
                .filter(|hp| hp.is_finite() && *hp > 0.0 && *hp <= 100_000.0)
                .ok_or_else(|| invalid("section health must be positive and at most 100000"))?;
            let material = value["material"]
                .as_str()
                .filter(|m| {
                    [
                        "soil", "road", "wood", "stone", "concrete", "steel", "water",
                    ]
                    .contains(m)
                })
                .ok_or_else(|| invalid("unknown section material"))?
                .to_owned();
            let anchor = value["anchor"]
                .as_bool()
                .ok_or_else(|| invalid("section anchor must be a boolean"))?;
            let input_supports = value["supports"]
                .as_array()
                .filter(|a| a.len() <= 8)
                .ok_or_else(|| invalid("supports must be a list of at most eight references"))?;
            let mut supports = Vec::with_capacity(input_supports.len());
            for input in input_supports {
                let id = input
                    .as_str()
                    .ok_or_else(|| invalid("support must reference a section id"))?
                    .to_owned();
                if supports.contains(&id) {
                    return Err(invalid("support references must be unique"));
                }
                supports.push(id);
            }
            local.insert(id.clone(), sections.len());
            sections.push(SectionDefinition {
                id,
                cell,
                hp,
                material,
                anchor,
                supports,
            });
        }
        for section in &sections {
            for id in &section.supports {
                let to = local.get(id).map(|i| &sections[*i]).ok_or_else(|| {
                    invalid("support must reference a section in the same structure")
                })?;
                let dx = (section.cell % w).abs_diff(to.cell % w);
                let dy = (section.cell / w).abs_diff(to.cell / w);
                if id == &section.id || dx + dy != 1 {
                    return Err(invalid("supports must reference neighboring sections"));
                }
            }
        }
        let nodes: Vec<_> = sections
            .iter()
            .map(|s| SupportNode {
                id: &s.id,
                hp: s.hp,
                failed: false,
                anchor: s.anchor,
                supports: &s.supports,
            })
            .collect();
        if supported_order(&nodes).len() != sections.len() {
            return Err(invalid("every starting section needs a path to an anchor"));
        }
        if kind == StructureKind::Bridge {
            for section in sections.iter().filter(|s| s.anchor) {
                if !neighbors(section.cell, w, w * h).any(|c| !b"W=".contains(&physical(layers, c)))
                {
                    return Err(invalid("bridge anchors must attach to a bank"));
                }
            }
        }
        result.push(StructureDefinition { id, kind, sections });
    }
    Ok(result)
}

pub(crate) fn initialize(
    authored: &[StructureDefinition],
    terrain: &mut TerrainState,
) -> StructureState {
    let n = terrain.w * terrain.h;
    let mut state = StructureState {
        structures: Vec::new(),
        by_id: BTreeMap::new(),
        by_cell: BTreeMap::new(),
    };
    let mut used = vec![false; n];
    for input in authored {
        let id = format!("map:{}", input.id);
        let sections = input
            .sections
            .iter()
            .map(|s| {
                used[s.cell] = true;
                terrain.cell_hp[s.cell] = s.hp as f32;
                StructuralSection {
                    id: format!("{id}:{}", s.id),
                    structure_id: id.clone(),
                    cell: s.cell,
                    hp: s.hp,
                    max_hp: s.hp,
                    material: s.material.clone(),
                    anchor: s.anchor,
                    supports: s.supports.iter().map(|s| format!("{id}:{s}")).collect(),
                    state: SectionState::Intact,
                    rubble: Vec::new(),
                }
            })
            .collect();
        state.add(Structure {
            id,
            kind: input.kind,
            sections,
        });
    }
    let mut visited = vec![false; n];
    for c in 0..n {
        let ch = physical(&terrain.layers, c);
        if used[c] || !b"B=".contains(&ch) {
            continue;
        }
        let mut cells = vec![c];
        visited[c] = true;
        let mut i = 0;
        while i < cells.len() {
            for next in neighbors(cells[i], terrain.w, n) {
                if !visited[next] && physical(&terrain.layers, next) == ch {
                    visited[next] = true;
                    cells.push(next);
                }
            }
            i += 1;
        }
        cells.retain(|c| !used[*c]);
        for part in cells.chunks(MAX_SECTIONS) {
            let kind = if ch == b'=' {
                StructureKind::Bridge
            } else {
                StructureKind::House
            };
            let id = format!("map:{}:{}", kind.as_str(), part[0]);
            let local: BTreeSet<_> = part.iter().copied().collect();
            let sections = part
                .iter()
                .map(|&cell| {
                    used[cell] = true;
                    let hp = f64::from(terrain.cell_hp[cell]);
                    let material = terrain
                        .layers
                        .object_materials
                        .get(&cell)
                        .cloned()
                        .unwrap_or_else(|| {
                            if (kind == StructureKind::Bridge
                                && terrain.tough.as_ref().is_some_and(|t| t[cell] > 1.0))
                                || (kind == StructureKind::House && terrain.house[cell] >= 2)
                            {
                                "stone"
                            } else {
                                "wood"
                            }
                            .into()
                        });
                    let anchor = kind == StructureKind::House
                        || neighbors(cell, terrain.w, n)
                            .any(|c| !b"W=".contains(&physical(&terrain.layers, c)));
                    let supports = neighbors(cell, terrain.w, n)
                        .filter(|c| local.contains(c))
                        .map(|c| format!("{id}:{c}"))
                        .collect();
                    StructuralSection {
                        id: format!("{id}:{cell}"),
                        structure_id: id.clone(),
                        cell,
                        hp,
                        max_hp: hp,
                        material,
                        anchor,
                        supports,
                        state: SectionState::Intact,
                        rubble: Vec::new(),
                    }
                })
                .collect();
            state.add(Structure { id, kind, sections });
        }
    }
    state
}
