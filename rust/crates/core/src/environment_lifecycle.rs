//! Building damage reconciliation and ordered vehicle wreck lifecycle.
use super::*;
use crate::{
    combat::DeathImpulse,
    environment_debris::{DEBRIS_LIMITS, wreck_mass},
    game::{Unit, Wreck},
};

/// Wreck query position and obstruction cell stay in `Game::wrecks`.
#[derive(Clone, Debug, PartialEq)]
pub struct WreckMetadata {
    pub kind: usize,
    pub owner: usize,
    pub rotation: f64,
    pub motion: Option<DebrisBody>,
}

impl EnvironmentRuntime {
    /// Remove metadata whose canonical hull was cleared by a terrain mutation.
    pub fn reconcile_wreck_metadata(&mut self, w: &World) {
        let live: BTreeSet<_> = w.game.wrecks.iter().filter_map(|wreck| wreck.id).collect();
        self.wreck_metadata.retain(|id, _| live.contains(id));
    }

    /// Read-only admission checks before the parent removes any corpse from live state.
    /// The caller checks combat-only death impulses, owners and bounty inputs.
    pub fn validate_retirement(
        &self,
        w: &World,
        corpses: &[&Unit],
        ctx: &Context<'_>,
    ) -> Result<(), EnvironmentError> {
        let t = check(w, ctx)?;
        self.wreck_bounds(w, t)?;
        wreck_limit()?;
        let mut ids = BTreeSet::new();
        for unit in corpses {
            detached_unit_bounds(unit, t)?;
            if !ids.insert(unit.id) {
                return Err(EnvironmentError::InvalidInput("duplicate corpse identity"));
            }
            let kind = &t.kinds.kinds[unit.kind];
            if kind.building {
                let footprint = unit
                    .cells
                    .as_ref()
                    .ok_or(EnvironmentError::InvalidInput("building footprint"))?;
                if footprint
                    .iter()
                    .any(|cell| *cell as usize >= w.game.chars.len())
                {
                    return Err(EnvironmentError::InvalidInput("building footprint"));
                }
                if let Some(structure) = w.structures.iter().find(|s| s.entity_id == Some(unit.id))
                {
                    section_bounds(w, structure)?;
                }
            } else if !kind.infantry
                && !kind.air
                && kind.speed > 0.
                && w.game.flags_at(unit.x, unit.z) & MOVE == 0
                && w.game.wrecks.iter().any(|wreck| wreck.id == Some(unit.id))
            {
                return Err(EnvironmentError::InvalidInput("duplicate wreck identity"));
            }
        }
        Ok(())
    }

    pub fn wreck_metadata(&self) -> &BTreeMap<u32, WreckMetadata> {
        &self.wreck_metadata
    }

    /// Reconcile externally applied building health loss against its nearest sections first.
    /// The owner comes from canonical combat metadata's last-hit field.
    pub fn sync_building_sections(
        &mut self,
        w: &mut World,
        unit_id: u32,
        owner: Option<i32>,
        ctx: &mut Context<'_>,
    ) -> Result<Vec<Effect>, EnvironmentError> {
        let mut out = Vec::new();
        self.sync_building_sections_emitting(w, unit_id, owner, ctx, &mut |effect| {
            out.push(effect)
        })?;
        Ok(out)
    }
    /// Emit each committed effect in source order, including the prefix before an error.
    pub fn sync_building_sections_emitting(
        &mut self,
        w: &mut World,
        unit_id: u32,
        owner: Option<i32>,
        ctx: &mut Context<'_>,
        out: &mut dyn FnMut(Effect),
    ) -> Result<(), EnvironmentError> {
        check(w, ctx)?;
        let unit = w
            .game
            .units
            .get(&unit_id)
            .ok_or(EnvironmentError::InvalidInput("building unit"))?
            .clone();
        let Some(structure) = w
            .structures
            .iter()
            .find(|s| s.entity_id == Some(unit_id))
            .cloned()
        else {
            return Ok(());
        };
        if unit.hp <= 0. {
            return Ok(());
        }
        let at = unit.hit_from.unwrap_or(unit.at());
        if ![at.x, at.z].iter().all(|v| v.is_finite())
            || at.x.abs() > 1_073_741_824.
            || at.z.abs() > 1_073_741_824.
        {
            return Err(EnvironmentError::InvalidInput("building hit origin"));
        }
        section_bounds(w, &structure)?;
        let mut loss = section_hp(w, &structure) - unit.hp;
        if loss <= 0.001 {
            return Ok(());
        }
        let mut sections: Vec<_> = structure.section_ids.iter().zip(&structure.cells).collect();
        // Stable sorting preserves the structure's source order when distances tie.
        sections.sort_by(|(_, a), (_, b)| {
            let a = center(w, **a);
            let b = center(w, **b);
            math::hypot(a.x - at.x, a.z - at.z).total_cmp(&math::hypot(b.x - at.x, b.z - at.z))
        });
        let skip = BTreeSet::new();
        let hit = DamageContext {
            actor_order: None,
            owner,
            contact_id: None,
            direction: Some(Point {
                x: unit.x - at.x,
                z: unit.z - at.z,
            }),
            debit_entity: false,
            skip_buildings: &skip,
            memory: None,
        };
        for (id, cell) in sections {
            if loss <= 0.001 {
                break;
            }
            let amount = math::min(loss, w.section_by_id(id).unwrap().1.hp);
            if amount <= 0. {
                continue;
            }
            self.damage_section_inner(w, *cell, amount, ctx, &hit, out)?;
            loss -= amount;
        }
        let remaining = section_hp(w, &structure);
        let unit = w.game.units.get_mut(&unit_id).unwrap();
        unit.hp = math::min(unit.hp, remaining);
        self.reconcile_wreck_metadata(w);
        Ok(())
    }

    /// The caller removes the dead building from the current units before this operation.
    /// Depot-node ownership is outside this environment state and remains caller-owned.
    pub fn wreck_building(
        &mut self,
        w: &mut World,
        unit: &Unit,
        ctx: &mut Context<'_>,
    ) -> Result<Vec<Effect>, EnvironmentError> {
        let mut out = Vec::new();
        self.wreck_building_emitting(w, unit, ctx, &mut |effect| out.push(effect))?;
        Ok(out)
    }
    /// Emit each committed effect in source order, including the prefix before an error.
    pub fn wreck_building_emitting(
        &mut self,
        w: &mut World,
        unit: &Unit,
        ctx: &mut Context<'_>,
        out: &mut dyn FnMut(Effect),
    ) -> Result<(), EnvironmentError> {
        let t = check(w, ctx)?;
        detached_unit_bounds(unit, t)?;
        let footprint = unit
            .cells
            .as_ref()
            .ok_or(EnvironmentError::InvalidInput("building footprint"))?;
        if footprint.iter().any(|c| *c as usize >= w.game.chars.len()) {
            return Err(EnvironmentError::InvalidInput("building footprint"));
        }
        let structure = w
            .structures
            .iter()
            .find(|s| s.entity_id == Some(unit.id))
            .cloned();
        if let Some(structure) = &structure {
            section_bounds(w, structure)?;
        }
        if let Some(structure) = structure {
            for (id, cell) in structure.section_ids.iter().zip(structure.cells) {
                self.fail_section(w, id, cell, ctx, Some(Point { x: 0., z: 1. }), None, out)?;
            }
        }
        for &cell in footprint {
            let cell = cell as usize;
            if w.game.chars[cell] != b'R' {
                w.set_cell(cell, b'R')?;
            }
        }
        self.reconcile_wreck_metadata(w);
        out(Effect::BuildingCollapse {
            unit: unit.id,
            owner: unit.owner,
            position: unit.at(),
        });
        Ok(())
    }

    /// Create the dead vehicle's hull after the caller removes its unit from live state.
    pub fn spawn_vehicle_wreck(
        &mut self,
        w: &mut World,
        unit: &Unit,
        impulse: Option<DeathImpulse>,
        ctx: &mut Context<'_>,
    ) -> Result<Vec<Effect>, EnvironmentError> {
        let mut out = Vec::new();
        self.spawn_vehicle_wreck_emitting(w, unit, impulse, ctx, &mut |effect| out.push(effect))?;
        Ok(out)
    }
    /// Create a hull through the emitting API. Hull creation currently emits no effects.
    pub fn spawn_vehicle_wreck_emitting(
        &mut self,
        w: &mut World,
        unit: &Unit,
        impulse: Option<DeathImpulse>,
        ctx: &mut Context<'_>,
        _emit: &mut dyn FnMut(Effect),
    ) -> Result<(), EnvironmentError> {
        let t = check(w, ctx)?;
        detached_unit_bounds(unit, t)?;
        self.reconcile_wreck_metadata(w);
        self.wreck_bounds(w, t)?;
        let kind = &t.kinds.kinds[unit.kind];
        if kind.infantry
            || kind.air
            || kind.speed <= 0.
            || w.game.flags_at(unit.x, unit.z) & MOVE != 0
        {
            return Ok(());
        }
        if self.wreck_metadata.contains_key(&unit.id)
            || w.game.wrecks.iter().any(|wreck| wreck.id == Some(unit.id))
        {
            return Err(EnvironmentError::InvalidInput("duplicate wreck identity"));
        }
        let impulse = impulse.unwrap_or(DeathImpulse {
            direction: unit.rot,
            magnitude: 0.25,
        });
        let mut body = DebrisBody::new(DebrisInput {
            id: format!("wreck:{}", unit.id),
            kind: DebrisKind::Wreck,
            x: unit.x,
            y: f64::from(w.game.level_at(unit.x, unit.z)) * t.level_height + 0.08,
            z: unit.z,
            dir: impulse.direction,
            impulse: impulse.magnitude,
            mass: wreck_mass(&kind.name, Some(kind.radius)),
            material: "steel".into(),
            radius: math::min(2.2, kind.radius * 0.6),
            time: ctx.tick as f64 * 0.05,
        })
        .map_err(|_| EnvironmentError::InvalidInput("wreck impulse"))?;
        let cap = wreck_limit()?;
        let overflow = self.falling_sections.len()
            + w.game.wrecks.iter().filter(|wreck| wreck.moving).count()
            + 1
            > DEBRIS_LIMITS.active;
        // Overflow solving changes only the new body. Validate it before touching canonical state.
        if overflow {
            finish_wreck_overflow(w, &mut body, ctx, t)?;
        }
        let index = w.game.wrecks.len();
        w.game.wrecks.push(Wreck {
            id: Some(unit.id),
            cell: None,
            x: unit.x,
            z: unit.z,
            moving: true,
        });
        self.wreck_metadata.insert(
            unit.id,
            WreckMetadata {
                kind: unit.kind,
                owner: unit.owner,
                rotation: unit.rot,
                motion: Some(body),
            },
        );
        for (dx, dz) in [(0., 0.), (2., 0.), (-2., 0.), (0., 2.), (0., -2.)] {
            if let Some(cell) = w.game.cell_of(unit.x + dx, unit.z + dz)
                && w.game.physical(cell) == b'.'
                && w.burnt[cell] == 0
            {
                w.burnt[cell] = 1;
                w.touch(cell)?;
            }
        }
        if overflow {
            self.settle_wreck(w, index, ctx, t)?;
        }
        if w.game.wrecks.len() > cap {
            let old = w.game.wrecks.remove(0);
            if let Some(id) = old.id {
                self.wreck_metadata.remove(&id);
            }
            if let Some(cell) = old.cell
                && w.game.physical(cell) == b'Q'
            {
                w.mutate_cell(
                    cell,
                    CellChange {
                        object: Some(b'.'),
                        ..Default::default()
                    },
                )?;
            }
        }
        self.reconcile_wreck_metadata(w);
        Ok(())
    }

    /// Call after `step_sections` to retain the source's section-then-wreck phase order.
    pub fn step_wrecks(
        &mut self,
        w: &mut World,
        dt: f64,
        ctx: &mut Context<'_>,
    ) -> Result<(), EnvironmentError> {
        let t = check(w, ctx)?;
        if !dt.is_finite() || !(0.0..=2.5).contains(&dt) {
            return Err(EnvironmentError::InvalidInput("wreck dt"));
        }
        self.reconcile_wreck_metadata(w);
        self.wreck_bounds(w, t)?;
        for index in 0..w.game.wrecks.len() {
            let Some(id) = w.game.wrecks[index].id else {
                continue;
            };
            let Some(mut body) = self.wreck_metadata[&id].motion.clone() else {
                continue;
            };
            body.step(
                dt,
                &mut WorldDebris {
                    world: w,
                    grid: ctx.grid,
                    tuning: t,
                },
            )
            .map_err(|_| EnvironmentError::InvalidInput("wreck environment"))?;
            let settled = body.settled;
            w.game.wrecks[index].x = body.x;
            w.game.wrecks[index].z = body.z;
            self.wreck_metadata.get_mut(&id).unwrap().motion = Some(body);
            if settled {
                self.settle_wreck(w, index, ctx, t)?;
            }
        }
        Ok(())
    }

    fn settle_wreck(
        &mut self,
        w: &mut World,
        index: usize,
        ctx: &Context<'_>,
        t: &Tuning,
    ) -> Result<(), EnvironmentError> {
        let id = w.game.wrecks[index].id.unwrap();
        let body = self.wreck_metadata[&id].motion.as_ref().unwrap();
        let (x, z) = (body.x, body.z);
        w.game.wrecks[index].x = x;
        w.game.wrecks[index].z = z;
        if let Some(cell) = w.game.cell_of(x, z)
            && b".+DM".contains(&w.game.physical(cell))
            && !occupied(w, cell, ctx.grid, t)
        {
            w.mutate_cell(
                cell,
                CellChange {
                    object: Some(b'Q'),
                    material: Some("steel".into()),
                    ..Default::default()
                },
            )?;
            w.game.wrecks[index].cell = Some(cell);
        }
        self.wreck_metadata.get_mut(&id).unwrap().motion = None;
        w.game.wrecks[index].moving = false;
        Ok(())
    }

    fn wreck_bounds(&self, w: &World, t: &Tuning) -> Result<(), EnvironmentError> {
        let mut ids = BTreeSet::new();
        for wreck in &w.game.wrecks {
            if ![wreck.x, wreck.z].iter().all(|v| v.is_finite())
                || wreck.cell.is_some_and(|c| c >= w.game.chars.len())
            {
                return Err(EnvironmentError::InvalidInput("wreck query"));
            }
            let Some(id) = wreck.id else {
                continue;
            };
            if !ids.insert(id) {
                return Err(EnvironmentError::InvalidInput("duplicate wreck identity"));
            }
            let meta = self
                .wreck_metadata
                .get(&id)
                .ok_or(EnvironmentError::InvalidInput("wreck metadata"))?;
            if meta.kind >= t.kinds.kinds.len()
                || !meta.rotation.is_finite()
                || wreck.moving != meta.motion.is_some()
            {
                return Err(EnvironmentError::InvalidInput("wreck metadata"));
            }
            if let Some(body) = &meta.motion {
                body_bounds(body)?;
            }
        }
        // A terrain operation may have removed a hull since the last reconciliation.
        // Read-only retirement admission validates the surviving canonical hulls only.
        Ok(())
    }
}

fn detached_unit_bounds(unit: &Unit, t: &Tuning) -> Result<(), EnvironmentError> {
    if unit.kind >= t.kinds.kinds.len()
        || ![unit.x, unit.z, unit.rot, unit.hp]
            .iter()
            .all(|v| v.is_finite())
        || unit.x.abs() > 1_073_741_824.
        || unit.z.abs() > 1_073_741_824.
    {
        return Err(EnvironmentError::InvalidInput("detached unit"));
    }
    Ok(())
}
fn section_bounds(
    w: &World,
    structure: &crate::world::WorldStructure,
) -> Result<(), EnvironmentError> {
    if structure.section_ids.len() != structure.cells.len()
        || structure.cells.iter().any(|c| *c >= w.game.chars.len())
        || structure.section_ids.iter().any(|id| {
            w.section_by_id(id).is_none_or(|(m, s)| {
                !s.hp.is_finite() || s.hp < 0. || m.failure.impulse.is_some_and(|v| !v.is_finite())
            })
        })
    {
        return Err(EnvironmentError::InvalidInput("building sections"));
    }
    if !section_hp(w, structure).is_finite() {
        return Err(EnvironmentError::InvalidInput("building section total"));
    }
    Ok(())
}
fn section_hp(w: &World, structure: &crate::world::WorldStructure) -> f64 {
    structure
        .section_ids
        .iter()
        .fold(0., |sum, id| sum + w.section_by_id(id).unwrap().1.hp)
}
fn body_bounds(body: &DebrisBody) -> Result<(), EnvironmentError> {
    if ![
        body.x,
        body.y,
        body.z,
        body.vx,
        body.vy,
        body.vz,
        body.dir,
        body.mass,
        body.radius,
        body.origin.x,
        body.origin.y,
        body.origin.z,
        body.gravity,
        body.drag,
        body.stamp,
        body.born,
        body.tilt,
    ]
    .iter()
    .all(|v| v.is_finite())
        || body.kind != DebrisKind::Wreck
        || !(0.0..=2.2).contains(&body.radius)
        || body.gravity != 9.81
        || body.drag != 3.2
        || body.x.abs() > 1_073_741_824.
        || body.z.abs() > 1_073_741_824.
        || body.y.abs() > 1_073_741_824.
        || body.origin.x.abs() > 1_073_741_824.
        || body.origin.z.abs() > 1_073_741_824.
        || body.vx.abs() > 2.4
        || body.vz.abs() > 2.4
        || body.vy.abs() > 32.
        || body.stamp < body.born
        || body.stamp - body.born > 2.6
        || body.contacts > DEBRIS_LIMITS.contacts
        || body.settled
    {
        return Err(EnvironmentError::InvalidInput("wreck motion"));
    }
    Ok(())
}
fn wreck_limit() -> Result<usize, EnvironmentError> {
    let limit = Catalog::compiled()
        .map_err(|_| EnvironmentError::Rules)?
        .table("CFG")
        .map_err(|_| EnvironmentError::Rules)?
        .get("wrecks")
        .and_then(Value::number)
        .filter(|v| v.is_finite() && *v >= 1. && *v <= 1024. && v.fract() == 0.)
        .ok_or(EnvironmentError::Rules)?;
    Ok(limit as usize)
}
fn finish_wreck_overflow(
    w: &World,
    body: &mut DebrisBody,
    ctx: &Context<'_>,
    t: &'static Tuning,
) -> Result<(), EnvironmentError> {
    let mut env = WorldDebris {
        world: w,
        grid: ctx.grid,
        tuning: t,
    };
    for _ in 0..=50 {
        if body.settled {
            break;
        }
        body.step(0.05, &mut env)
            .map_err(|_| EnvironmentError::InvalidInput("wreck environment"))?;
    }
    body.settle(&mut env)
        .map_err(|_| EnvironmentError::InvalidInput("wreck environment"))
}
