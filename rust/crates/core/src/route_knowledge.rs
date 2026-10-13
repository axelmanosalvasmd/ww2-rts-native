//! Recipient terrain composed from explicit authored data and ordered remembered rows.
//! No authoritative game, visibility collector or wire decoder is consulted here.
use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::game::{COVER, MOVE, Point, SHOAL, SIGHT, TRENCH, Wreck};
use crate::math;
use crate::navigation::{TerrainChange, TerrainView, Versions};
use crate::pathfinding::PathTerrain;
use crate::rules::Rules;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Material {
    Soil,
    Road,
    Wood,
    Stone,
    Concrete,
    Steel,
    Water,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RememberedSection {
    pub hp: f64,
    pub max_hp: Option<f64>,
    pub failed: bool,
    pub material: Option<Material>,
}
/// Producer-supplied public or remembered inputs that the reference inherits
/// from its world object. Absence in World mode means unknown, not toughness 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CoverBasis {
    pub house_kind: Option<u8>,
    pub toughness: f64,
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CellMetadata {
    pub ground: Option<u8>,
    pub object: Option<u8>,
    pub material: Option<Material>,
    pub section: Option<RememberedSection>,
    pub trench_front: Option<f64>,
    pub mine_owned: bool,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RememberedCell {
    pub cell: usize,
    /// Identity of this cell's row object, stable until the producer replaces it.
    /// Same revision skips the row even if its other fields differ.
    pub revision: u64,
    pub character: u8,
    pub height: i8,
    pub state: u8,
    pub metadata: CellMetadata,
    pub cover_basis: Option<CoverBasis>,
}
#[derive(Clone, Copy)]
pub struct InitialTerrain<'a> {
    pub width: usize,
    pub height: usize,
    pub chars: &'a [u8],
    pub ground: Option<&'a [u8]>,
    pub objects: Option<&'a [u8]>,
    pub heights: Option<&'a [i8]>,
    pub shoals: Option<&'a [bool]>,
    pub house_kinds: Option<&'a [u8]>,
    /// None explicitly means this public authored map has no toughness override.
    pub toughness: Option<&'a [f64]>,
    pub trench_front: Option<&'a [f32]>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KnowledgeMode {
    Observed,
    World,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KnowledgeError {
    InvalidShape,
    InvalidCell,
    InvalidValue,
    VersionRegression,
    UnknownCell,
    MissingCoverBasis,
}
#[derive(Clone, Debug)]
struct Change {
    from: u64,
    to: u64,
    cells: Vec<usize>,
}

#[derive(Clone)]
#[cfg_attr(test, derive(Debug))]
pub struct RouteKnowledge {
    width: usize,
    height: usize,
    mode: KnowledgeMode,
    identity: u64,
    rules: Rules,
    known: Vec<bool>,
    revisions: Vec<Option<u64>>,
    chars: Vec<u8>,
    ground: Vec<u8>,
    objects: Vec<u8>,
    flags: Vec<u16>,
    heights: Vec<i8>,
    wear: Vec<f32>,
    shoals: Vec<bool>,
    near_walls: Vec<u8>,
    mines: Vec<Option<bool>>,
    known_mines: Vec<bool>,
    materials: Vec<Option<Material>>,
    sections: Vec<Option<RememberedSection>>,
    basis: Vec<Option<CoverBasis>>,
    cell_hp: Vec<Option<f32>>,
    trench_front: Vec<f32>,
    version: u64,
    memory_version: u64,
    world_bound: bool,
    history: VecDeque<Change>,
    direct: Option<Change>,
}
/// Live values must already be filtered to this recipient. This module does not
/// decide visibility, ownership policy, weather publicity or road publicity.
#[derive(Clone, Copy)]
pub struct LiveRouting<'a> {
    pub fires: &'a [bool],
    pub bunkers: &'a [Point],
    pub wetness: f64,
    pub roads: bool,
}
pub struct RouteQuery<'a> {
    owner: &'a RouteKnowledge,
    history: Vec<TerrainChange<'a>>,
}
#[derive(Clone, Copy, Debug)]
pub struct KnownCell {
    pub known: bool,
    pub character: u8,
    pub ground: u8,
    pub object: u8,
    pub flags: u16,
    pub height: i8,
    pub wear: f32,
    pub near_wall: u8,
    pub mine_owned: Option<bool>,
    pub known_mine: bool,
    pub cell_hp: Option<f32>,
    pub trench_front: f32,
    pub material: Material,
    pub ground_material: Material,
}
impl RouteKnowledge {
    pub fn world(
        width: usize,
        height: usize,
        identity: u64,
        rules: Rules,
    ) -> Result<Self, KnowledgeError> {
        Self::empty(width, height, identity, rules, KnowledgeMode::World)
    }
    fn empty(
        width: usize,
        height: usize,
        identity: u64,
        rules: Rules,
        mode: KnowledgeMode,
    ) -> Result<Self, KnowledgeError> {
        let n = width
            .checked_mul(height)
            .filter(|n| *n > 0 && *n <= i32::MAX as usize)
            .ok_or(KnowledgeError::InvalidShape)?;
        Ok(Self {
            width,
            height,
            mode,
            identity,
            rules,
            known: vec![mode == KnowledgeMode::Observed; n],
            revisions: vec![None; n],
            chars: vec![b'.'; n],
            ground: vec![b'.'; n],
            objects: vec![b'.'; n],
            flags: vec![0; n],
            heights: vec![0; n],
            wear: vec![0.0; n],
            shoals: vec![false; n],
            near_walls: vec![0; n],
            mines: vec![None; n],
            known_mines: vec![false; n],
            materials: vec![None; n],
            sections: vec![None; n],
            basis: vec![None; n],
            cell_hp: vec![Some(0.0); n],
            trench_front: vec![f32::NAN; n],
            version: 0,
            memory_version: 0,
            world_bound: false,
            history: VecDeque::new(),
            direct: None,
        })
    }
    pub fn observed(
        initial: InitialTerrain<'_>,
        identity: u64,
        rules: Rules,
    ) -> Result<Self, KnowledgeError> {
        let mut out = Self::empty(
            initial.width,
            initial.height,
            identity,
            rules,
            KnowledgeMode::Observed,
        )?;
        let n = out.flags.len();
        if initial.chars.len() != n
            || initial.ground.is_some_and(|v| v.len() != n)
            || initial.objects.is_some_and(|v| v.len() != n)
            || initial.heights.is_some_and(|v| v.len() != n)
            || initial.shoals.is_some_and(|v| v.len() != n)
            || initial.house_kinds.is_some_and(|v| v.len() != n)
            || initial.toughness.is_some_and(|v| v.len() != n)
            || initial.trench_front.is_some_and(|v| v.len() != n)
        {
            return Err(KnowledgeError::InvalidShape);
        }
        for c in 0..n {
            let raw = initial.chars[c];
            if !valid_char(raw) {
                return Err(KnowledgeError::InvalidValue);
            }
            let ch = if raw == b'N' {
                compose(
                    *initial
                        .ground
                        .and_then(|v| v.get(c))
                        .ok_or(KnowledgeError::InvalidValue)?,
                    *initial
                        .objects
                        .and_then(|v| v.get(c))
                        .ok_or(KnowledgeError::InvalidValue)?,
                )
            } else {
                raw
            };
            let ground = initial.ground.map_or(ch, |v| v[c]);
            let object = initial.objects.map_or(b'.', |v| v[c]);
            if !valid_char(ground) || !valid_char(object) {
                return Err(KnowledgeError::InvalidValue);
            }
            let toughness = initial.toughness.map_or(1.0, |v| v[c]);
            if !toughness.is_finite() || toughness < 0.0 {
                return Err(KnowledgeError::InvalidValue);
            }
            out.chars[c] = ch;
            out.ground[c] = ground;
            out.objects[c] = object;
            out.shoals[c] = initial.shoals.is_some_and(|v| v[c]);
            out.flags[c] = terrain_flags(ch) | if out.shoals[c] { SHOAL } else { 0 };
            out.heights[c] = initial.heights.map_or(0, |v| v[c]);
            out.wear[c] = start_wear(ch, c) as f32;
            out.basis[c] = Some(CoverBasis {
                house_kind: initial.house_kinds.map(|v| v[c]),
                toughness,
            });
            out.trench_front[c] = initial.trench_front.map_or(f32::NAN, |v| v[c]);
            out.cell_hp[c] = out.max_hp(c).map(|v| v as f32);
        }
        out.update_walls(&(0..n).collect::<Vec<_>>());
        Ok(out)
    }
    /// Rows must be supplied in memory/log iteration order. This is a transform,
    /// not a collector: absent rows never remove or reveal remembered cells.
    pub fn apply_rows(
        &mut self,
        memory_version: u64,
        rows: &[RememberedCell],
    ) -> Result<(), KnowledgeError> {
        if self.mode == KnowledgeMode::World && memory_version < self.memory_version {
            return Err(KnowledgeError::VersionRegression);
        }
        if self.mode == KnowledgeMode::World
            && self.world_bound
            && memory_version == self.memory_version
        {
            return Ok(());
        }
        let mut accepted_revisions = BTreeMap::new();
        for row in rows {
            if row.cell >= self.flags.len() {
                return Err(KnowledgeError::InvalidCell);
            }
            if accepted_revisions
                .get(&row.cell)
                .copied()
                .or(self.revisions[row.cell])
                == Some(row.revision)
            {
                continue;
            }
            if !valid_char(row.character)
                || row.metadata.ground.is_some_and(|v| !valid_char(v))
                || row.metadata.object.is_some_and(|v| !valid_char(v))
                || row.metadata.trench_front.is_some_and(|v| !v.is_finite())
                || row.metadata.section.is_some_and(|v| {
                    !v.hp.is_finite()
                        || v.hp < 0.0
                        || v.max_hp.is_some_and(|x| !x.is_finite() || x < 0.0)
                })
                || row
                    .cover_basis
                    .is_some_and(|v| !v.toughness.is_finite() || v.toughness < 0.0)
            {
                return Err(KnowledgeError::InvalidValue);
            }
            accepted_revisions.insert(row.cell, row.revision);
        }
        let previous = self.version;
        if self.mode == KnowledgeMode::World
            && let Some(direct) = &mut self.direct
        {
            direct.from = previous;
            direct.to = previous;
        }
        let mut changed = Vec::new();
        let mut walls_changed = Vec::new();
        for row in rows {
            let c = row.cell;
            if self.revisions[c] == Some(row.revision) {
                continue;
            }
            let old_flags = self.flags[c];
            let old_height = self.heights[c];
            let old_wall = is_wall(old_flags);
            self.revisions[c] = Some(row.revision);
            self.known[c] = true;
            self.chars[c] = row.character;
            if self.mode == KnowledgeMode::World {
                self.ground[c] = row.metadata.ground.unwrap_or(row.character);
                self.objects[c] = row.metadata.object.unwrap_or(b'.');
            }
            let physical = if row.character == b'N' {
                compose(
                    row.metadata.ground.unwrap_or(self.ground[c]),
                    row.metadata.object.unwrap_or(self.objects[c]),
                )
            } else {
                row.character
            };
            self.flags[c] = terrain_flags(physical)
                | if self.mode == KnowledgeMode::Observed && self.shoals[c] {
                    SHOAL
                } else {
                    0
                };
            self.heights[c] = row.height;
            self.wear[c] = f32::from(row.state & 3) / 4.0;
            if self.mode == KnowledgeMode::Observed {
                if let Some(v) = row.metadata.ground {
                    self.ground[c] = v;
                }
                if let Some(v) = row.metadata.object {
                    self.objects[c] = v;
                }
            }
            if let Some(material) = row.metadata.material {
                self.materials[c] = Some(material);
            }
            self.sections[c] = row.metadata.section;
            if let Some(basis) = row.cover_basis {
                self.basis[c] = Some(basis);
            }
            let hp = row.metadata.section.map(|v| v.hp).or_else(|| {
                self.max_hp(c).map(|max| {
                    max * if row.state & 16 != 0 {
                        0.17
                    } else if row.state & 8 != 0 {
                        0.5
                    } else {
                        1.0
                    }
                })
            });
            self.cell_hp[c] = hp.map(|v| v as f32);
            self.trench_front[c] = row.metadata.trench_front.map_or(f32::NAN, |v| v as f32);
            self.mines[c] = (row.character == b'N').then_some(row.metadata.mine_owned);
            self.known_mines[c] = row.character == b'N' && !row.metadata.mine_owned;
            if self.mode == KnowledgeMode::Observed {
                self.version += 1;
                changed.push(c);
                walls_changed.push(c);
            } else {
                if old_flags != self.flags[c] || old_height != self.heights[c] {
                    changed.push(c);
                }
                if old_wall != is_wall(self.flags[c]) {
                    walls_changed.push(c);
                }
            }
        }
        self.update_walls(&walls_changed);
        if self.mode == KnowledgeMode::World {
            if !changed.is_empty() || !self.world_bound {
                self.version = memory_version;
                self.world_bound = true;
            }
            self.memory_version = memory_version;
        }
        if self.version != previous
            || (self.mode == KnowledgeMode::World && self.history.is_empty() && self.world_bound)
        {
            self.direct = Some(Change {
                from: previous,
                to: self.version,
                cells: changed.clone(),
            });
            self.history.push_back(Change {
                from: previous,
                to: self.version,
                cells: changed,
            });
            if self.history.len() > 32 {
                self.history.pop_front();
            }
        }
        Ok(())
    }
    fn update_walls(&mut self, changed: &[usize]) {
        let mut seen = BTreeSet::new();
        let mut affected = Vec::new();
        for &c in changed {
            let x = c % self.width;
            let y = c / self.width;
            for ny in y.saturating_sub(1)..=(y + 1).min(self.height - 1) {
                for nx in x.saturating_sub(1)..=(x + 1).min(self.width - 1) {
                    let n = ny * self.width + nx;
                    if seen.insert(n) {
                        affected.push(n);
                    }
                }
            }
        }
        for c in affected {
            let x = c % self.width;
            let y = c / self.width;
            self.near_walls[c] = u8::from(
                (y.saturating_sub(1)..=(y + 1).min(self.height - 1)).any(|ny| {
                    (x.saturating_sub(1)..=(x + 1).min(self.width - 1))
                        .any(|nx| is_wall(self.flags[ny * self.width + nx]))
                }),
            );
        }
    }
    fn physical(&self, c: usize) -> u8 {
        compose(self.ground[c], self.objects[c])
    }
    fn max_hp(&self, c: usize) -> Option<f64> {
        if let Some(s) = self.sections[c].filter(|s| !s.failed)
            && s.max_hp.is_some()
        {
            return s.max_hp;
        }
        let ch = self.physical(c);
        if ch == b'B' {
            self.basis[c]
                .and_then(|b| b.house_kind)
                .and_then(|kind| self.rules.movement.house_hp.get(usize::from(kind)).copied())
        } else {
            let max = self.rules.movement.terrain_hp(ch).unwrap_or(0.0);
            if max == 0.0 {
                Some(0.0)
            } else {
                self.basis[c].map(|b| max * b.toughness)
            }
        }
    }
    pub fn cell(&self, c: usize) -> Result<KnownCell, KnowledgeError> {
        if c >= self.flags.len() {
            return Err(KnowledgeError::InvalidCell);
        }
        Ok(KnownCell {
            known: self.known[c],
            character: self.chars[c],
            ground: self.ground[c],
            object: self.objects[c],
            flags: self.flags[c],
            height: self.heights[c],
            wear: self.wear[c],
            near_wall: self.near_walls[c],
            mine_owned: self.mines[c],
            known_mine: self.known_mines[c],
            cell_hp: self.cell_hp[c],
            trench_front: self.trench_front[c],
            material: self.material(c),
            ground_material: default_material(self.ground[c], b'.'),
        })
    }
    fn material(&self, c: usize) -> Material {
        self.sections[c]
            .filter(|s| !s.failed)
            .and_then(|s| s.material)
            .or(self.materials[c])
            .unwrap_or_else(|| default_material(self.ground[c], self.objects[c]))
    }
    pub fn query(&self) -> RouteQuery<'_> {
        RouteQuery {
            owner: self,
            history: self
                .history
                .iter()
                .map(|c| TerrainChange {
                    from: c.from,
                    to: c.to,
                    cells: &c.cells,
                })
                .collect(),
        }
    }
    pub fn cover_q(&self, c: usize) -> Result<f64, KnowledgeError> {
        if c >= self.flags.len() {
            return Err(KnowledgeError::InvalidCell);
        }
        if !self.known[c] {
            return Err(KnowledgeError::UnknownCell);
        }
        let ch = self.physical(c);
        if ch == b'O' {
            return Ok(self.rules.movement.wood_cover);
        }
        if ch == b'+' {
            return Ok(math::min(1.4, 0.55 + f64::from(self.wear[c])));
        }
        let max = self.max_hp(c).ok_or(KnowledgeError::MissingCoverBasis)?;
        if max > 1.0 {
            Ok(0.5
                + 0.5
                    * math::max(
                        0.0,
                        f64::from(self.cell_hp[c].ok_or(KnowledgeError::MissingCoverBasis)?),
                    )
                    / max)
        } else {
            Ok(1.0)
        }
    }
    pub fn trench_faces(&self, at: Point, from: Option<Point>) -> bool {
        let Some(from) = from else {
            return true;
        };
        let Some(c) = self.cell_of(at) else {
            return true;
        };
        let front = f64::from(self.trench_front[c]);
        front.is_nan()
            || math::cos(math::atan2(from.z - at.z, from.x - at.x) - front)
                > self.rules.movement.trench_arc
    }
    /// Static terrain and explicitly supplied settled wrecks. As in coverRank,
    /// moving units are excluded so a chosen cover spot survives their departure.
    pub fn cover_behind(
        &self,
        at: Point,
        from: Point,
        wrecks: &[Wreck],
    ) -> Result<f64, KnowledgeError> {
        let angle = math::atan2(from.z - at.z, from.x - at.x);
        let ca = math::cos(angle);
        let sa = math::sin(angle);
        let mut best = 0.0;
        let mut d = 0.8;
        while d < 3.7 {
            if let Some(c) = self.cell_of(Point {
                x: at.x + ca * d,
                z: at.z + sa * d,
            }) && matches!(self.physical(c), b'B' | b'#' | b'R' | b'H' | b'K' | b'Q')
            {
                best = math::min(1.0, (3.8 - d) / 1.6) * math::min(1.0, self.cover_q(c)?);
                break;
            }
            d += 0.7;
        }
        let x = (at.x / 2.0).floor() as isize;
        let y = (at.z / 2.0).floor() as isize;
        for (dx, dy) in [
            (1, 0),
            (-1, 0),
            (0, 1),
            (0, -1),
            (1, 1),
            (1, -1),
            (-1, 1),
            (-1, -1),
        ] {
            let nx = x + dx;
            let ny = y + dy;
            if nx < 0 || ny < 0 || nx >= self.width as isize || ny >= self.height as isize {
                continue;
            }
            if matches!(
                self.physical(ny as usize * self.width + nx as usize),
                b'B' | b'K'
            ) && (dx as f64 * ca + dy as f64 * sa) / math::hypot(dx as f64, dy as f64) > 0.5
            {
                best = 1.0;
            }
        }
        for wreck in wrecks.iter().filter(|w| !w.moving) {
            let dx = wreck.x - at.x;
            let dz = wreck.z - at.z;
            let d = math::hypot(dx, dz);
            if d > 0.0 && d < 4.0 && (dx * ca + dz * sa) / d > 0.7 {
                best = math::max(best, math::min(1.0, (4.0 - d) / 1.5));
            }
        }
        Ok(best)
    }
    pub fn cover_rank(
        &self,
        at: Point,
        from: Option<Point>,
        wrecks: &[Wreck],
    ) -> Result<u8, KnowledgeError> {
        let c = self.cell_of(at).ok_or(KnowledgeError::InvalidCell)?;
        if !self.known[c] {
            return Err(KnowledgeError::UnknownCell);
        }
        let flags = self.flags[c];
        Ok(if flags & TRENCH != 0 && self.trench_faces(at, from) {
            0
        } else if flags & COVER != 0 {
            1
        } else if let Some(from) = from
            && self.cover_behind(at, from, wrecks)? > 0.4
        {
            2
        } else {
            3
        })
    }
    fn cell_of(&self, at: Point) -> Option<usize> {
        if !at.x.is_finite() || !at.z.is_finite() {
            return None;
        }
        let x = (at.x / 2.0).floor();
        let y = (at.z / 2.0).floor();
        (x >= 0.0 && y >= 0.0 && x < self.width as f64 && y < self.height as f64)
            .then(|| y as usize * self.width + x as usize)
    }
}
impl RouteQuery<'_> {
    pub fn navigation(&self) -> TerrainView<'_> {
        let o = self.owner;
        TerrainView {
            width: o.width,
            height: o.height,
            flags: &o.flags,
            heights: Some(&o.heights),
            world_known: o.mode == KnowledgeMode::World,
            knowledge_id: o.identity,
            versions: Versions {
                infantry: o.version,
                vehicle: o.version,
                naval: o.version,
                obstruction: o.version,
                knowledge: o.version,
            },
            changed: o.direct.as_ref().map(|c| TerrainChange {
                from: c.from,
                to: c.to,
                cells: &c.cells,
            }),
            changes: &self.history,
        }
    }
    pub fn path_terrain<'a>(
        &'a self,
        live: LiveRouting<'a>,
    ) -> Result<PathTerrain<'a>, KnowledgeError> {
        if live.fires.len() != self.owner.flags.len() {
            return Err(KnowledgeError::InvalidShape);
        }
        Ok(PathTerrain {
            navigation: self.navigation(),
            near_walls: &self.owner.near_walls,
            known_mines: &self.owner.known_mines,
            fires: live.fires,
            bunkers: live.bunkers,
            wear: Some(&self.owner.wear),
            wetness: live.wetness,
            roads: live.roads,
        })
    }
}
fn compose(ground: u8, object: u8) -> u8 {
    if object != b'.' { object } else { ground }
}
fn valid_char(ch: u8) -> bool {
    b".BH#+TWFRKXYDMNOAQ=".contains(&ch)
}
fn is_wall(flags: u16) -> bool {
    flags & (MOVE | SIGHT) == (MOVE | SIGHT)
}
fn terrain_flags(ch: u8) -> u16 {
    use crate::game::{FORD, LAND, MUD, ROAD, VBLOCK, WIRE, WOOD};
    let flags = match ch {
        b'B' | b'K' => MOVE | SIGHT,
        b'H' => SIGHT | COVER,
        b'#' | b'+' => COVER,
        b'T' => COVER | TRENCH,
        b'W' => MOVE,
        b'F' => FORD,
        b'=' | b'D' => ROAD,
        b'R' | b'Y' | b'Q' => VBLOCK | COVER,
        b'X' => WIRE,
        b'M' => MUD,
        b'O' => COVER | WOOD,
        _ => 0,
    };
    flags | if ch != b'W' && ch != b'F' { LAND } else { 0 }
}
fn default_material(ground: u8, object: u8) -> Material {
    if b"QY".contains(&object) {
        Material::Steel
    } else if b"#BR".contains(&object) {
        Material::Stone
    } else if b"HK=TOXA".contains(&object) {
        Material::Wood
    } else if ground == b'D' {
        Material::Road
    } else if b"WF".contains(&ground) {
        Material::Water
    } else {
        Material::Soil
    }
}
fn start_wear(ch: u8, c: usize) -> f64 {
    let mut h = (c as u32).wrapping_add(1).wrapping_mul(0x9e3779b1);
    h ^= h >> 15;
    h = h.wrapping_mul(0x85ebca6b);
    let noise = f64::from(h ^ (h >> 13)) / 4294967296.0;
    if ch == b'M' || ch == b'F' {
        0.2 + 0.6 * noise
    } else if ch == b'+' {
        0.3 + 0.3 * noise
    } else {
        0.0
    }
}
