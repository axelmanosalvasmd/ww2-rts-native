//! Source-positioned crushing and vehicle wear through the canonical World owner.
use crate::{catalog::Catalog, game::Pose, grid::UnitId, match_runtime::Match, math};

impl Match {
    #[cfg(test)]
    pub(crate) fn apply_moved_ground_contact(
        &mut self,
        id: UnitId,
        before: Pose,
        before_dust: Option<f64>,
    ) -> Result<(), String> {
        self.apply_moved_ground_contact_emitting(id, before, before_dust, &mut |_| {})
    }

    /// After real movement and before firing. Hostile mines remain unavailable.
    pub(crate) fn apply_moved_ground_contact_emitting(
        &mut self,
        id: UnitId,
        before: Pose,
        before_dust: Option<f64>,
        out: &mut dyn FnMut(crate::environment::Effect),
    ) -> Result<(), String> {
        let unit = self
            .world
            .game
            .units
            .get(&id)
            .ok_or("missing ground-contact actor")?;
        let moved = math::hypot(unit.x - before.x, unit.z - before.z);
        if !moved.is_finite() {
            return Err("invalid ground-contact displacement".into());
        }
        if moved <= 0.0 {
            return Ok(());
        }
        // The workload mover raised dust before this Source contact boundary.
        // Undo its premature dust even if a fallible contact cannot complete.
        self.world.game.units.get_mut(&id).unwrap().dust = before_dust;
        let g = &self.world.game;
        let unit = &g.units[&id];
        let profile = self
            .rules
            .kinds
            .get(unit.kind)
            .ok_or("unknown ground-contact kind")?;
        self.actor_policy(unit.owner)?;
        if self.uses_world_routes() {
            self.validate_world_ground_actor(unit)?;
            if profile.naval || profile.structure {
                return Err("unsupported World ground-contact profile".into());
            }
        }
        let Some(c) = g.cell_of(unit.x, unit.z) else {
            return Ok(());
        };
        self.world
            .preflight_cell_rows(&[c])
            .map_err(|e| e.to_string())?;
        let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
        let definition = catalog
            .table("UNITS")
            .map_err(|e| e.to_string())?
            .get(&profile.name)
            .ok_or("missing ground-contact kind")?;
        let cfg = catalog.table("CFG").map_err(|e| e.to_string())?;
        let crushes = definition
            .get("crushes")
            .and_then(|v| v.boolean())
            .unwrap_or(false);
        let ch = g.chars[c];
        if let Some(into) = crushes.then(|| cfg.get("crush")
            .and_then(|v| v.get(&(ch as char).to_string()))).flatten()
        {
            let into = into.string().filter(|s| s.len() == 1)
                .ok_or("invalid crushing replacement")?.as_bytes()[0];
            let ids: Vec<_> = g.units.keys().copied().collect();
            let mut context = crate::environment::Context {
                tick: u64::try_from(g.tick).map_err(|_| "invalid crushing clock")?,
                seed: &mut self.start.environment_seed,
                next_id: &mut self.next_unit_id,
                unit_order: &ids,
                grid: &self.grid,
                rain: g.rain,
                wet: g.wet.unwrap_or(0.0),
                wind_angle: self.start.wind.0,
                wind_strength: self.start.wind.1,
            };
            self.environment.wreck_cell_emitting(&mut self.world, c, Some(into), &mut context, out)
                .map_err(|e| format!("moved crushing: {e:?}"))?;
        }
        // Contact can change the physical cell. Wear and dust read its new state.
        let g = &self.world.game;
        let unit = &g.units[&id];
        let ch = g.chars[c];
        if ch == b'N' {
            let source_owner = self.world.mines.get(&c).copied().unwrap_or(-1);
            let owner = if source_owner >= 0 {
                Some(source_owner as usize)
            } else if source_owner == -1 && self.uses_world_routes() {
                Some(crate::game::LOCAL_DEFENDER_OWNER)
            } else {
                None
            };
            if !owner.is_some_and(|owner| g.allied(owner, unit.owner)) {
                return Err("moved hostile mine detonation is not connected".into());
            }
        }
        if profile.infantry {
            return Ok(());
        }
        let wet = crate::terrain::wet_at(g, c);
        if matches!(ch, b'.' | b'M') {
            let wear = g
                .wear
                .as_ref()
                .and_then(|v| v.get(c))
                .copied()
                .ok_or("missing ground-contact wear")?;
            let traffic = cfg
                .get("traffic")
                .and_then(|v| v.number())
                .ok_or("missing traffic wear rate")?;
            let wet_wear = cfg
                .get("weather")
                .and_then(|v| v.get("wetWear"))
                .and_then(|v| v.number())
                .ok_or("missing wet wear rate")?;
            if ![f64::from(wear), wet, traffic, wet_wear]
                .iter()
                .all(|v| v.is_finite() && *v >= 0.0)
            {
                return Err("invalid ground-contact wear inputs".into());
            }
            let next = math::min(
                1.0,
                f64::from(wear)
                    + moved
                        * traffic
                        * if crushes { 1.0 } else { 0.5 }
                        * if ch == b'M' { 0.5 } else { 1.0 }
                        * (1.0 + wet_wear * wet),
            ) as f32;
            self.world
                .apply_vehicle_wear(c, next)
                .map_err(|e| e.to_string())?;
        }
        let unit = self.world.game.units.get_mut(&id).unwrap();
        // The workload mover raises dust before terrain contact. Match applies its
        // Source decision after contact, so newly made mud preserves prior dust.
        unit.dust = if wet < 0.3
            && self.world.game.flags[c] & (crate::game::MUD | crate::game::FORD) == 0
        {
            Some(self.world.game.tick as f64)
        } else {
            before_dust
        };
        Ok(())
    }
}

#[cfg(test)]
#[path = "match_ground_contact_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "match_world_contact_tests.rs"]
mod world_tests;
