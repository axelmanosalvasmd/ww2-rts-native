//! Bank-connected bridge construction over the canonical structural graph.
use crate::{
    game::Section,
    world::{Appearance, FailureMetadata, SectionMetadata, World, WorldError, WorldStructure},
};
use std::collections::BTreeSet;

impl World {
    pub(crate) fn neighbors(&self, c: usize) -> [Option<usize>; 4] {
        let w = self.game.w;
        [
            c.checked_sub(w),
            (c + w < self.game.chars.len()).then_some(c + w),
            c.checked_sub(1).filter(|_| !c.is_multiple_of(w)),
            (c % w + 1 < w).then_some(c + 1),
        ]
    }

    fn supported_bridge_section(&self, id: &str, structure: &WorldStructure) -> bool {
        let mut live = BTreeSet::new();
        for section in &structure.section_ids {
            if self
                .section_by_id(section)
                .is_some_and(|(_, s)| !s.failed && s.hp > 0.)
            {
                live.insert(section.as_str());
            }
        }
        let mut seen: BTreeSet<&str> = live
            .iter()
            .copied()
            .filter(|id| self.section_by_id(id).unwrap().0.anchor)
            .collect();
        loop {
            let before = seen.len();
            for &a in &live {
                for b in &self.section_by_id(a).unwrap().0.supports {
                    if live.contains(b.as_str()) && (seen.contains(a) || seen.contains(b.as_str()))
                    {
                        seen.insert(a);
                        seen.insert(b);
                    }
                }
            }
            if seen.len() == before {
                return seen.contains(id);
            }
        }
    }

    pub(crate) fn bridge_supported_at(&self, c: usize) -> bool {
        self.neighbors(c).into_iter().flatten().any(|n| {
            if self.game.chars[n] != b'=' {
                return !b"WF".contains(&self.game.ground.as_ref().unwrap()[n]);
            }
            let Some(section) = self.sections.get(&n) else {
                return true;
            };
            self.structures
                .iter()
                .find(|s| s.id == section.structure_id)
                .is_none_or(|s| self.supported_bridge_section(&section.id, s))
        })
    }

    pub(crate) fn build_bridge_cell(&mut self, c: usize) -> Result<(), WorldError> {
        if c >= self.game.chars.len() || self.game.chars[c] != b'W' || !self.bridge_supported_at(c)
        {
            return Err(WorldError::InvalidChange);
        }
        self.preflight_cell_rows(&[c])?;
        let neighbors = self.neighbors(c);
        let existing = self.sections.get(&c).and_then(|m| {
            self.structures
                .iter()
                .position(|s| s.id == m.structure_id && s.kind == "bridge")
        });
        let index = existing
            .or_else(|| {
                neighbors.into_iter().flatten().find_map(|n| {
                    let m = self.sections.get(&n)?;
                    self.structures.iter().position(|s| {
                        s.id == m.structure_id && s.kind == "bridge" && s.section_ids.len() < 128
                    })
                })
            })
            .unwrap_or(self.structures.len());
        if index == self.structures.len() {
            // Cell plus append-only graph count keeps native identity independent of actor IDs.
            self.structures.push(WorldStructure {
                id: format!("bridge:built:{c}:{index}"),
                kind: "bridge",
                cells: Vec::new(),
                section_ids: Vec::new(),
                entity_id: None,
            });
        }
        let structure_id = self.structures[index].id.clone();
        let id = self
            .sections
            .get(&c)
            .filter(|m| m.structure_id == structure_id)
            .map_or_else(|| format!("{structure_id}:{c}"), |m| m.id.clone());
        let supports: Vec<_> = neighbors
            .into_iter()
            .flatten()
            .filter_map(|n| {
                self.sections
                    .get(&n)
                    .filter(|m| m.structure_id == structure_id)
                    .map(|m| m.id.clone())
            })
            .collect();
        let anchor = neighbors
            .into_iter()
            .flatten()
            .any(|n| !b"WF".contains(&self.game.ground.as_ref().unwrap()[n]));
        self.set_cell(c, b'=')?;
        if let Some(prior) = self.sections.remove(&c) {
            let state = self
                .game
                .sections
                .remove(&(c as u32))
                .ok_or(WorldError::InvalidTerrain)?;
            if prior.id != id {
                self.detached_sections.insert(
                    prior.id.clone(),
                    crate::world::DetachedSection {
                        cell: c,
                        metadata: prior,
                        state,
                    },
                );
            }
        }
        // set_cell has reset the old stone multiplier before reading the plank health.
        let hp = f64::from(self.game.cell_hp[c]);
        self.game.sections.insert(
            c as u32,
            Section {
                failed: false,
                hp,
                max_hp: Some(hp),
            },
        );
        self.sections.insert(
            c,
            SectionMetadata {
                id: id.clone(),
                structure_id,
                material: "wood".into(),
                anchor,
                supports: supports.clone(),
                rubble: Vec::new(),
                appearance: Appearance::Intact,
                failure: FailureMetadata::default(),
            },
        );
        for neighbor in supports {
            if let Some((m, _)) = self.section_by_id_mut(&neighbor)
                && !m.supports.contains(&id)
            {
                m.supports.push(id.clone());
            }
        }
        let s = &mut self.structures[index];
        if !s.section_ids.contains(&id) {
            s.section_ids.push(id);
            s.cells.push(c);
        }
        self.log_cell(c, false)?;
        Ok(())
    }
}
