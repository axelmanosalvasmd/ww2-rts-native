//! Prepare complete spawn batches, then publish through disjoint canonical fields.
//! Wallets, story and other unit systems remain with their single owners.
use crate::{
    catalog::Catalog,
    game::{Game, LOCAL_DEFENDER_OWNER, ModeKind, Point, Unit},
    grid::{SpatialGrid, UnitId},
    match_commands::{Command, UnitControl},
    match_runtime::RecruitMetadata,
    navigation::{TerrainView, Versions},
    rules::Rules,
    unit_spawn::{self, SpawnError, SpawnPlacer},
};
use std::collections::BTreeMap;
#[derive(Debug, PartialEq, Eq)]
pub enum PublicationError {
    Invalid(&'static str),
    Spawn(SpawnError),
    Control(String),
}
/// Mutable preparation only. ID, owner, kind, geometry and control are checked
/// again when converting the entire batch into a publication guard.
pub struct StagedUnit {
    pub unit: Unit,
    pub control: UnitControl,
    pub recruitment: Option<RecruitMetadata>,
}
/// A spawn source is resolved by the authoritative commander or World actor owner.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SpawnActor {
    Commander { owner: usize, spawn: Point },
    WorldLocalDefender { spawn: Point },
}
impl StagedUnit {
    /// Run ordinary source scatter before a final pose override. Explicit IDs and
    /// indices let a caller stage a source-ordered respawn batch before any payment.
    #[allow(clippy::too_many_arguments)]
    pub fn ordinary(
        game: &Game,
        rules: &Rules,
        owner: usize,
        kind: usize,
        id: UnitId,
        scatter_index: u32,
        player_spawn: Point,
        pose_override: Option<Point>,
    ) -> Result<Self, PublicationError> {
        Self::ordinary_with_actor(
            game,
            rules,
            SpawnActor::Commander {
                owner,
                spawn: player_spawn,
            },
            kind,
            id,
            scatter_index,
            pose_override,
        )
    }
    /// World actors use the same scatter and initializer without a commander seat.
    /// World ability/guard overrides are staged by their authoritative owner.
    #[allow(clippy::too_many_arguments)]
    pub fn ordinary_with_actor(
        game: &Game,
        rules: &Rules,
        actor: SpawnActor,
        kind: usize,
        id: UnitId,
        scatter_index: u32,
        pose_override: Option<Point>,
    ) -> Result<Self, PublicationError> {
        let (owner, player_spawn) = match actor {
            SpawnActor::Commander { owner, spawn } if owner < game.players.len() => (owner, spawn),
            SpawnActor::WorldLocalDefender { spawn } if game.mode == Some(ModeKind::World) => {
                (LOCAL_DEFENDER_OWNER, spawn)
            }
            _ => return Err(PublicationError::Invalid("spawn actor")),
        };
        if let Some(at) = pose_override {
            if !finite(at) {
                return Err(PublicationError::Invalid("spawn override"));
            }
        }
        let profile = rules
            .kinds
            .get(kind)
            .ok_or(PublicationError::Invalid("spawn kind"))?;
        let mut placer = SpawnPlacer::new();
        let at = placer
            .scatter_position(
                TerrainView {
                    width: game.w,
                    height: game.h,
                    flags: &game.flags,
                    heights: game.height.as_deref(),
                    world_known: false,
                    knowledge_id: 0,
                    versions: Versions::default(),
                    changed: None,
                    changes: &[],
                },
                profile,
                player_spawn,
                scatter_index,
            )
            .map_err(PublicationError::Spawn)?
            .point;
        let catalog = Catalog::compiled().map_err(|e| PublicationError::Spawn(e.into()))?;
        let mut unit = unit_spawn::initial_unit(rules, catalog, kind, id, owner, at)
            .map_err(PublicationError::Spawn)?;
        if let Some(at) = pose_override {
            unit.x = at.x;
            unit.z = at.z;
        }
        let control = UnitControl::new(&unit, rules).map_err(PublicationError::Control)?;
        Ok(Self {
            unit,
            control,
            recruitment: None,
        })
    }
}
/// Only the fields publication writes are held exclusively. Economy, companion,
/// story, firing, combat and reserve state can be prepared independently.
pub struct PublicationParts<'a> {
    pub units: &'a mut BTreeMap<UnitId, Unit>,
    pub grid: &'a mut SpatialGrid,
    pub controls: &'a mut BTreeMap<UnitId, UnitControl>,
    pub recruitment: &'a mut BTreeMap<UnitId, RecruitMetadata>,
    pub next_id: &'a mut UnitId,
}
#[must_use = "publish the validated batch or drop without insertion"]
pub struct PreparedPublication<'a> {
    parts: PublicationParts<'a>,
    staged: Vec<StagedUnit>,
    next_id: UnitId,
    hp: BTreeMap<UnitId, f64>,
}
fn finite(p: Point) -> bool {
    p.x.is_finite() && p.z.is_finite()
}
fn valid_unit(u: &Unit, rules: &Rules, players: usize, world: bool) -> bool {
    u.id != 0
        && (u.owner < players || (world && u.owner == LOCAL_DEFENDER_OWNER))
        && u.kind < rules.kinds.len()
        && finite(u.at())
        && u.rot.is_finite()
        && u.hp.is_finite()
        && u.hp >= 0.0
        && u.supp.is_finite()
        && u.path.iter().all(|p| finite(*p))
        && u.world_goal.is_none_or(finite)
}
fn valid_control(c: &UnitControl) -> bool {
    c.aim.is_finite()
        && c.route_end.is_none_or(finite)
        && c.face.is_none_or(|f| f.angle.is_finite() && finite(f.at))
        && c.nade.is_none_or(finite)
        && c.orders.iter().all(|command| match command {
            Command::Move { orders, face, .. } => {
                face.is_none_or(f64::is_finite)
                    && orders
                        .iter()
                        .all(|order| order.unit != 0 && finite(order.at))
            }
            _ => true,
        })
}
pub fn prepare_publication<'a>(
    parts: PublicationParts<'a>,
    rules: &Rules,
    players: usize,
    staged: Vec<StagedUnit>,
) -> Result<PreparedPublication<'a>, PublicationError> {
    prepare(parts, rules, players, staged, false)
}
/// Only the actual World mode admits the named local defender actor. All other
/// invalid owners remain errors, including arbitrary integers outside player slots.
pub fn prepare_world_publication<'a>(
    parts: PublicationParts<'a>,
    rules: &Rules,
    players: usize,
    mode: Option<ModeKind>,
    staged: Vec<StagedUnit>,
) -> Result<PreparedPublication<'a>, PublicationError> {
    if mode != Some(ModeKind::World) {
        return Err(PublicationError::Invalid("World publication mode"));
    }
    prepare(parts, rules, players, staged, true)
}
fn prepare<'a>(
    parts: PublicationParts<'a>,
    rules: &Rules,
    players: usize,
    staged: Vec<StagedUnit>,
    world: bool,
) -> Result<PreparedPublication<'a>, PublicationError> {
    if players == 0 || players > 6 || *parts.next_id == 0 {
        return Err(PublicationError::Invalid("publication players/allocator"));
    }
    for (id, u) in parts.units.iter() {
        if *id != u.id || !valid_unit(u, rules, players, world) || *id >= *parts.next_id {
            return Err(PublicationError::Invalid("existing unit/allocator"));
        }
    }
    if parts
        .controls
        .keys()
        .chain(parts.recruitment.keys())
        .any(|id| *id == 0 || *id >= *parts.next_id)
    {
        return Err(PublicationError::Invalid("existing metadata/allocator"));
    }
    let mut next = *parts.next_id;
    for draft in &staged {
        if draft.unit.id != next
            || !valid_unit(&draft.unit, rules, players, world)
            || !valid_control(&draft.control)
            || parts.units.contains_key(&next)
            || parts.controls.contains_key(&next)
            || parts.recruitment.contains_key(&next)
        {
            return Err(PublicationError::Invalid("staged unit/control/identity"));
        }
        next = next
            .checked_add(1)
            .ok_or(PublicationError::Invalid("unit ID capacity exhausted"))?;
    }
    Ok(PreparedPublication {
        parts,
        staged,
        next_id: next,
        hp: BTreeMap::new(),
    })
}
impl PreparedPublication<'_> {
    /// Validate and stage an existing squad's post-payment health. Repeated updates
    /// replace that squad's staged value. No live health changes before publication.
    pub fn stage_existing_hp(&mut self, id: UnitId, hp: f64) -> Result<(), PublicationError> {
        if !self.parts.units.contains_key(&id) || !hp.is_finite() || hp < 0.0 {
            return Err(PublicationError::Invalid("existing HP update"));
        }
        self.hp.insert(id, hp);
        Ok(())
    }
    pub fn existing_hp(&self, id: UnitId) -> Option<f64> {
        self.hp
            .get(&id)
            .copied()
            .or_else(|| self.parts.units.get(&id).map(|u| u.hp))
    }
    pub fn ids(&self) -> impl Iterator<Item = UnitId> + '_ {
        self.staged.iter().map(|draft| draft.unit.id)
    }
    pub fn publish(self) -> Vec<UnitId> {
        self.publish_with(|_, _| {})
    }
    /// The callback may only publish already prepared, infallible metadata owned
    /// by other systems. All their fallible validation belongs before this call.
    pub fn publish_with(self, mut metadata: impl FnMut(&Unit, &UnitControl)) -> Vec<UnitId> {
        for (id, hp) in self.hp {
            self.parts
                .units
                .get_mut(&id)
                .expect("guard keeps existing units")
                .hp = hp;
        }
        let mut ids = Vec::with_capacity(self.staged.len());
        for draft in self.staged {
            let id = draft.unit.id;
            self.parts.units.insert(id, draft.unit);
            self.parts.grid.update(id, &self.parts.units[&id]);
            self.parts.controls.insert(id, draft.control);
            if let Some(row) = draft.recruitment {
                self.parts.recruitment.insert(id, row);
            }
            metadata(&self.parts.units[&id], &self.parts.controls[&id]);
            ids.push(id);
        }
        *self.parts.next_id = self.next_id;
        ids
    }
}
