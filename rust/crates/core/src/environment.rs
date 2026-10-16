//! Authoritative terrain damage, fire, flooding and structural collapse.
//! Callers supply source unit order and the shared environment seed/ID allocator.
use crate::{
    catalog::{Catalog, Value},
    environment_debris::{DebrisBody, DebrisInput, DebrisKind, section_mass},
    game::{MOVE, Point, Smoke},
    math,
    rules::Rules,
    world::{Appearance, CellChange, World, WorldError},
};
#[path = "environment_lifecycle.rs"]
mod lifecycle;
pub use lifecycle::WreckMetadata;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EnvironmentError {
    InvalidInput(&'static str),
    World(WorldError),
    Rules,
    Capacity,
}
impl From<WorldError> for EnvironmentError {
    fn from(e: WorldError) -> Self {
        Self::World(e)
    }
}
impl std::fmt::Display for EnvironmentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for EnvironmentError {}
/// Shared match fields, borrowed for the duration of an operation. Unit order is the source
/// collection order; it must name every current unit exactly once. No RNG stream is created.
pub struct Context<'a> {
    pub tick: u64,
    pub seed: &'a mut u32,
    pub next_id: &'a mut u32,
    pub unit_order: &'a [u32],
    /// The maintained match grid preserves bucket update order for debris contact ties.
    pub grid: &'a crate::grid::SpatialGrid,
    pub rain: f64,
    pub wet: f64,
    pub wind_angle: f64,
    pub wind_strength: f64,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Hurt {
    pub unit: u32,
    pub owner: usize,
    pub at: Point,
    pub killed: bool,
    pub from: Option<i32>,
    pub local: bool,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Collapse {
    pub id: String,
    pub at: f64,
    pub position: Point,
    pub section: Option<String>,
    pub impulse: Option<f64>,
    pub direction: Option<f64>,
    pub material: Option<String>,
    pub entity: Option<u32>,
    pub owner: Option<usize>,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Effect {
    Hurt(Hurt),
    Collapse(Collapse),
    BuildingCollapse {
        unit: u32,
        owner: usize,
        position: Point,
    },
    LastHit {
        unit: u32,
        owner: i32,
    },
}
impl Effect {
    pub fn to_json(&self) -> serde_json::Value {
        use serde_json::json;
        match self {
            Self::BuildingCollapse {
                unit,
                owner,
                position,
            } => {
                json!({"k":"collapse","t":unit,"to":crate::game::source_owner_json(*owner),"x":position.x,"z":position.z})
            }
            Self::LastHit { unit, owner } => json!({"lastHit":[unit,owner]}),
            Self::Hurt(h) => {
                let mut v = json!({"t":h.unit,"to":crate::game::source_owner_json(h.owner),"x":h.at.x,"z":h.at.z,"k":"hurt","kill":h.killed});
                if let Some(o) = h.from {
                    v["fo"] = json!(o);
                }
                if h.local {
                    v["local"] = json!(true);
                }
                v
            }
            Self::Collapse(c) => {
                let mut v =
                    json!({"k":"collapse","id":c.id,"at":c.at,"x":c.position.x,"z":c.position.z});
                if let Some(s) = &c.section {
                    v["section"] = json!(s);
                }
                if let Some(i) = c.impulse {
                    v["impulse"] = json!(i);
                }
                if let Some(d) = c.direction {
                    v["dir"] = json!(d);
                }
                if let Some(m) = &c.material {
                    v["material"] = json!(m);
                }
                if let Some(t) = c.entity {
                    v["t"] = json!(t);
                }
                if let Some(o) = c.owner {
                    v["to"] = crate::game::source_owner_json(o);
                }
                v
            }
        }
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DamageMemory {
    pub cells: BTreeSet<usize>,
    pub buildings: BTreeSet<u32>,
}
/// One circular terrain hit. Radii are metres; scar is the source ground-sinking multiplier.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Blast {
    pub at: Point,
    pub radius: f64,
    pub damage: f64,
    pub scar: f64,
}
/// A caller can reuse memory for overlapping circles in one beam/breath.
pub struct DamageContext<'a> {
    /// Captured source list for casualty and legacy rubble loops. None uses all current units.
    pub actor_order: Option<&'a [u32]>,
    pub owner: Option<i32>,
    pub contact_id: Option<&'a str>,
    pub direction: Option<Point>,
    pub debit_entity: bool,
    pub skip_buildings: &'a BTreeSet<u32>,
    pub memory: Option<&'a mut DamageMemory>,
}
impl<'a> DamageContext<'a> {
    pub fn plain(skip_buildings: &'a BTreeSet<u32>) -> Self {
        Self {
            actor_order: None,
            owner: None,
            contact_id: None,
            direction: None,
            debit_entity: true,
            skip_buildings,
            memory: None,
        }
    }
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FallingSection {
    pub body: DebrisBody,
    pub section: String,
    pub cell: usize,
    pub bridge: bool,
    pub at: f64,
    pub until: u64,
    pub impulse: f64,
}
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct EnvironmentRuntime {
    #[cfg(test)]
    #[serde(skip)]
    pub(crate) replay: Option<crate::aligned_replay::Replay>,
    contacts: Vec<String>,
    contact_members: BTreeSet<String>,
    falling_sections: Vec<FallingSection>,
    pub(crate) wreck_metadata: BTreeMap<u32, WreckMetadata>,
}
impl EnvironmentRuntime {
    pub(crate) fn cancel_structure_sections(&mut self, ids: &BTreeSet<String>) {
        self.falling_sections
            .retain(|section| !ids.contains(&section.section));
    }
    pub fn contacts(&self) -> &[String] {
        &self.contacts
    }
    pub fn falling_sections(&self) -> &[FallingSection] {
        &self.falling_sections
    }
}
struct Tuning {
    kinds: Rules,
    burn: BTreeMap<u8, f64>,
    spread: BTreeMap<u8, f64>,
    ignite: BTreeMap<u8, f64>,
    wreck: BTreeMap<u8, u8>,
    inf: f64,
    supp: f64,
    smoke_r: f64,
    smoke_t: f64,
    trench_hp: f64,
    scar_hit: f64,
    min_level: i8,
    level_height: f64,
    evict: f64,
    rubble_spill: f64,
    unit_hp: Vec<f64>,
    flight: BTreeMap<u8, f64>,
}
fn tuning() -> Result<&'static Tuning, EnvironmentError> {
    static T: OnceLock<Result<Tuning, EnvironmentError>> = OnceLock::new();
    T.get_or_init(|| {
        let catalog = Catalog::compiled().map_err(|_| EnvironmentError::Rules)?;
        let cfg = catalog.table("CFG").map_err(|_| EnvironmentError::Rules)?;
        let num = |v: &Value, k: &str| {
            v.get(k)
                .and_then(Value::number)
                .filter(|v| v.is_finite())
                .ok_or(EnvironmentError::Rules)
        };
        fn field<'a>(v: &'a Value, k: &str) -> Result<&'a Value, EnvironmentError> {
            v.get(k).ok_or(EnvironmentError::Rules)
        }
        let nums = |v: &Value| -> Result<BTreeMap<u8, f64>, EnvironmentError> {
            v.object()
                .ok_or(EnvironmentError::Rules)?
                .iter()
                .map(|(k, v)| {
                    Ok((
                        *k.as_bytes().first().ok_or(EnvironmentError::Rules)?,
                        v.number().ok_or(EnvironmentError::Rules)?,
                    ))
                })
                .collect()
        };
        let fire = field(cfg, "fire")?;
        let smoke = field(fire, "smoke")?;
        let kinds = Rules::compiled().map_err(|_| EnvironmentError::Rules)?;
        let units = catalog
            .table("UNITS")
            .map_err(|_| EnvironmentError::Rules)?;
        let unit_hp = kinds
            .kinds
            .iter()
            .map(|k| {
                let v = field(units, &k.name)?;
                Ok(num(v, "models")? * num(v, "hpPer")?)
            })
            .collect::<Result<Vec<_>, EnvironmentError>>()?;
        let wreck = field(cfg, "wreck")?
            .object()
            .ok_or(EnvironmentError::Rules)?
            .iter()
            .map(|(k, v)| {
                Ok((
                    k.as_bytes()[0],
                    v.string().ok_or(EnvironmentError::Rules)?.as_bytes()[0],
                ))
            })
            .collect::<Result<_, EnvironmentError>>()?;
        Ok(Tuning {
            kinds,
            burn: nums(field(fire, "burn")?)?,
            spread: nums(field(fire, "spread")?)?,
            ignite: nums(field(fire, "ignite")?)?,
            wreck,
            inf: num(fire, "inf")?,
            supp: num(fire, "supp")?,
            smoke_r: num(smoke, "r")?,
            smoke_t: num(smoke, "t")?,
            trench_hp: num(field(cfg, "trench")?, "hp")?,
            scar_hit: num(field(cfg, "scar")?, "hit")?,
            min_level: num(cfg, "minLevel")? as i8,
            level_height: num(cfg, "levelHeight")?,
            evict: num(cfg, "garrisonEvictDamage")?,
            rubble_spill: num(cfg, "rubbleSpill")?,
            unit_hp,
            flight: BTreeMap::from([
                (b'B', 5.),
                (b'K', 5.),
                (b'H', 1.8),
                (b'#', 1.2),
                (b'Y', 1.2),
                (b'Q', 1.6),
                (b'R', 0.8),
                (b'O', 0.8),
                (b'=', 0.25),
            ]),
        })
    })
    .as_ref()
    .map_err(Clone::clone)
}
fn material(name: &str) -> (f64, f64, f64, f64) {
    // crater, fire, absorption, rebound from world-layers.js
    match name {
        "road" => (0.5, 0., 0.65, 0.12),
        "wood" => (0., 1., 0.8, 0.05),
        "stone" => (0., 0.4, 0.3, 0.55),
        "concrete" => (0., 0.2, 0.25, 0.65),
        "steel" => (0., 0., 0.15, 0.75),
        "water" => (0., 0., 1., 0.),
        _ => (1., 0., 0.9, 0.02),
    }
}
fn check_damage_actors(w: &World, hit: &DamageContext<'_>) -> Result<(), EnvironmentError> {
    if let Some(ids) = hit.actor_order {
        let unique: BTreeSet<_> = ids.iter().copied().collect();
        if unique.len() != ids.len() || ids.iter().any(|id| !w.game.units.contains_key(id)) {
            return Err(EnvironmentError::InvalidInput("damage actor membership"));
        }
    }
    Ok(())
}
fn center(w: &World, c: usize) -> Point {
    Point {
        x: (c % w.game.w) as f64 * 2. + 1.,
        z: (c / w.game.w) as f64 * 2. + 1.,
    }
}
fn level(w: &World, c: usize) -> i8 {
    w.game.height.as_ref().map_or(0, |h| h[c])
}
fn near(w: &World, c: usize) -> [Option<usize>; 4] {
    let n = w.game.chars.len();
    let width = w.game.w;
    [
        c.checked_sub(width),
        (c + width < n).then_some(c + width),
        c.checked_sub(1).filter(|_| !c.is_multiple_of(width)),
        (c % width + 1 < width).then_some(c + 1),
    ]
}
// World construction establishes these shapes. Public terrain arrays can later be
// replaced, so every environment admission rechecks them before indexing or writes.
fn check_terrain(w: &World) -> Result<(), EnvironmentError> {
    let g = &w.game;
    let n =
        g.w.checked_mul(g.h)
            .ok_or(EnvironmentError::InvalidInput("terrain shape"))?;
    if g.w == 0
        || g.h == 0
        || g.w > (1usize << 26)
        || g.h > (1usize << 26)
        || [
            g.chars.len(),
            g.flags.len(),
            g.house.len(),
            g.cell_hp.len(),
            w.mine_layer.len(),
            w.surface_wear.len(),
            w.burnt.len(),
            w.scar.len(),
            w.cell_state.len(),
        ]
        .iter()
        .any(|len| *len != n)
        || g.ground.as_ref().is_none_or(|v| v.len() != n)
        || g.objects.as_ref().is_none_or(|v| v.len() != n)
        || g.wear.as_ref().is_none_or(|v| v.len() != n)
        || g.height.as_ref().is_some_and(|v| v.len() != n)
        || g.trench_front.as_ref().is_some_and(|v| v.len() != n)
        || g.tough.as_ref().is_some_and(|v| v.len() != n)
        || w.shoal.as_ref().is_some_and(|v| v.len() != n)
    {
        return Err(EnvironmentError::InvalidInput("terrain shape"));
    }
    Ok(())
}
fn check(w: &World, ctx: &Context<'_>) -> Result<&'static Tuning, EnvironmentError> {
    check_terrain(w)?;
    if ctx.tick > 9_007_199_254_740_941 || ctx.tick.checked_add(50).is_none() {
        return Err(EnvironmentError::InvalidInput("tick"));
    }
    if ![ctx.rain, ctx.wet, ctx.wind_angle, ctx.wind_strength]
        .iter()
        .all(|v| v.is_finite())
    {
        return Err(EnvironmentError::InvalidInput("environment"));
    }
    if !(0.0..=1.0).contains(&ctx.rain) || !(0.0..=1.0).contains(&ctx.wet) {
        return Err(EnvironmentError::InvalidInput("rain/wet"));
    }
    let ids: BTreeSet<_> = ctx.unit_order.iter().copied().collect();
    if ids.len() != ctx.unit_order.len()
        || ids.len() != w.game.units.len()
        || ids.iter().any(|id| !w.game.units.contains_key(id))
    {
        return Err(EnvironmentError::InvalidInput("unit order"));
    }
    let t = tuning()?;
    if w.game.units.values().any(|u| {
        u.kind >= t.kinds.kinds.len()
            || ![u.x, u.z, u.hp, u.supp].iter().all(|v| v.is_finite())
            || u.x.abs() > 1_073_741_824.
            || u.z.abs() > 1_073_741_824.
            || u.garrison.is_some_and(|c| c >= w.game.chars.len())
            || u.path_gen == u64::MAX
    }) {
        return Err(EnvironmentError::InvalidInput("unit"));
    }
    if w.game
        .smokes
        .iter()
        .any(|s| ![s.x, s.z, s.r].iter().all(|v| v.is_finite()))
    {
        return Err(EnvironmentError::InvalidInput("smoke"));
    }
    if w.fires
        .iter()
        .any(|(cell, timer)| *cell >= w.game.chars.len() || !timer.is_finite())
    {
        return Err(EnvironmentError::InvalidInput("fire timer"));
    }
    let count = w.game.chars.len() as u64 * 32 + 4096;
    w.mutation_capacity(count)?;
    if u64::from(*ctx.next_id) + w.game.chars.len() as u64 > u64::from(u32::MAX) {
        return Err(EnvironmentError::Capacity);
    }
    Ok(t)
}
fn hurt(w: &World, id: u32, from: Option<i32>, local: bool, at: Option<Point>) -> Effect {
    let u = &w.game.units[&id];
    Effect::Hurt(Hurt {
        unit: id,
        owner: u.owner,
        at: at.unwrap_or(u.at()),
        killed: u.hp <= 0.,
        from,
        local,
    })
}
fn flammable(w: &World, c: usize, wet: f64) -> bool {
    let ch = w.game.physical(c);
    !w.fires.contains_key(&c)
        && (b"HBO".contains(&ch)
            || ch == b'.'
                && w.burnt[c] == 0
                && w.game.wear.as_ref().unwrap()[c] < 0.5
                && wet < 0.35)
}
impl EnvironmentRuntime {
    /// Validate shared producers and reserve headroom before a larger composed transaction.
    pub fn validate_context(
        &self,
        world: &World,
        ctx: &Context<'_>,
    ) -> Result<(), EnvironmentError> {
        check(world, ctx).map(|_| ())
    }
    pub fn ignite(
        &mut self,
        w: &mut World,
        c: usize,
        ctx: &mut Context<'_>,
    ) -> Result<(), EnvironmentError> {
        let t = check(w, ctx)?;
        if c >= w.game.chars.len() {
            return Err(EnvironmentError::InvalidInput("cell"));
        }
        self.ignite_inner(w, c, ctx, t)
    }
    fn ignite_inner(
        &mut self,
        w: &mut World,
        c: usize,
        ctx: &mut Context<'_>,
        t: &Tuning,
    ) -> Result<(), EnvironmentError> {
        if !flammable(w, c, ctx.wet) {
            return Ok(());
        }
        w.fires.insert(
            c,
            *t.burn.get(&w.game.physical(c)).unwrap_or(&t.burn[&b'B']),
        );
        if w.game.sections.get(&(c as u32)).is_some_and(|s| !s.failed) {
            w.sections.get_mut(&c).unwrap().appearance = Appearance::Burning;
            w.log_cell(c, false)?;
        }
        let at = center(w, c);
        if w.game.chars[c] != b'.'
            && !w
                .game
                .smokes
                .iter()
                .any(|s| math::hypot(s.x - at.x, s.z - at.z) < t.smoke_r * 1.5)
        {
            w.game.smokes.push(Smoke {
                id: *ctx.next_id,
                x: at.x,
                z: at.z,
                r: t.smoke_r,
                remaining: Some(t.smoke_t),
            });
            *ctx.next_id += 1;
        }
        Ok(())
    }
    pub fn damage_section(
        &mut self,
        w: &mut World,
        c: usize,
        damage: f64,
        ctx: &mut Context<'_>,
        hit: &DamageContext<'_>,
    ) -> Result<(f64, Vec<Effect>), EnvironmentError> {
        let mut out = Vec::new();
        let taken =
            self.damage_section_emitting(w, c, damage, ctx, hit, &mut |effect| out.push(effect))?;
        Ok((taken, out))
    }
    /// Emit each committed effect in source order, including the prefix before an error.
    pub fn damage_section_emitting(
        &mut self,
        w: &mut World,
        c: usize,
        damage: f64,
        ctx: &mut Context<'_>,
        hit: &DamageContext<'_>,
        out: &mut dyn FnMut(Effect),
    ) -> Result<f64, EnvironmentError> {
        check(w, ctx)?;
        check_damage_actors(w, hit)?;
        if c >= w.game.chars.len() || damage.is_nan() || damage == f64::NEG_INFINITY {
            return Err(EnvironmentError::InvalidInput("section damage"));
        }
        if hit
            .direction
            .is_some_and(|p| !p.x.is_finite() || !p.z.is_finite())
        {
            return Err(EnvironmentError::InvalidInput("direction"));
        }
        self.damage_section_inner(w, c, damage, ctx, hit, out)
    }
    fn damage_section_inner(
        &mut self,
        w: &mut World,
        c: usize,
        damage: f64,
        ctx: &mut Context<'_>,
        hit: &DamageContext<'_>,
        out: &mut dyn FnMut(Effect),
    ) -> Result<f64, EnvironmentError> {
        if damage <= 0. || w.game.sections.get(&(c as u32)).is_none_or(|s| s.failed) {
            return Ok(0.);
        }
        let meta = w.sections[&c].clone();
        if let Some(contact) = hit.contact_id {
            let key = format!("{contact}:{}", meta.id);
            if self.contact_members.contains(&key) {
                return Ok(0.);
            }
            self.contact_members.insert(key.clone());
            self.contacts.push(key);
            if self.contacts.len() > 2048 {
                self.contact_members.remove(&self.contacts.remove(0));
            }
        }
        let structure = w
            .structures
            .iter()
            .find(|s| s.id == meta.structure_id)
            .unwrap()
            .clone();
        let entity = structure
            .entity_id
            .filter(|id| w.game.units.contains_key(id));
        let debit = entity.map_or(f64::INFINITY, |id| {
            if hit.debit_entity {
                math::max(0., w.game.units[&id].hp)
            } else {
                f64::INFINITY
            }
        });
        let taken = math::min(math::min(w.game.sections[&(c as u32)].hp, damage), debit);
        let impulse = math::min(
            4.,
            math::max(
                0.25,
                damage
                    / match meta.material.as_str() {
                        "wood" => 180.,
                        "stone" => 400.,
                        "concrete" => 500.,
                        _ => 300.,
                    },
            ),
        );
        w.sections.get_mut(&c).unwrap().failure.impulse = Some(impulse);
        let section = w.game.sections.get_mut(&(c as u32)).unwrap();
        section.hp -= taken;
        let hp = section.hp;
        if let Some(id) = entity {
            let u = w.game.units.get_mut(&id).unwrap();
            if hit.debit_entity {
                u.hp -= taken;
            }
            if let Some(owner) = hit.owner.filter(|o| *o >= 0) {
                out(Effect::LastHit { unit: id, owner });
            }
        }
        w.game.cell_hp[c] = hp as f32;
        if let Some(id) = entity.filter(|_| taken > 0.) {
            out(hurt(
                w,
                id,
                hit.owner,
                hit.contact_id.is_some(),
                Some(center(w, c)),
            ));
        }
        if hp > 0. {
            w.sections.get_mut(&c).unwrap().appearance = if w.fires.contains_key(&c) {
                Appearance::Burning
            } else {
                Appearance::Damaged
            };
            w.touch(c)?;
            w.log_cell(c, true)?;
            return Ok(taken);
        }
        self.fail_section(w, &meta.id, c, ctx, hit.direction, hit.actor_order, out)?;
        let supported = self.supported(w, &structure.section_ids);
        for (id, &cell) in structure.section_ids.iter().zip(&structure.cells) {
            let (m, s) = w.section_by_id(id).unwrap();
            if s.failed || supported.contains(id) {
                continue;
            }
            let other_hp = s.hp;
            let material_same = m.material == meta.material;
            if let Some(entity) = entity {
                let u = w.game.units.get_mut(&entity).unwrap();
                u.hp -= math::min(u.hp, other_hp);
            }
            w.section_by_id_mut(id).unwrap().0.failure.impulse = Some(math::min(
                4.,
                impulse * if material_same { 1. } else { 0.7 },
            ));
            self.fail_section(w, id, cell, ctx, hit.direction, hit.actor_order, out)?;
        }
        if let Some(entity) = entity {
            let alive = structure.section_ids.iter().any(|id| {
                w.section_by_id(id)
                    .is_some_and(|(_, s)| !s.failed && s.hp > 0.)
            });
            if !alive || w.game.units[&entity].hp <= 0. {
                w.game.units.get_mut(&entity).unwrap().hp = 0.;
            }
        }
        Ok(taken)
    }
    fn supported(&self, w: &World, ids: &[String]) -> BTreeSet<String> {
        let live: BTreeSet<_> = ids
            .iter()
            .filter(|id| {
                w.section_by_id(id)
                    .is_some_and(|(_, s)| !s.failed && s.hp > 0.)
            })
            .cloned()
            .collect();
        let mut links: BTreeMap<&str, Vec<&str>> =
            live.iter().map(|id| (id.as_str(), Vec::new())).collect();
        for id in ids.iter().filter(|id| live.contains(*id)) {
            let m = w.section_by_id(id).unwrap().0;
            for to in m.supports.iter().filter(|to| live.contains(*to)) {
                if !links[id.as_str()].contains(&to.as_str()) {
                    links.get_mut(id.as_str()).unwrap().push(to);
                }
                if !links[to.as_str()].contains(&id.as_str()) {
                    links.get_mut(to.as_str()).unwrap().push(id);
                }
            }
        }
        let mut seen = BTreeSet::new();
        let mut queue = Vec::new();
        for id in ids {
            if live.contains(id) && w.section_by_id(id).unwrap().0.anchor {
                seen.insert(id.clone());
                queue.push(id.as_str());
            }
        }
        let mut i = 0;
        while i < queue.len() {
            for &to in &links[queue[i]] {
                if seen.insert(to.to_owned()) {
                    queue.push(to);
                }
            }
            i += 1;
        }
        seen
    }
    #[allow(clippy::too_many_arguments)]
    fn fail_section(
        &mut self,
        w: &mut World,
        id: &str,
        c: usize,
        ctx: &mut Context<'_>,
        direction: Option<Point>,
        actor_order: Option<&[u32]>,
        out: &mut dyn FnMut(Effect),
    ) -> Result<(), EnvironmentError> {
        if w.section_by_id(id).unwrap().1.failed {
            return Ok(());
        }
        let t = tuning()?;
        let was = w.game.chars[c];
        let at = center(w, c);
        let (metadata, state) = w.section_by_id_mut(id).unwrap();
        state.failed = true;
        state.hp = 0.;
        metadata.failure.failed_at = Some(ctx.tick as f64 * 0.05);
        metadata.rubble.clear();
        let d = direction.unwrap_or(Point { x: 0., z: 0. });
        let mag = math::hypot(d.x, d.z);
        let dir = if mag != 0. {
            Point {
                x: d.x / mag,
                z: d.z / mag,
            }
        } else {
            Point { x: 0., z: 1. }
        };
        metadata.failure.direction = Some(dir);
        let meta = metadata.clone();
        w.fires.remove(&c);
        w.mutate_cell(
            c,
            CellChange {
                object: Some(b'.'),
                ..Default::default()
            },
        )?;
        w.game.cell_hp[c] = 0.;
        let structure = w
            .structures
            .iter()
            .find(|s| s.id == meta.structure_id)
            .unwrap();
        let entity = structure.entity_id;
        let impulse = meta.failure.impulse.unwrap_or(1.);
        let rotation = math::atan2(dir.z, dir.x);
        let collapse_id = format!("collapse:{id}:{}", ctx.tick);
        out(Effect::Collapse(Collapse {
            id: collapse_id.clone(),
            at: ctx.tick as f64 * 0.05,
            position: at,
            section: Some(id.to_owned()),
            impulse: Some(impulse),
            direction: Some(rotation),
            material: Some(meta.material.clone()),
            entity,
            owner: entity.and_then(|id| w.game.units.get(&id).map(|u| u.owner)),
        }));
        let body = DebrisBody::new(DebrisInput {
            id: collapse_id,
            kind: DebrisKind::Section,
            x: at.x,
            y: f64::from(level(w, c)) * t.level_height + 1.6,
            z: at.z,
            dir: rotation,
            impulse,
            mass: section_mass(&meta.material),
            material: meta.material.clone(),
            radius: 0.65,
            time: ctx.tick as f64 * 0.05,
        })
        .map_err(|_| EnvironmentError::InvalidInput("debris"))?;
        let falling = FallingSection {
            body,
            section: id.to_owned(),
            cell: c,
            bridge: structure.kind == "bridge",
            at: ctx.tick as f64 * 0.05,
            until: ctx.tick + 50,
            impulse,
        };
        if self.falling_sections.len() + w.game.wrecks.iter().filter(|w| w.moving).count() < 96 {
            self.falling_sections.push(falling);
        } else {
            self.finish_overflow(w, falling, ctx)?;
        }
        for &uid in ctx.unit_order {
            if w.game.units[&uid].garrison == Some(c) {
                let u = w.game.units.get_mut(&uid).unwrap();
                u.garrison = None;
                u.hp -= t.unit_hp[u.kind] * t.evict;
                out(hurt(w, uid, None, false, None));
            }
        }
        if was == b'=' {
            for &uid in actor_order.unwrap_or(ctx.unit_order) {
                let u = &w.game.units[&uid];
                if u.hp > 0.
                    && u.air.is_none()
                    && !u.riding
                    && !t.kinds.kinds[u.kind].naval
                    && w.game.cell_of(u.x, u.z) == Some(c)
                {
                    w.game.units.get_mut(&uid).unwrap().hp = 0.;
                    out(hurt(w, uid, None, false, None));
                }
            }
        }
        w.log_cell(c, false)?;
        Ok(())
    }
    pub fn wreck_cell(
        &mut self,
        w: &mut World,
        c: usize,
        into: Option<u8>,
        ctx: &mut Context<'_>,
    ) -> Result<Vec<Effect>, EnvironmentError> {
        let mut out = Vec::new();
        self.wreck_cell_emitting(w, c, into, ctx, &mut |effect| out.push(effect))?;
        Ok(out)
    }
    /// Emit each committed effect in source order, including the prefix before an error.
    pub fn wreck_cell_emitting(
        &mut self,
        w: &mut World,
        c: usize,
        into: Option<u8>,
        ctx: &mut Context<'_>,
        out: &mut dyn FnMut(Effect),
    ) -> Result<(), EnvironmentError> {
        check(w, ctx)?;
        if c >= w.game.chars.len() || into.is_some_and(|ch| !b".D+MWFBH#TXY=RNAQKO".contains(&ch)) {
            return Err(EnvironmentError::InvalidInput("wreck cell"));
        }
        self.wreck_inner(w, c, into, ctx, None, out)
    }
    fn wreck_inner(
        &mut self,
        w: &mut World,
        c: usize,
        into: Option<u8>,
        ctx: &mut Context<'_>,
        actor_order: Option<&[u32]>,
        out: &mut dyn FnMut(Effect),
    ) -> Result<(), EnvironmentError> {
        if w.game.sections.get(&(c as u32)).is_some_and(|s| !s.failed) {
            let skip = BTreeSet::new();
            let mut hit = DamageContext::plain(&skip);
            hit.actor_order = actor_order;
            self.damage_section_inner(w, c, f64::INFINITY, ctx, &hit, out)?;
            return Ok(());
        }
        let t = tuning()?;
        let was = w.game.physical(c);
        let Some(into) = into.or_else(|| t.wreck.get(&was).copied()) else {
            return Ok(());
        };
        let at = center(w, c);
        w.mutate_cell(
            c,
            if b".D+MWF".contains(&into) {
                CellChange {
                    ground: Some(into),
                    object: Some(b'.'),
                    ..Default::default()
                }
            } else {
                CellChange {
                    object: Some(into),
                    ..Default::default()
                }
            },
        )?;
        out(Effect::Collapse(Collapse {
            id: format!("collapse:{}:{c}", ctx.tick),
            at: ctx.tick as f64 * 0.05,
            position: at,
            section: None,
            impulse: None,
            direction: None,
            material: None,
            entity: None,
            owner: None,
        }));
        if was == b'B' {
            let n = near(w, c);
            for next in [n[2], n[3], n[0], n[1]].into_iter().flatten() {
                if !b".D+M".contains(&w.game.chars[next])
                    || aligned_environment_draw!(
                        ctx.seed,
                        self.replay.as_ref(),
                        serde_json::json!(["environment.rubble", ctx.tick, c, next])
                    ) >= t.rubble_spill
                {
                    continue;
                }
                if actor_order.unwrap_or(ctx.unit_order).iter().any(|id| {
                    let u = &w.game.units[id];
                    u.hp > 0.
                        && u.air.is_none()
                        && !t.kinds.kinds[u.kind].infantry
                        && w.game.cell_of(u.x, u.z) == Some(next)
                }) {
                    continue;
                }
                w.set_cell(next, b'R')?;
            }
        }
        for &id in ctx.unit_order {
            if w.game.units[&id].garrison == Some(c) {
                let u = w.game.units.get_mut(&id).unwrap();
                u.garrison = None;
                u.hp -= t.unit_hp[u.kind] * t.evict;
                out(hurt(w, id, None, false, None));
            }
        }
        if crate::map::flags(into) & MOVE != 0 {
            for &id in actor_order.unwrap_or(ctx.unit_order) {
                let u = &w.game.units[&id];
                if u.hp > 0. && u.air.is_none() && w.game.cell_of(u.x, u.z) == Some(c) {
                    w.game.units.get_mut(&id).unwrap().hp = 0.;
                    out(hurt(w, id, None, false, None));
                }
            }
        }
        Ok(())
    }
    pub fn damage_cells(
        &mut self,
        w: &mut World,
        blast: Blast,
        ctx: &mut Context<'_>,
        hit: &mut DamageContext<'_>,
    ) -> Result<Vec<Effect>, EnvironmentError> {
        let mut out = Vec::new();
        self.damage_cells_emitting(w, blast, ctx, hit, &mut |effect| out.push(effect))?;
        Ok(out)
    }
    /// Emit each committed effect in source order, including the prefix before an error.
    pub fn damage_cells_emitting(
        &mut self,
        w: &mut World,
        blast: Blast,
        ctx: &mut Context<'_>,
        hit: &mut DamageContext<'_>,
        out: &mut dyn FnMut(Effect),
    ) -> Result<(), EnvironmentError> {
        let t = check(w, ctx)?;
        check_damage_actors(w, hit)?;
        let Blast {
            at,
            radius,
            damage,
            scar,
        } = blast;
        if ![at.x, at.z, radius, damage, scar]
            .iter()
            .all(|v| v.is_finite())
            || radius < 0.
            || damage < 0.
            || scar < 0.
            || radius > 4096.
            || damage > 100000.
            || scar > 100000.
            || at.x.abs() > 1_073_741_824.
            || at.z.abs() > 1_073_741_824.
        {
            return Err(EnvironmentError::InvalidInput("blast"));
        }
        if hit
            .direction
            .is_some_and(|p| !p.x.is_finite() || !p.z.is_finite())
        {
            return Err(EnvironmentError::InvalidInput("direction"));
        }
        let r = (radius / 2.).ceil() as i64;
        let x0 = (at.x / 2.).floor() as i64;
        let y0 = (at.z / 2.).floor() as i64;
        let mut buildings: Vec<(u32, Vec<(usize, f64)>)> = Vec::new();
        for y in (y0 - r).max(0)..=(y0 + r).min(w.game.h as i64 - 1) {
            for x in (x0 - r).max(0)..=(x0 + r).min(w.game.w as i64 - 1) {
                let c = y as usize * w.game.w + x as usize;
                let p = center(w, c);
                let d = math::hypot(p.x - at.x, p.z - at.z);
                if d > radius + 1. {
                    continue;
                }
                if let Some(memory) = hit.memory.as_mut()
                    && !memory.cells.insert(c)
                {
                    continue;
                }
                let amount = damage * (1. - math::min(1., d / (radius + 2.)) * 0.5);
                let ch = w.game.physical(c);
                let ground = w.game.ground.as_ref().unwrap()[c];
                if w.mine_layer[c] != 0 && amount > 0. {
                    w.mutate_cell(
                        c,
                        CellChange {
                            mine: Some(false),
                            ..Default::default()
                        },
                    )?;
                }
                if ground == b'D' || ground == b'+' {
                    if w.game.objects.as_ref().unwrap()[c] != b'T' {
                        w.surface_wear[c] = w.game.wear.as_ref().unwrap()[c];
                    }
                    w.surface_wear[c] = math::min(
                        1.,
                        f64::from(w.surface_wear[c])
                            + amount / if ground == b'D' { 200. } else { 500. },
                    ) as f32;
                    if w.game.objects.as_ref().unwrap()[c] != b'T' {
                        w.game.wear.as_mut().unwrap()[c] = w.surface_wear[c];
                    }
                    if ground == b'D' && w.surface_wear[c] >= 1. {
                        w.mutate_cell(
                            c,
                            CellChange {
                                ground: Some(b'+'),
                                ..Default::default()
                            },
                        )?;
                    } else {
                        w.touch(c)?;
                    }
                }
                if scar != 0. && b".+D".contains(&ground) {
                    let crater = material(w.material_name(c, true).unwrap()).0;
                    let next_scar = math::min(
                        1.,
                        f64::from(w.scar[c]) + amount * scar * crater / t.scar_hit,
                    );
                    w.scar[c] = next_scar as f32;
                    if next_scar >= 1. {
                        let was = level(w, c);
                        self.dent(w, c, t)?;
                        if level(w, c) < was {
                            w.scar[c] = 0.;
                            if ground == b'.' {
                                w.mutate_cell(
                                    c,
                                    CellChange {
                                        ground: Some(b'+'),
                                        ..Default::default()
                                    },
                                )?;
                            }
                        }
                    } else {
                        w.touch(c)?;
                    }
                }
                if ch == b'T' {
                    let next_wear =
                        f64::from(w.game.wear.as_ref().unwrap()[c]) + amount / t.trench_hp;
                    w.game.wear.as_mut().unwrap()[c] = next_wear as f32;
                    if next_wear >= 1. {
                        self.wreck_inner(w, c, Some(b'+'), ctx, hit.actor_order, out)?;
                        continue;
                    }
                }
                if damage >= 60.
                    && t.ignite.get(&ch).is_some_and(|p| *p != 0.)
                    && aligned_environment_draw!(
                        ctx.seed,
                        self.replay.as_ref(),
                        serde_json::json!(["environment.ignite", ctx.tick, c])
                    ) < t.ignite[&ch]
                {
                    self.ignite_inner(w, c, ctx, t)?;
                }
                if w.game.sections.get(&(c as u32)).is_some_and(|s| !s.failed) {
                    let meta = &w.sections[&c];
                    let entity = w
                        .structures
                        .iter()
                        .find(|s| s.id == meta.structure_id)
                        .unwrap()
                        .entity_id;
                    if entity.is_some_and(|id| hit.skip_buildings.contains(&id)) {
                        continue;
                    }
                    if let Some(id) = entity {
                        if let Some((_, cells)) = buildings.iter_mut().find(|(key, _)| *key == id) {
                            cells.push((c, amount));
                        } else {
                            buildings.push((id, vec![(c, amount)]));
                        }
                    } else {
                        self.damage_section_inner(w, c, amount, ctx, hit, out)?;
                    }
                    continue;
                }
                if w.game.cell_hp[c] > 0. {
                    let next_hp = f64::from(w.game.cell_hp[c]) - amount;
                    w.game.cell_hp[c] = next_hp as f32;
                    if next_hp <= 0. {
                        self.wreck_inner(w, c, None, ctx, hit.actor_order, out)?;
                    } else {
                        w.touch(c)?;
                    }
                }
            }
        }
        for (id, mut cells) in buildings {
            if let Some(memory) = hit.memory.as_mut()
                && !memory.buildings.insert(id)
            {
                continue;
            }
            crate::sort::sort_by(&mut cells, |a, b| b.1 - a.1);
            let mut left = cells[0].1;
            for (c, _) in cells {
                if left <= 0. {
                    break;
                }
                left -= self.damage_section_inner(w, c, left, ctx, hit, out)?;
            }
        }
        Ok(())
    }
    fn dent(&self, w: &mut World, c: usize, t: &Tuning) -> Result<(), EnvironmentError> {
        w.game
            .height
            .get_or_insert_with(|| vec![0; w.game.w * w.game.h]);
        // Source subtraction uses Numbers, not the Int8 terrain storage width.
        let target = i16::from(level(w, c)) - 1;
        if target < i16::from(t.min_level)
            || near(w, c)
                .into_iter()
                .flatten()
                .any(|n| i16::from(level(w, n)) - target > 1)
        {
            return Ok(());
        }
        w.set_level(c, target as i8)?;
        Ok(())
    }
    /// Ordinary burning uses the complete current actor order.
    pub fn burn(
        &mut self,
        w: &mut World,
        dt: f64,
        ctx: &mut Context<'_>,
    ) -> Result<Vec<Effect>, EnvironmentError> {
        let mut out = Vec::new();
        self.burn_emitting(w, dt, ctx, &mut |effect| out.push(effect))?;
        Ok(out)
    }
    /// Emit each committed effect in source order, including the prefix before an error.
    pub fn burn_emitting(
        &mut self,
        w: &mut World,
        dt: f64,
        ctx: &mut Context<'_>,
        out: &mut dyn FnMut(Effect),
    ) -> Result<(), EnvironmentError> {
        self.burn_inner(w, dt, ctx, None, out)
    }
    /// Burning after late births uses the source's earlier captured actor list.
    /// Context keeps all current units for validation, garrison eviction and live
    /// grid queries. Only direct casualties and list-based rubble effects are captured.
    pub fn burn_captured(
        &mut self,
        w: &mut World,
        dt: f64,
        ctx: &mut Context<'_>,
        actor_order: &[u32],
    ) -> Result<Vec<Effect>, EnvironmentError> {
        let mut out = Vec::new();
        self.burn_captured_emitting(w, dt, ctx, actor_order, &mut |effect| out.push(effect))?;
        Ok(out)
    }
    /// Emit captured burning effects in source order, retaining the prefix on error.
    pub fn burn_captured_emitting(
        &mut self,
        w: &mut World,
        dt: f64,
        ctx: &mut Context<'_>,
        actor_order: &[u32],
        out: &mut dyn FnMut(Effect),
    ) -> Result<(), EnvironmentError> {
        self.burn_inner(w, dt, ctx, Some(actor_order), out)
    }
    fn burn_inner(
        &mut self,
        w: &mut World,
        dt: f64,
        ctx: &mut Context<'_>,
        actor_order: Option<&[u32]>,
        out: &mut dyn FnMut(Effect),
    ) -> Result<(), EnvironmentError> {
        let t = check(w, ctx)?;
        let skip = BTreeSet::new();
        let mut hit = DamageContext::plain(&skip);
        hit.actor_order = actor_order;
        check_damage_actors(w, &hit)?;
        if !dt.is_finite() || !(0.0..=7200.).contains(&dt) {
            return Err(EnvironmentError::InvalidInput("burn dt"));
        }
        let wx = math::cos(ctx.wind_angle);
        let wz = math::sin(ctx.wind_angle);
        for &id in actor_order.unwrap_or(ctx.unit_order) {
            let u = &w.game.units[&id];
            let c = if u.hp > 0. && u.air.is_none() && !u.riding && t.kinds.kinds[u.kind].infantry {
                u.garrison.or_else(|| w.game.cell_of(u.x, u.z))
            } else {
                None
            };
            let Some(c) = c.filter(|c| w.fires.contains_key(c)) else {
                continue;
            };
            let u = w.game.units.get_mut(&id).unwrap();
            u.hp -= t.inf * dt;
            u.supp = math::min(100., u.supp + t.supp * dt);
            out(hurt(w, id, None, false, None));
            let u = &w.game.units[&id];
            if u.garrison.is_none() && u.path.is_empty() {
                let at = u.at();
                let exits = [
                    (1, 0),
                    (-1, 0),
                    (0, 1),
                    (0, -1),
                    (1, 1),
                    (-1, 1),
                    (1, -1),
                    (-1, -1),
                ]
                .iter()
                .filter_map(|(dx, dz)| {
                    w.game
                        .cell_of(at.x + f64::from(*dx) * 2., at.z + f64::from(*dz) * 2.)
                })
                .filter(|n| w.game.flags[*n] & MOVE == 0 && !w.fires.contains_key(n))
                .collect::<Vec<_>>();
                if let Some(&to) = exits
                    .iter()
                    .find(|n| w.game.chars[**n] != b'H')
                    .or(exits.first())
                {
                    let p = center(w, to);
                    let u = w.game.units.get_mut(&id).unwrap();
                    u.path = vec![p];
                    u.path_gen += 1;
                }
            }
            let _ = c;
        }
        let fires: Vec<_> = w.fires.iter().map(|(c, left)| (*c, *left)).collect();
        for (c, left) in fires {
            let ch = w.game.physical(c);
            let x = c % w.game.w;
            let y = c / w.game.w;
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let nx = x as i64 + dx;
                    let ny = y as i64 + dy;
                    if dx == 0 && dy == 0
                        || nx < 0
                        || ny < 0
                        || nx >= w.game.w as i64
                        || ny >= w.game.h as i64
                    {
                        continue;
                    }
                    let n = ny as usize * w.game.w + nx as usize;
                    if !flammable(w, n, ctx.wet) {
                        continue;
                    }
                    let along = (dx as f64 * wx + dy as f64 * wz)
                        / math::hypot(dx as f64, dy as f64)
                        * ctx.wind_strength;
                    // Even zero dt/rain consumes the draw for every flammable candidate.
                    let random = aligned_environment_draw!(
                        ctx.seed,
                        self.replay.as_ref(),
                        serde_json::json!(["environment.spread", ctx.tick, c, n])
                    );
                    if random
                        < t.spread
                            .get(&w.game.physical(n))
                            .copied()
                            .unwrap_or(f64::NAN)
                            * dt
                            * math::max(0.1, 1. + 1.5 * along)
                            * (1. - ctx.rain)
                    {
                        self.ignite_inner(w, n, ctx, t)?;
                    }
                }
            }
            if ch == b'B' {
                let amount = w.max_hp(c) / t.burn[&b'B']
                    * dt
                    * (0.2 + 0.8 * material(w.material_name(c, false).unwrap()).1)
                    * (1. - ctx.rain * 0.7);
                hit.direction = Some(Point { x: wx, z: wz });
                self.damage_section_inner(w, c, amount, ctx, &hit, out)?;
                if w.game.sections.get(&(c as u32)).is_some_and(|s| s.failed) {
                    w.burnt[c] = 1;
                    continue;
                }
            }
            let remaining = left - dt * (1. + 3. * ctx.rain);
            if remaining > 0. {
                w.fires.insert(c, remaining);
                continue;
            }
            w.fires.remove(&c);
            w.burnt[c] = 1;
            if let Some(s) = w.game.sections.get(&(c as u32)).filter(|s| !s.failed) {
                w.sections.get_mut(&c).unwrap().appearance = if s.hp < s.max_hp.unwrap() {
                    Appearance::Damaged
                } else {
                    Appearance::Intact
                };
                w.append_journal(c);
            }
            if ch == b'H' || ch == b'O' {
                self.wreck_inner(w, c, Some(b'.'), ctx, actor_order, out)?;
            } else {
                w.touch(c)?;
            }
        }
        Ok(())
    }
    pub fn flood(&mut self, w: &mut World) -> Result<(), EnvironmentError> {
        w.mutation_capacity(w.game.chars.len() as u64 * 4)?;
        let fill: Vec<_> =
            w.soak()
                .iter()
                .filter_map(|c| usize::try_from(*c).ok())
                .filter(|c| {
                    *c < w.game.chars.len()
                        && w.game.chars[*c] == b'+'
                        && near(w, *c).into_iter().flatten().any(|n| {
                            b"WF".contains(&w.game.chars[n]) && level(w, n) >= level(w, *c)
                        })
                })
                .collect();
        w.take_soak();
        for c in fill {
            let deep = w.game.wear.as_ref().unwrap()[c];
            w.set_cell(c, b'F')?;
            w.game.wear.as_mut().unwrap()[c] = deep;
            w.touch(c)?;
        }
        Ok(())
    }
}

impl EnvironmentRuntime {
    fn finish_overflow(
        &mut self,
        w: &mut World,
        mut falling: FallingSection,
        ctx: &Context<'_>,
    ) -> Result<(), EnvironmentError> {
        let mut env = WorldDebris {
            world: w,
            grid: ctx.grid,
            tuning: tuning()?,
        };
        for _ in 0..=50 {
            if falling.body.settled {
                break;
            }
            falling
                .body
                .step(0.05, &mut env)
                .map_err(|_| EnvironmentError::InvalidInput("debris environment"))?;
        }
        falling
            .body
            .settle(&mut env)
            .map_err(|_| EnvironmentError::InvalidInput("debris environment"))?;
        self.settle_section(w, &falling, ctx)
    }
    fn settle_section(
        &self,
        w: &mut World,
        falling: &FallingSection,
        ctx: &Context<'_>,
    ) -> Result<(), EnvironmentError> {
        // The reference resolves the current cell section when a queued body settles.
        let Some(meta) = w.sections.get(&falling.cell) else {
            return Ok(());
        };
        let id = meta.id.clone();
        if !w.game.sections[&(falling.cell as u32)].failed || meta.failure.settled_at.is_some() {
            return Ok(());
        }
        let (meta, _) = w.section_by_id_mut(&id).unwrap();
        meta.failure.settled_at = Some(ctx.tick as f64 * 0.05);
        meta.rubble.clear();
        if falling.bridge {
            return Ok(());
        }
        let body = &falling.body;
        let t = tuning()?;
        let points = [
            Point {
                x: body.x,
                z: body.z,
            },
            Point {
                x: body.x + math::cos(body.dir) * 2. * 0.7,
                z: body.z + math::sin(body.dir) * 2. * 0.7,
            },
        ];
        let mut cells = Vec::new();
        for p in points {
            if let Some(c) = w.game.cell_of(p.x, p.z)
                && !cells.contains(&c)
            {
                cells.push(c);
            }
        }
        let name = w.sections[&falling.cell].material.clone();
        for c in cells {
            if !b".D+MR".contains(&w.game.physical(c)) || occupied(w, c, ctx.grid, t) {
                continue;
            }
            w.mutate_cell(
                c,
                CellChange {
                    object: Some(b'R'),
                    material: Some(name.clone()),
                    ..Default::default()
                },
            )?;
            w.sections.get_mut(&falling.cell).unwrap().rubble.push(c);
        }
        w.log_cell(falling.cell, false)?;
        Ok(())
    }
    /// Advance queued section bodies with authoritative contacts, then place settled rubble.
    /// Vehicle wreck bodies are owned by their producer and are not advanced here.
    pub fn step_sections(
        &mut self,
        w: &mut World,
        dt: f64,
        ctx: &mut Context<'_>,
    ) -> Result<(), EnvironmentError> {
        check(w, ctx)?;
        if !dt.is_finite() || !(0.0..=2.5).contains(&dt) {
            return Err(EnvironmentError::InvalidInput("debris dt"));
        }
        let mut bodies = std::mem::take(&mut self.falling_sections);
        for falling in &mut bodies {
            if w.game
                .sections
                .get(&(falling.cell as u32))
                .is_none_or(|s| !s.failed)
            {
                falling.body.settled = true;
                continue;
            }
            falling
                .body
                .step(
                    dt,
                    &mut WorldDebris {
                        world: w,
                        grid: ctx.grid,
                        tuning: tuning()?,
                    },
                )
                .map_err(|_| EnvironmentError::InvalidInput("debris environment"))?;
            if falling.body.settled {
                self.settle_section(w, falling, ctx)?;
            }
        }
        self.falling_sections = bodies.into_iter().filter(|s| !s.body.settled).collect();
        Ok(())
    }
}
fn occupied(w: &World, c: usize, grid: &crate::grid::SpatialGrid, t: &Tuning) -> bool {
    let at = center(w, c);
    let radius = t.kinds.kinds.iter().fold(0., |r, k| math::max(r, k.radius));
    grid.has_heavy(&w.game.units, (at.x, at.z), radius + 2., |_, u| {
        let kind = &t.kinds.kinds[u.kind];
        u.hp > 0.
            && u.air.is_none()
            && !u.riding
            && !kind.infantry
            && !kind.structure
            && (u.x - at.x).abs() < kind.radius + 1.
            && (u.z - at.z).abs() < kind.radius + 1.
    })
}
struct WorldDebris<'a> {
    world: &'a World,
    grid: &'a crate::grid::SpatialGrid,
    tuning: &'static Tuning,
}
impl crate::environment_debris::Environment for WorldDebris<'_> {
    fn ground(&mut self, x: f64, z: f64) -> f64 {
        f64::from(self.world.game.level_at(x, z)) * self.tuning.level_height
    }
    fn material(&mut self, x: f64, z: f64) -> Option<crate::environment_debris::ContactMaterial> {
        let name = self
            .world
            .game
            .cell_of(x, z)
            .and_then(|c| self.world.material_name(c, true))
            .unwrap_or("soil");
        let m = material(name);
        Some(crate::environment_debris::ContactMaterial {
            absorption: Some(m.2),
            rebound: Some(m.3),
        })
    }
    fn contact(
        &mut self,
        a: crate::environment_debris::Vec3,
        b: crate::environment_debris::Vec3,
        body: &DebrisBody,
    ) -> Option<crate::environment_debris::Contact> {
        use crate::projectiles::{self, BodyPosition, Box3, Point3};
        let w = self.world;
        let pa = Point3::new(a.x, a.y, a.z);
        let pb = Point3::new(b.x, b.y, b.z);
        let radius = body.radius;
        let reach = (radius / 2.).ceil() as i64;
        let crossed = projectiles::crossed_cells(
            BodyPosition { x: a.x, z: a.z },
            BodyPosition { x: b.x, z: b.z },
            2.,
            w.game.w,
            w.game.h,
        )
        .expect("validated world geometry");
        let mut cells = Vec::new();
        let mut seen = BTreeSet::new();
        for c in crossed {
            for dz in -reach..=reach {
                for dx in -reach..=reach {
                    let x = (c % w.game.w) as i64 + dx;
                    let z = (c / w.game.w) as i64 + dz;
                    if x >= 0 && z >= 0 && x < w.game.w as i64 && z < w.game.h as i64 {
                        let next = z as usize * w.game.w + x as usize;
                        if seen.insert(next) {
                            cells.push(next);
                        }
                    }
                }
            }
        }
        let mut best: Option<(crate::projectiles::ContactHit, &str)> = None;
        for c in cells {
            let height = self
                .tuning
                .flight
                .get(&w.game.physical(c))
                .copied()
                .unwrap_or(0.);
            let top = f64::from(level(w, c)) * self.tuning.level_height + height;
            if height <= 0. && top <= a.y + 0.01 {
                continue;
            }
            let x = (c % w.game.w) as f64 * 2.;
            let z = (c / w.game.w) as f64 * 2.;
            let hit = projectiles::sweep_box(
                pa,
                pb,
                Box3 {
                    x: [x - radius, x + 2. + radius],
                    y: [-100., top],
                    z: [z - radius, z + 2. + radius],
                },
            )
            .expect("validated world geometry");
            if let Some(hit) = hit
                && best
                    .as_ref()
                    .is_none_or(|(old, _)| hit.fraction < old.fraction)
            {
                best = Some((hit, w.material_name(c, false).unwrap()));
            }
        }
        let middle = Point {
            x: (a.x + b.x) / 2.,
            z: (a.z + b.z) / 2.,
        };
        let candidates =
            self.grid
                .unordered(&w.game.units, (middle.x, middle.z), radius + 12., |_, u| {
                    let k = &self.tuning.kinds.kinds[u.kind];
                    u.hp > 0. && u.air.is_none() && !u.riding && !k.infantry && !k.structure
                });
        for id in candidates {
            let u = &w.game.units[&id];
            let pos = BodyPosition { x: u.x, z: u.z };
            let ground = f64::from(w.game.level_at(u.x, u.z)) * self.tuning.level_height;
            if let Some(hit) = projectiles::sweep_body(
                pa,
                pb,
                pos,
                pos,
                radius + self.tuning.kinds.kinds[u.kind].body_radius(),
                ground,
                ground + 2.5,
            )
            .expect("validated body geometry")
                && best
                    .as_ref()
                    .is_none_or(|(old, _)| hit.fraction < old.fraction)
            {
                best = Some((hit, "steel"));
            }
        }
        if b.x < 0. || b.z < 0. || b.x >= w.game.w as f64 * 2. || b.z >= w.game.h as f64 * 2. {
            return Some(crate::environment_debris::Contact {
                fraction: 0.,
                normal: crate::environment_debris::Vec3::new(
                    if b.x < 0. {
                        1.
                    } else if b.x >= w.game.w as f64 * 2. {
                        -1.
                    } else {
                        0.
                    },
                    0.,
                    if b.z < 0. {
                        1.
                    } else if b.z >= w.game.h as f64 * 2. {
                        -1.
                    } else {
                        0.
                    },
                ),
                material: Some(crate::environment_debris::ContactMaterial {
                    absorption: Some(0.25),
                    rebound: Some(0.65),
                }),
            });
        }
        best.map(|(hit, name)| {
            let m = material(name);
            crate::environment_debris::Contact {
                fraction: hit.fraction,
                normal: crate::environment_debris::Vec3::new(
                    hit.normal.x,
                    hit.normal.y,
                    hit.normal.z,
                ),
                material: Some(crate::environment_debris::ContactMaterial {
                    absorption: Some(m.2),
                    rebound: Some(m.3),
                }),
            }
        })
    }
}
