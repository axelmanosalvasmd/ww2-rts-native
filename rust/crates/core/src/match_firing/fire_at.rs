//! Terrain-targeted orders use the ordinary projectile and recipient owners.
use super::*;
use crate::{game::SIGHT, path_requests::RequestKind};

impl Match {
    pub(crate) fn fire_at_center(&self, cell: usize) -> Result<Point, String> {
        let g = &self.world.game;
        if cell >= g.chars.len() {
            return Err("fire-at cell outside battlefield".into());
        }
        Ok(Point {
            x: (cell % g.w) as f64 * CELL + CELL / 2.,
            z: (cell / g.w) as f64 * CELL + CELL / 2.,
        })
    }
    pub(crate) fn accepts_fire_at(&self, id: UnitId, solid: bool) -> Result<bool, String> {
        Ok(self
            .weapon(&self.world.game.units[&id])?
            .is_some_and(|w| flag(w, "salvo") || solid && flag(w, "shellTerrain")))
    }
    fn fire_at_range(&self, u: &Unit, w: &Value) -> Result<f64, String> {
        let nest = matches!(
            self.rules.kinds[u.kind].name.as_str(),
            "mg" | "at" | "mortar" | "flak" | "howitzer"
        ) && self.world.game.cell_of(u.x, u.z).is_some_and(|c| {
            self.field_works.pits().contains(&c) && self.world.game.chars[c] == b'T'
        });
        let factor = if nest {
            n(
                Catalog::compiled()
                    .map_err(|e| e.to_string())?
                    .table("FORTS")
                    .map_err(|e| e.to_string())?
                    .get("nest")
                    .ok_or("missing nest rules")?,
                "range",
                1.,
            )
        } else {
            1.
        };
        Ok(n(w, "range", 0.) * factor)
    }
    pub(crate) fn can_shell_cell(&self, id: UnitId, cell: usize) -> Result<bool, String> {
        let g = &self.world.game;
        let u = g.units.get(&id).ok_or("missing terrain shooter")?;
        let at = self.fire_at_center(cell)?;
        let sees = |at: Point| vision::los(g, &self.rules, (u.x, u.z), (at.x, at.z));
        if sees(at) {
            return Ok(true);
        }
        let Some(target) = self.world.sections.get(&cell) else {
            return Ok(false);
        };
        let Some(blocker) = first_sight_blocker(g, &self.rules, u.at(), at) else {
            return Ok(false);
        };
        Ok(self
            .world
            .sections
            .get(&blocker)
            .is_some_and(|s| s.structure_id == target.structure_id)
            && sees(self.fire_at_center(blocker)?))
    }
    pub(crate) fn current_fire_at_goal(&self, id: UnitId) -> Result<Option<Point>, String> {
        let Some(u) = self.world.game.units.get(&id) else {
            return Ok(None);
        };
        let Some(cell) = self.commands.units.get(&id).and_then(|c| c.fire_at) else {
            return Ok(None);
        };
        let at = self.fire_at_center(cell)?;
        if u.hp <= 0. || !u.path.is_empty() {
            return Ok(None);
        }
        let w = self.weapon(u)?.ok_or("unarmed terrain order")?;
        let salvo = flag(w, "salvo");
        if !salvo && self.world.game.cell_hp[cell] <= 0. {
            return Ok(None);
        }
        Ok((dist(u.at(), at) > self.fire_at_range(u, w)?
            || !salvo && !self.can_shell_cell(id, cell)?)
        .then_some(at))
    }
    pub(crate) fn apply_fire_at_order(&mut self, id: UnitId) -> Result<(), String> {
        let Some(cell) = self.commands.units.get(&id).and_then(|c| c.fire_at) else {
            return Ok(());
        };
        let u = &self.world.game.units[&id];
        let w = self.weapon(u)?.ok_or("unarmed terrain order")?;
        let salvo = flag(w, "salvo");
        let at = self.fire_at_center(cell)?;
        if !salvo && self.world.game.cell_hp[cell] <= 0. {
            self.commands.units.get_mut(&id).unwrap().fire_at = None;
        } else if dist(u.at(), at) <= self.fire_at_range(u, w)?
            && (salvo || self.can_shell_cell(id, cell)?)
        {
            let u = self.world.game.units.get_mut(&id).unwrap();
            if !u.path.is_empty() {
                u.path.clear();
                u.path_gen = u
                    .path_gen
                    .checked_add(1)
                    .ok_or("terrain order path identity exhausted")?;
            }
        } else if u.path.is_empty() && u.repath <= 0. {
            self.request_command_path(id, at, RequestKind::FireAt)?;
        }
        Ok(())
    }
    pub(super) fn fire_at_weapon(&mut self, id: UnitId, terrain_suppression: f64) -> Result<(), String> {
        let u = &self.world.game.units[&id];
        let w = self.weapon(u)?.ok_or("unarmed terrain order")?;
        self.firing.initialize(id).target_id = 0;
        if n(w, "range", 0.) == 0. || !self.logistics.can_fire(id, 1.) || u.retreating {
            self.commands.units.get_mut(&id).unwrap().aim = u.rot;
            return Ok(());
        }
        let cell = self.commands.units[&id]
            .fire_at
            .ok_or("missing terrain order")?;
        let at = self.fire_at_center(cell)?;
        self.commands.units.get_mut(&id).unwrap().aim = math::atan2(at.z - u.z, at.x - u.x);
        if !u.path.is_empty()
            || dist(u.at(), at) > self.fire_at_range(u, w)?
            || self.firing.get(id).unwrap().cooldown > 0.
        {
            return Ok(());
        }
        let rate = self.logistics.shortage_rate(id);
        let interval = n(w, "interval", 0.);
        if flag(w, "salvo") {
            if u.still.is_some_and(|s| s >= n(w, "setup", 0.)) {
                self.launch_weapon_salvo(id, at, None)?;
                self.firing.initialize(id).cooldown = interval * self.logistics.shortage_rate(id);
            }
        } else if self.world.game.cell_hp[cell] > 0. && self.can_shell_cell(id, cell)? {
            let suppression = if terrain_suppression >= 90. {
                3.
            } else if terrain_suppression >= 50. {
                1.5
            } else {
                1.
            };
            self.launch_terrain_shell(id, cell)?;
            self.firing.initialize(id).cooldown = interval * suppression * rate;
        }
        Ok(())
    }
    fn launch_terrain_shell(&mut self, id: UnitId, cell: usize) -> Result<(), String> {
        let at = self.fire_at_center(cell)?;
        let end = self.launch_admission(id, at, 1)?;
        let g = &self.world.game;
        let u = &g.units[&id];
        let w = self.weapon(u)?.ok_or("unarmed terrain launch")?;
        let level_height = n(self.firing_config()?, "levelHeight", 2.5);
        let obstacle = match g.physical(cell) {
            b'B' | b'K' => 5.,
            b'H' => 1.8,
            b'#' | b'Y' => 1.2,
            b'Q' => 1.6,
            b'R' | b'O' => 0.8,
            b'=' => 0.25,
            _ => 0.,
        };
        let from = projectiles::Point3::new(
            u.x,
            f64::from(g.level_at(u.x, u.z)) * level_height
                + if u.garrison.is_some() { 3.5 } else { 1.2 },
            u.z,
        );
        let aim = projectiles::Point3::new(
            at.x,
            f64::from(g.level_at(at.x, at.z)) * level_height
                + math::max(0.2, math::min(1.2, obstacle / 2.)),
            at.z,
        );
        let profile = projectiles::projectile_profile(
            &self.rules.kinds[u.kind].name,
            flag(w, "flame"),
            n(w, "veh", 0.),
            flag(w, "shellTerrain"),
        )
        .map_err(|e| format!("terrain projectile: {e:?}"))?;
        let solution = projectiles::launch_solution(
            from,
            aim,
            profile.speed.unwrap_or(1.),
            profile.gravity,
            None,
        )
        .map_err(|e| format!("terrain trajectory: {e:?}"))?;
        let born = g.tick as f64 * self.rules.movement.tick;
        let flight = LiveFlight {
            id: self.firing.next_flight,
            owner: u.owner,
            shooter: id,
            source_kind: u.kind,
            solution: projectiles::Flight {
                origin: solution.origin,
                velocity: solution.velocity,
                gravity: solution.gravity,
                stamp: born,
            },
            launch_origin: from,
            launched: born,
            last: born,
            expires: born
                + math::max(
                    solution.duration + 0.15,
                    n(w, "range", 20.) / profile.speed.unwrap_or(1.),
                ),
            sent: true,
            energy: 1.,
            sequence: 0,
            max_contacts: profile.max_contacts,
            near: profile.near,
            start_cell: g
                .cell_of(u.x, u.z)
                .ok_or("terrain shooter outside battlefield")?,
            intended: None,
            damage_scale: 1.,
            armor_piercing: false,
            ap_multiplier: 1.5,
            ap_naval: false,
            suppression: n(w, "supp", 0.),
            suppression_scale: 1.,
            volley_group: self.firing.next_flight,
            explosive: true,
            terrain: n(w, "shellTerrain", 0.),
            scar: n(
                self.firing_config()?
                    .get("scar")
                    .ok_or("missing scar rules")?,
                "tank",
                0.25,
            ),
            blast_radius: None,
            blast_source: None,
            effect: None,
        };
        self.logistics.consume_ammo(id, 1.);
        self.firing.next_flight = end;
        self.commit_physical_flight(flight);
        self.world.game.units.get_mut(&id).unwrap().shot_at = Some(self.world.game.tick as f64);
        Ok(())
    }
}

// Same grid walk as terrain::clear, returning the first obstruction before the endpoint.
fn first_sight_blocker(
    g: &crate::game::Game,
    rules: &crate::rules::Rules,
    a: Point,
    b: Point,
) -> Option<usize> {
    let (mut x, mut z) = ((a.x / CELL).floor() as i64, (a.z / CELL).floor() as i64);
    let (ex, ez) = ((b.x / CELL).floor() as i64, (b.z / CELL).floor() as i64);
    let (dx, dz) = (b.x - a.x, b.z - a.z);
    let (sx, sz) = (vision::step_of(dx), vision::step_of(dz));
    let tdx = if dx == 0. {
        f64::INFINITY
    } else {
        CELL / dx.abs()
    };
    let tdz = if dz == 0. {
        f64::INFINITY
    } else {
        CELL / dz.abs()
    };
    let mut tx = if dx == 0. {
        f64::INFINITY
    } else {
        (if sx > 0 {
            (x + 1) as f64 * CELL - a.x
        } else {
            a.x - x as f64 * CELL
        }) / dx.abs()
    };
    let mut tz = if dz == 0. {
        f64::INFINITY
    } else {
        (if sz > 0 {
            (z + 1) as f64 * CELL - a.z
        } else {
            a.z - z as f64 * CELL
        }) / dz.abs()
    };
    let mut wood = 0.;
    for _ in 0..g.w + g.h {
        if x == ex && z == ez {
            break;
        }
        if z == ez || x != ex && tx < tz {
            x += sx;
            tx += tdx;
        } else {
            z += sz;
            tz += tdz;
        }
        if x == ex && z == ez {
            break;
        }
        if x < 0 || z < 0 || x >= g.w as i64 || z >= g.h as i64 {
            return None;
        }
        let c = z as usize * g.w + x as usize;
        if g.flags[c] & SIGHT != 0 {
            return Some(c);
        }
        if g.flags[c] & crate::game::WOOD != 0 {
            wood += 1.;
            if wood > rules.vision.wood_sight {
                return Some(c);
            }
        }
    }
    None
}
