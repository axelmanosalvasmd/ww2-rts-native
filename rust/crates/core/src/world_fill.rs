//! Fill battle damage through the authoritative terrain layers and mutation records.
use crate::world::{World, WorldError};

fn level(world: &World, cell: usize) -> i8 {
    world.game.height.as_ref().map_or(0, |v| v[cell])
}

fn initial_level(world: &World, cell: usize) -> i8 {
    world.initial().height.as_ref().map_or(0, |v| v[cell])
}

fn fill_level(world: &World, cell: usize) -> i8 {
    let width = world.game.w;
    let x = cell % width;
    let low = [
        cell.checked_sub(width),
        cell.checked_add(width)
            .filter(|&c| c < world.game.chars.len()),
        cell.checked_sub(1).filter(|_| x > 0),
        (x + 1 < width).then_some(cell + 1),
    ]
    .into_iter()
    .flatten()
    .map(|c| level(world, c))
    .min();
    low.map_or(initial_level(world, cell), |low| {
        initial_level(world, cell).min(low.saturating_add(1))
    })
}

impl World {
    /// Battle craters, rubble, flooded ground and ground below its authored height.
    pub fn fillable(&self, cell: usize) -> bool {
        let Some(&ch) = self.game.chars.get(cell) else {
            return false;
        };
        match ch {
            b'+' => true,
            b'R' => !(self.geography().is_some() && self.initial().chars[cell] == b'R'),
            b'F' => self.initial().chars[cell] != b'F',
            b'.' => level(self, cell) < initial_level(self, cell),
            _ => false,
        }
    }

    /// Ground the AI repairs, preserving dry crater cover at its authored height.
    pub fn spoiled(&self, cell: usize) -> bool {
        let Some(&ch) = self.game.chars.get(cell) else {
            return false;
        };
        (ch == b'R'
            && self
                .game
                .ground
                .as_ref()
                .map_or(self.initial().chars[cell], |v| v[cell])
                == b'D')
            || (self.fillable(cell)
                && (ch == b'F' || level(self, cell) < initial_level(self, cell))
                && (ch != b'.' || fill_level(self, cell) > level(self, cell)))
    }

    /// Restore eligible damage without raising it above its map or four-neighbour slope.
    pub fn fill_cell(&mut self, cell: usize) -> Result<(), WorldError> {
        if cell >= self.game.chars.len() {
            return Err(WorldError::InvalidCell);
        }
        self.preflight_cell_rows(&[cell])?;
        if !self.fillable(cell) {
            return Err(WorldError::InvalidChange);
        }
        let height = fill_level(self, cell);
        let raise = height > level(self, cell);
        let target = if self.game.objects.as_ref().unwrap()[cell] != b'.' {
            self.game.ground.as_ref().unwrap()[cell]
        } else {
            b'.'
        };
        let clear = self.game.chars[cell] != target;
        let mutations = u64::from(raise) + u64::from(clear);
        if mutations == 0 {
            return Ok(());
        }
        self.mutation_capacity(mutations)?;
        if raise {
            self.set_level(cell, height)?;
        }
        if clear {
            self.set_cell(cell, target)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        game::VBLOCK,
        map::AuthoritativeMap,
        world::{CellChange, Recipient},
    };
    use serde_json::json;

    const CENTER: usize = 210;

    fn world(world_map: bool, authored: &[(usize, u8)], height: i8) -> World {
        let mut chars = vec![b'.'; 400];
        for &(cell, ch) in authored {
            chars[cell] = ch;
        }
        let rows: Vec<_> = chars
            .chunks(20)
            .map(|r| String::from_utf8(r.to_vec()).unwrap())
            .collect();
        let mut input = json!({"name":"Fill terrain","w":20,"h":20,"rows":rows,
            "heights":vec![char::from(b'0' + height as u8).to_string().repeat(20);20],
            "spawns":[{"x":0,"y":0},{"x":19,"y":19}],
            "points":[{"x":10,"y":9}]});
        if world_map {
            input["points"] = json!([]);
            let regions: Vec<_> = (0..64)
                .map(|id| {
                    let mut r = json!({"id":id,"x":id % 20,"y":id / 20,"bounds":[0,0,20,20]});
                    if id < 2 {
                        r["home"] = json!(id);
                    }
                    r
                })
                .collect();
            input["world"] = json!({"size":"huge","total":64,"regionCells":64,"regions":regions});
        }
        World::from_map(&AuthoritativeMap::from_json(&serde_json::to_vec(&input).unwrap()).unwrap())
            .unwrap()
    }

    #[test]
    fn intent_eng52_fillable_preserves_world_spoil_and_natural_fords() {
        let authored = [(CENTER, b'R'), (CENTER + 1, b'F')];
        let mut ordinary = world(false, &authored, 0);
        let mut geography = world(true, &authored, 0);
        assert!(ordinary.fillable(CENTER));
        assert!(!geography.fillable(CENTER));
        for w in [&mut ordinary, &mut geography] {
            assert!(!w.fillable(CENTER + 1));
            assert!(!w.fillable(CENTER + 2));
            w.set_cell(CENTER + 2, b'+').unwrap();
            w.set_cell(CENTER + 3, b'R').unwrap();
            w.set_cell(CENTER + 4, b'F').unwrap();
            w.set_level(CENTER + 5, -1).unwrap();
            for c in CENTER + 2..=CENTER + 5 {
                assert!(w.fillable(c), "cell {c}");
            }
            w.set_cell(CENTER + 6, b'M').unwrap();
            assert!(!w.fillable(CENTER + 6));
            assert!(!w.fillable(usize::MAX));
            assert!(!w.spoiled(usize::MAX));
        }
    }

    #[test]
    fn intent_eng52_fill_restores_bowls_from_lowest_neighbour_without_wrapping() {
        let mut w = world(false, &[], 2);
        w.set_cell(CENTER, b'+').unwrap();
        w.set_level(CENTER, -2).unwrap();
        w.set_level(CENTER - 1, -2).unwrap();
        w.fill_cell(CENTER).unwrap();
        assert_eq!(level(&w, CENTER), -1);
        assert_eq!(w.game.chars[CENTER], b'.');
        assert!(w.fillable(CENTER));
        assert!(!w.spoiled(CENTER));
        w.fill_cell(CENTER).unwrap();
        assert_eq!(level(&w, CENTER), -1);
        w.set_level(CENTER - 1, 2).unwrap();
        w.fill_cell(CENTER).unwrap();
        assert_eq!(level(&w, CENTER), 2);
        assert!(!w.fillable(CENTER));

        let edge = 20;
        w.set_level(edge, -2).unwrap();
        w.set_level(edge - 1, -2).unwrap();
        w.set_level(edge + 21, -2).unwrap();
        w.fill_cell(edge).unwrap();
        assert_eq!(
            level(&w, edge),
            2,
            "row wrap and diagonals are not neighbours"
        );
    }

    #[test]
    fn intent_eng52_rubble_restores_underlying_road_through_canonical_records() {
        let mut w = world(false, &[(CENTER, b'D')], 1);
        w.mutate_cell(
            CENTER,
            CellChange {
                object: Some(b'R'),
                material: Some("stone".into()),
                height: Some(0),
                ..CellChange::default()
            },
        )
        .unwrap();
        assert!(w.spoiled(CENTER));
        assert_ne!(w.game.flags[CENTER] & VBLOCK, 0);
        w.take_new_cells();
        w.reset_journal();
        w.register_recipient(Recipient::Player(0));
        let version = w.game.terrain_version;
        let regions = w.versions;
        let ground = w.game.ground.as_ref().unwrap().as_ptr();
        let height = w.game.height.as_ref().unwrap().as_ptr();
        w.fill_cell(CENTER).unwrap();
        assert_eq!(w.game.chars[CENTER], b'D');
        assert_eq!(w.game.ground.as_ref().unwrap()[CENTER], b'D');
        assert_eq!(w.game.objects.as_ref().unwrap()[CENTER], b'.');
        assert_eq!(level(&w, CENTER), 1);
        assert_eq!(w.game.flags[CENTER] & VBLOCK, 0);
        assert_eq!(w.material_name(CENTER, false), Some("road"));
        assert!(!w.object_materials.contains_key(&CENTER));
        assert_eq!(w.game.ground.as_ref().unwrap().as_ptr(), ground);
        assert_eq!(w.game.height.as_ref().unwrap().as_ptr(), height);
        assert_eq!(w.game.terrain_version, version + 2);
        assert_eq!(w.versions.infantry, regions.infantry + 1);
        assert_eq!(w.versions.vehicle, regions.vehicle + 2);
        assert_eq!(w.versions.obstruction, regions.obstruction + 1);
        assert_eq!(w.journal(), &[CENTER, CENTER]);
        assert_eq!(w.new_cells().len(), 2);
        assert_eq!((w.new_cells()[0].ch, w.new_cells()[0].height), (b'R', 1));
        assert_eq!((w.new_cells()[1].ch, w.new_cells()[1].height), (b'D', 1));
        assert_eq!(w.pending(Recipient::Player(0)).unwrap().len(), 1);
        assert_eq!(w.initial().chars[CENTER], b'D');
        assert!(!w.fillable(CENTER));
        assert!(!w.spoiled(CENTER));
    }

    #[test]
    fn intent_eng52_spoiled_preserves_cover_and_repairs_flooded_or_lowered_ground() {
        let mut w = world(false, &[], 0);
        w.set_cell(CENTER, b'+').unwrap();
        assert!(w.fillable(CENTER));
        assert!(!w.spoiled(CENTER));
        w.fill_cell(CENTER).unwrap();
        assert_eq!(w.game.chars[CENTER], b'.');
        w.set_cell(CENTER, b'R').unwrap();
        assert!(!w.spoiled(CENTER));
        w.set_cell(CENTER, b'.').unwrap();
        w.set_cell(CENTER, b'F').unwrap();
        assert!(w.spoiled(CENTER));
        w.fill_cell(CENTER).unwrap();
        assert_eq!(w.game.chars[CENTER], b'.');
        w.set_level(CENTER, -1).unwrap();
        assert!(w.spoiled(CENTER));
        w.set_level(CENTER - 1, -2).unwrap();
        assert!(!w.spoiled(CENTER));
        w.set_cell(CENTER, b'+').unwrap();
        assert!(w.spoiled(CENTER));
    }

    #[test]
    fn intent_eng52_fill_rejection_and_capacity_failure_leave_terrain_unchanged() {
        let mut w = world(true, &[(CENTER, b'R'), (CENTER + 1, b'F')], 0);
        for c in [CENTER, CENTER + 1, CENTER + 2] {
            assert_eq!(w.fill_cell(c), Err(WorldError::InvalidChange));
        }
        assert_eq!(w.fill_cell(usize::MAX), Err(WorldError::InvalidCell));
        w.set_cell(CENTER + 2, b'+').unwrap();
        w.set_level(CENTER + 2, -1).unwrap();
        let before = w.cell_row(CENTER + 2).unwrap();
        let journal = w.journal().to_vec();
        w.game.terrain_version = u32::MAX - 1;
        assert_eq!(w.fill_cell(CENTER + 2), Err(WorldError::VersionExhausted));
        assert_eq!(w.cell_row(CENTER + 2).unwrap(), before);
        assert_eq!(w.journal(), journal);
        assert_eq!(w.game.terrain_version, u32::MAX - 1);
    }
}
