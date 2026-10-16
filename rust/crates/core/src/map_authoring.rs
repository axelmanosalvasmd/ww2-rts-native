//! In-memory layer selection editing, separate from live match state.
//!
//! Edits may leave an invalid draft. Validation is explicit, so removing an anchor
//! prunes its references without silently accepting an unsupported survivor.
use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value, json};

use crate::{
    map::{AuthoritativeMap, Layers, MapError},
    structures::{StructureDefinition, StructureKind},
};

/// A selection's terrain character chooses its ground, object or mine layer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LayerSelection {
    pub ch: u8,
    pub cells: Vec<[usize; 2]>,
}

#[derive(Clone, Copy)]
enum Layer {
    Ground,
    Object,
    Mine,
}
impl Layer {
    fn for_char(ch: u8) -> Option<Self> {
        match ch {
            b'N' => Some(Self::Mine),
            ch if b".D+MWF".contains(&ch) => Some(Self::Ground),
            ch if b"BH#TXY=RO".contains(&ch) => Some(Self::Object),
            _ => None,
        }
    }
    fn cells(self, layers: &Layers) -> &[u8] {
        match self {
            Self::Ground => &layers.ground,
            Self::Object => &layers.objects,
            Self::Mine => &layers.mines,
        }
    }
    fn cells_mut(self, layers: &mut Layers) -> &mut [u8] {
        match self {
            Self::Ground => &mut layers.ground,
            Self::Object => &mut layers.objects,
            Self::Mine => &mut layers.mines,
        }
    }
    fn materials(self, layers: &mut Layers) -> &mut BTreeMap<usize, String> {
        match self {
            Self::Ground => &mut layers.base_materials,
            Self::Object => &mut layers.object_materials,
            Self::Mine => &mut layers.mine_materials,
        }
    }
}

/// Editable layers and structures, separate from live match state.
#[derive(Clone, Debug)]
pub struct MapAuthoring {
    source: Value,
    w: usize,
    h: usize,
    layers: Layers,
    structures: Vec<StructureDefinition>,
}
impl MapAuthoring {
    pub fn from_map(map: &AuthoritativeMap) -> Self {
        let (w, h) = map.dimensions();
        Self {
            source: map.source().clone(),
            w,
            h,
            layers: map.layers().clone(),
            structures: map.structures().to_vec(),
        }
    }
    pub fn dimensions(&self) -> (usize, usize) {
        (self.w, self.h)
    }
    pub fn source(&self) -> &Value {
        &self.source
    }
    pub fn layers(&self) -> &Layers {
        &self.layers
    }
    pub fn structures(&self) -> &[StructureDefinition] {
        &self.structures
    }
    fn selection(&self, picked: &LayerSelection) -> Option<(Layer, BTreeSet<usize>)> {
        let layer = Layer::for_char(picked.ch)?;
        let mut cells = BTreeSet::new();
        for &[x, y] in &picked.cells {
            if x >= self.w || y >= self.h || !cells.insert(y * self.w + x) {
                return None;
            }
        }
        Some((layer, cells))
    }
    /// Bounds and occupied object/mine destinations are checked before any edit.
    /// Ground selections may overwrite ground, as in the prototype editor.
    pub fn move_selection(&mut self, picked: &mut LayerSelection, dx: i32, dy: i32) -> bool {
        let Some((layer, source)) = self.selection(picked) else {
            return false;
        };
        let mut moved = Vec::with_capacity(picked.cells.len());
        for &[x, y] in &picked.cells {
            let Some(nx) = x.checked_add_signed(dx as isize).filter(|x| *x < self.w) else {
                return false;
            };
            let Some(ny) = y.checked_add_signed(dy as isize).filter(|y| *y < self.h) else {
                return false;
            };
            let from = y * self.w + x;
            let to = ny * self.w + nx;
            let empty = if matches!(layer, Layer::Mine) {
                0
            } else {
                b'.'
            };
            if !matches!(layer, Layer::Ground)
                && layer.cells(&self.layers)[to] != empty
                && !source.contains(&to)
            {
                return false;
            }
            moved.push((from, to, [nx, ny], layer.cells(&self.layers)[from]));
        }
        let materials: Vec<_> = moved
            .iter()
            .map(|(from, ..)| layer.materials(&mut self.layers).get(from).cloned())
            .collect();
        self.clear_selection(layer, &source);
        for ((_, to, _, value), material) in moved.iter().zip(materials) {
            layer.cells_mut(&mut self.layers)[*to] = *value;
            if let Some(material) = material {
                layer.materials(&mut self.layers).insert(*to, material);
            }
        }
        if !matches!(layer, Layer::Mine) {
            let destinations: BTreeMap<_, _> = moved.iter().map(|m| (m.0, m.1)).collect();
            for section in self.structures.iter_mut().flat_map(|s| &mut s.sections) {
                if let Some(&cell) = destinations.get(&section.cell) {
                    // Resolve from the original cell once, including overlapping moves.
                    section.cell = cell;
                }
            }
        }
        picked.cells = moved.into_iter().map(|m| m.2).collect();
        self.refresh_materials();
        true
    }
    fn clear_selection(&mut self, layer: Layer, cells: &BTreeSet<usize>) {
        for &cell in cells {
            layer.cells_mut(&mut self.layers)[cell] = if matches!(layer, Layer::Mine) {
                0
            } else {
                b'.'
            };
            layer.materials(&mut self.layers).remove(&cell);
        }
    }
    /// Remove only the selected layer. Structural pruning remains explicit.
    pub fn remove_selection(&mut self, picked: &LayerSelection) -> bool {
        let Some((layer, cells)) = self.selection(picked) else {
            return false;
        };
        self.clear_selection(layer, &cells);
        self.refresh_materials();
        true
    }
    /// Drop sections whose object was removed, then remove their support references.
    /// Surviving sections retain their identity, health, material and anchor flag.
    pub fn prune_structures(&mut self) {
        for structure in &mut self.structures {
            let expected = match structure.kind {
                StructureKind::House => b'B',
                StructureKind::Bridge => b'=',
            };
            structure
                .sections
                .retain(|s| self.layers.objects.get(s.cell) == Some(&expected));
            let ids: BTreeSet<_> = structure.sections.iter().map(|s| s.id.clone()).collect();
            for section in &mut structure.sections {
                section.supports.retain(|id| ids.contains(id));
            }
        }
        self.structures.retain(|s| !s.sections.is_empty());
    }
    fn refresh_materials(&mut self) {
        self.layers.materials = self
            .layers
            .base_materials
            .iter()
            .filter(|(c, _)| self.layers.objects[**c] == b'.')
            .map(|(c, material)| (*c, material.clone()))
            .collect();
        for (&cell, material) in &self.layers.object_materials {
            if self.layers.objects[cell] != b'.' {
                self.layers.materials.insert(cell, material.clone());
            }
        }
        for (cell, object) in self.layers.objects.iter().enumerate() {
            if *object == b'R' {
                self.layers
                    .materials
                    .entry(cell)
                    .or_insert_with(|| "stone".into());
            }
        }
    }
    /// Current authoritative draft, including hidden mines and original map metadata.
    /// This is not a client projection and may fail validation after an edit.
    pub fn document(&self) -> Value {
        let rows = |cells: &[u8]| -> Vec<String> {
            cells
                .chunks(self.w)
                .map(|row| row.iter().map(|c| char::from(*c)).collect())
                .collect()
        };
        let material_rows = |materials: &BTreeMap<usize, String>| -> Vec<Value> {
            materials
                .iter()
                .map(|(c, material)| json!({"c":c,"material":material}))
                .collect()
        };
        let composed: Vec<_> = (0..self.w * self.h)
            .map(|c| {
                if self.layers.mines[c] != 0 {
                    b'N'
                } else if self.layers.objects[c] != b'.' {
                    self.layers.objects[c]
                } else {
                    self.layers.ground[c]
                }
            })
            .collect();
        let mut document = self.source.clone();
        document["worldVersion"] = json!(2);
        document["rows"] = json!(rows(&composed));
        document["layers"] = json!({
            "ground": rows(&self.layers.ground),
            "objects": rows(&self.layers.objects),
            "mines": rows(&self.layers.mines.iter().map(|m| if *m == 0 { b'.' } else { b'N' }).collect::<Vec<_>>()),
            "groundMaterials": material_rows(&self.layers.base_materials),
            "objectMaterials": material_rows(&self.layers.object_materials),
            "mineMaterials": material_rows(&self.layers.mine_materials),
        });
        document["structures"] = json!(self.structures.iter().map(|structure| {
            json!({
                "id":structure.id,"kind":structure.kind.as_str(),
                "sections":structure.sections.iter().map(|section| json!({
                    "id":section.id,"c":section.cell,"hp":section.hp,
                    "material":section.material,"anchor":section.anchor,"supports":section.supports,
                })).collect::<Vec<_>>()
            })
        }).collect::<Vec<_>>());
        document
    }
    pub fn validate(&self) -> Result<(), MapError> {
        AuthoritativeMap::from_value(self.document()).map(|_| ())
    }
}

#[cfg(test)]
mod tests;
