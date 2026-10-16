//! Per-player terrain memory, filtered before storage, with a separate outbox.
//!
//! Ordinary observed terrain only. Visibility and footprint facts come from the
//! caller; this module does not calculate fog or adopt a wire/privacy contract.

use crate::{
    game::ModeKind,
    route_knowledge::{
        CellMetadata, KnowledgeError, Material, RememberedCell, RememberedSection, RouteKnowledge,
    },
    world::{CellRow, Recipient, World, WorldError},
};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObservationFrame {
    pub team: i32,
    pub skip_fog: bool,
}

/// Facts for the same footprint object. Ruins absent from the unit table use
/// `ruin_visible`; live objects use `visible`. Allied footprints are always seen.
#[derive(Clone, Copy, Debug)]
pub struct FootprintFact {
    pub identity: u64,
    pub allied: bool,
    pub live: bool,
    pub visible: bool,
    pub ruin_visible: bool,
}
impl FootprintFact {
    fn seen(self) -> bool {
        self.allied
            || if self.live {
                self.visible
            } else {
                self.ruin_visible
            }
    }
}

/// The root adapter must provide the original teamSees/mineKnown/allied and
/// footprint-table semantics. No authoritative fallback is applied if false.
pub trait ObservationFacts {
    fn cell_visible(&mut self, cell: usize, team: i32) -> bool;
    fn mine_known(&mut self, cell: usize, team: i32) -> bool;
    fn mine_owned(&mut self, cell: usize, team: i32) -> bool;
    fn footprint(&mut self, cell: usize, slot: usize, team: i32) -> Option<FootprintFact>;
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ObservedRow {
    pub row: CellRow,
    /// Present only for a mine known to this recipient, including known enemy mines.
    pub mine_owned: Option<bool>,
    /// Stable while this remembered row is unchanged; scoped to this producer.
    pub revision: u64,
}
impl ObservedRow {
    fn same_content(&self, row: &CellRow, owned: Option<bool>) -> bool {
        self.row == *row && self.mine_owned == owned
    }
    pub fn route_row(&self) -> Result<RememberedCell, ObservedError> {
        let d = &self.row.data;
        Ok(RememberedCell {
            cell: self.row.cell,
            revision: self.revision,
            character: self.row.ch,
            height: self.row.height,
            state: self.row.state,
            metadata: CellMetadata {
                ground: Some(d.ground),
                object: Some(d.object),
                material: Some(material(&d.material)?),
                section: d
                    .section
                    .as_ref()
                    .map(|s| {
                        Ok::<RememberedSection, ObservedError>(RememberedSection {
                            hp: s.hp,
                            max_hp: Some(s.max_hp),
                            failed: s.state == "failed",
                            material: Some(material(&s.material)?),
                        })
                    })
                    .transpose()?,
                trench_front: d.trench_front.map(f64::from),
                mine_owned: self.mine_owned.unwrap_or(false),
            },
            cover_basis: None,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ObservedError {
    InvalidShape,
    InvalidCell,
    InvalidValue,
    InitialChanged,
    RecipientMismatch,
    UnsupportedWorld,
    RevisionExhausted,
    World(WorldError),
    Knowledge(KnowledgeError),
}
impl From<WorldError> for ObservedError {
    fn from(value: WorldError) -> Self {
        Self::World(value)
    }
}
impl From<KnowledgeError> for ObservedError {
    fn from(value: KnowledgeError) -> Self {
        Self::Knowledge(value)
    }
}

/// Keep this producer with its player across snapshots and reconnects. Recreate
/// it and the corresponding RouteKnowledge when a new initial map is installed.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ObservedMemory {
    slot: usize,
    identity: u64,
    width: usize,
    height: usize,
    initial_ptr: usize,
    rows: Vec<ObservedRow>,
    positions: BTreeMap<usize, usize>,
    outbox: Vec<ObservedRow>,
    key: Option<(u32, i64)>,
    revision: u64,
}
// Transient admission, not a second recipient memory or delivery ledger.
struct PreparedRefresh {
    key: Option<(u32, i64)>,
    ack: Option<Vec<usize>>,
    accepted: Vec<(CellRow, Option<bool>)>,
}
impl ObservedMemory {
    pub(crate) fn relocate_save_storage(&mut self, bindings: &BTreeMap<usize, usize>) {
        self.initial_ptr = bindings.get(&self.initial_ptr).copied().unwrap_or(0);
    }

    pub fn new(world: &World, slot: usize, identity: u64) -> Result<Self, ObservedError> {
        validate_world(world)?;
        Ok(Self {
            slot,
            identity,
            width: world.game.w,
            height: world.game.h,
            initial_ptr: world.initial().chars.as_ptr() as usize,
            rows: Vec::new(),
            positions: BTreeMap::new(),
            outbox: Vec::new(),
            key: None,
            revision: 0,
        })
    }
    pub fn identity(&self) -> u64 {
        self.identity
    }
    pub fn slot(&self) -> usize {
        self.slot
    }
    /// Full route/reconnect reads preserve the first insertion order and do not drain snapshots.
    pub fn full_rows(&self) -> &[ObservedRow] {
        &self.rows
    }
    pub fn outbox(&self) -> &[ObservedRow] {
        &self.outbox
    }
    pub fn take_changes(&mut self) -> Vec<ObservedRow> {
        std::mem::take(&mut self.outbox)
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub(crate) fn validate_binding(&self, world: &World) -> Result<(), ObservedError> {
        validate_world(world)?;
        if world.game.w != self.width
            || world.game.h != self.height
            || world.initial().chars.as_ptr() as usize != self.initial_ptr
        {
            return Err(ObservedError::InitialChanged);
        }
        Ok(())
    }

    /// Whether pending rows can be replayed at this collector key. The caller
    /// still invokes refresh on an empty first read to register future updates.
    pub(crate) fn needs_replay(&self, world: &World) -> bool {
        let pending = world
            .pending(Recipient::Player(self.slot))
            .map_or(!world.cell_log().is_empty(), |v| !v.is_empty());
        pending
            && self.key
                != Some((
                    world.game.terrain_version,
                    world.game.vision_tick.unwrap_or(-1),
                ))
    }

    /// Replay sorted pending log indices at most once per terrain/vision key.
    /// Accepted updates are staged, including revision capacity, before memory
    /// or the World's pending set changes. Hidden cells remain pending.
    pub fn refresh<F: ObservationFacts>(
        &mut self,
        world: &mut World,
        frame: ObservationFrame,
        facts: &mut F,
    ) -> Result<usize, ObservedError> {
        let (count, receipt) = self.refresh_impl(world, frame, facts, false)?;
        receipt
            .commit(world)
            .map_err(|_| ObservedError::RecipientMismatch)?;
        Ok(count)
    }
    pub(crate) fn refresh_impl<F: ObservationFacts>(
        &mut self,
        world: &World,
        frame: ObservationFrame,
        facts: &mut F,
        capture_frame: bool,
    ) -> Result<(usize, crate::world::ObservationReceipt), ObservedError> {
        let PreparedRefresh { key, ack, accepted } = self.prepare_refresh(world, frame, facts)?;
        let count = accepted.len();
        for (row, mine_owned) in accepted {
            self.revision += 1;
            let c = row.cell;
            let entry = ObservedRow {
                row,
                mine_owned,
                revision: self.revision,
            };
            if let Some(&at) = self.positions.get(&c) {
                self.rows[at] = entry.clone();
            } else {
                self.positions.insert(c, self.rows.len());
                self.rows.push(entry.clone());
            }
            self.outbox.retain(|old| old.row.cell != c);
            self.outbox.push(entry);
        }
        if let Some(key) = key {
            self.key = Some(key);
        }
        let recipient = Recipient::Player(self.slot);
        Ok((
            count,
            world.defer_observation(ack.map(|indices| (recipient, indices)), capture_frame),
        ))
    }

    /// Admit the same collector read without publishing rows or creating a receipt.
    pub(crate) fn preflight_refresh<F: ObservationFacts>(
        &self,
        world: &World,
        frame: ObservationFrame,
        facts: &mut F,
    ) -> Result<(), ObservedError> {
        self.prepare_refresh(world, frame, facts).map(|_| ())
    }

    fn prepare_refresh<F: ObservationFacts>(
        &self,
        world: &World,
        frame: ObservationFrame,
        facts: &mut F,
    ) -> Result<PreparedRefresh, ObservedError> {
        self.validate_binding(world)?;
        let key = (
            world.game.terrain_version,
            world.game.vision_tick.unwrap_or(-1),
        );
        let recipient = Recipient::Player(self.slot);
        let pending: Vec<_> = world.pending(recipient).map_or_else(
            || (0..world.cell_log().len()).collect(),
            |p| p.iter().copied().collect(),
        );
        if pending.is_empty() {
            return Ok(PreparedRefresh {
                key: None,
                ack: Some(Vec::new()),
                accepted: Vec::new(),
            });
        }
        if self.key == Some(key) {
            return Ok(PreparedRefresh {
                key: None,
                ack: None,
                accepted: Vec::new(),
            });
        }
        let mut ack = Vec::new();
        let mut accepted = Vec::new();
        let mut footprints = BTreeMap::new();
        for index in pending {
            let stored = world
                .cell_log()
                .get(index)
                .ok_or(ObservedError::InvalidCell)?;
            let c = stored.cell;
            if c >= self.width * self.height {
                return Err(ObservedError::InvalidCell);
            }
            let old = self.positions.get(&c).map(|&at| &self.rows[at]);
            let known = if world.game.chars[c] == b'N' || stored.ch == b'N' {
                facts.mine_known(c, frame.team)
            } else {
                false
            };
            let mut row = world.cell_row(c)?;
            let owned = if row.ch == b'N' && known {
                Some(facts.mine_owned(c, frame.team))
            } else {
                None
            };
            if row.ch == b'N' && !known {
                row.ch = world.game.physical(c);
                row.data.mine = false;
                row.data.mine_material = None;
            }
            validate_row(&row)?;
            if old.is_some_and(|old| old.same_content(&row, owned)) {
                ack.push(index);
                continue;
            }
            let hidden_mine = stored.ch == b'N' && !known;
            if hidden_mine
                && old.map_or_else(
                    || {
                        row.ch == world.initial().chars[c]
                            && row.height == world.initial().height.as_ref().map_or(0, |h| h[c])
                    },
                    |old| {
                        old.row.ch == row.ch
                            && old.row.height == row.height
                            && old.row.state == row.state
                    },
                )
            {
                continue;
            }
            if let Some(footprint) = facts.footprint(c, self.slot, frame.team) {
                if !*footprints
                    .entry(footprint.identity)
                    .or_insert_with(|| footprint.seen())
                {
                    continue;
                }
            } else if !world.game.reveal && !frame.skip_fog && !facts.cell_visible(c, frame.team) {
                continue;
            }
            if !hidden_mine {
                ack.push(index);
            }
            accepted.push((row, owned));
        }
        let increment =
            u64::try_from(accepted.len()).map_err(|_| ObservedError::RevisionExhausted)?;
        self.revision
            .checked_add(increment)
            .ok_or(ObservedError::RevisionExhausted)?;
        Ok(PreparedRefresh {
            key: Some(key),
            ack: Some(ack),
            accepted,
        })
    }

    /// Forward full remembered rows in canonical order. Snapshot drain order
    /// must never be used as route order; authoritative arrays are not consulted.
    pub fn apply_routes(&self, routes: &mut RouteKnowledge) -> Result<(), ObservedError> {
        let query = routes.query();
        let terrain = query.navigation();
        if terrain.knowledge_id != self.identity
            || terrain.width != self.width
            || terrain.height != self.height
            || terrain.world_known
        {
            return Err(ObservedError::RecipientMismatch);
        }
        let rows = self
            .rows
            .iter()
            .map(ObservedRow::route_row)
            .collect::<Result<Vec<_>, _>>()?;
        routes.apply_rows(self.revision, &rows)?;
        Ok(())
    }
}
fn material(name: &str) -> Result<Material, ObservedError> {
    match name {
        "soil" => Ok(Material::Soil),
        "road" => Ok(Material::Road),
        "wood" => Ok(Material::Wood),
        "stone" => Ok(Material::Stone),
        "concrete" => Ok(Material::Concrete),
        "steel" => Ok(Material::Steel),
        "water" => Ok(Material::Water),
        _ => Err(ObservedError::InvalidValue),
    }
}
fn validate_row(row: &CellRow) -> Result<(), ObservedError> {
    if [row.ch, row.data.ground, row.data.object]
        .iter()
        .any(|ch| !b".BH#+TWFRKXYDMNOAQ=".contains(ch))
    {
        return Err(ObservedError::InvalidValue);
    }
    material(&row.data.material)?;
    material(&row.data.ground_material)?;
    if !row.data.ground_wear.is_finite()
        || row.data.trench_front.is_some_and(|v| !v.is_finite())
        || row.data.section.as_ref().is_some_and(|s| {
            !s.hp.is_finite()
                || s.hp < 0.0
                || !s.max_hp.is_finite()
                || s.max_hp < 0.0
                || material(&s.material).is_err()
        })
    {
        return Err(ObservedError::InvalidValue);
    }
    Ok(())
}
pub(crate) fn validate_world(world: &World) -> Result<(), ObservedError> {
    let g = &world.game;
    if g.mode == Some(ModeKind::World) {
        return Err(ObservedError::UnsupportedWorld);
    }
    let n =
        g.w.checked_mul(g.h)
            .filter(|&n| n > 0 && n <= i32::MAX as usize)
            .ok_or(ObservedError::InvalidShape)?;
    if [
        g.chars.len(),
        g.flags.len(),
        g.house.len(),
        g.cell_hp.len(),
        world.cell_state.len(),
        world.mine_layer.len(),
        world.surface_wear.len(),
        world.burnt.len(),
        world.scar.len(),
        world.initial().chars.len(),
        world.initial().ground.len(),
        world.initial().objects.len(),
        world.initial().mines.len(),
    ]
    .iter()
    .any(|&len| len != n)
        || g.ground.as_ref().is_none_or(|v| v.len() != n)
        || g.objects.as_ref().is_none_or(|v| v.len() != n)
        || g.wear.as_ref().is_none_or(|v| v.len() != n)
        || g.height.as_ref().is_some_and(|v| v.len() != n)
        || g.trench_front.as_ref().is_some_and(|v| v.len() != n)
        || g.tough.as_ref().is_some_and(|v| v.len() != n)
        || world.shoal.as_ref().is_some_and(|v| v.len() != n)
        || world
            .initial()
            .height
            .as_ref()
            .is_some_and(|v| v.len() != n)
        || world
            .initial()
            .trench_front
            .as_ref()
            .is_some_and(|v| v.len() != n)
    {
        return Err(ObservedError::InvalidShape);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::AuthoritativeMap;
    struct Seen;
    impl ObservationFacts for Seen {
        fn cell_visible(&mut self, _: usize, _: i32) -> bool {
            true
        }
        fn mine_known(&mut self, _: usize, _: i32) -> bool {
            false
        }
        fn mine_owned(&mut self, _: usize, _: i32) -> bool {
            false
        }
        fn footprint(&mut self, _: usize, _: usize, _: i32) -> Option<FootprintFact> {
            None
        }
    }
    fn world() -> World {
        let input = serde_json::json!({"name":"Guard","w":20,"h":20,"rows":vec![".".repeat(20);20],"spawns":[{"x":0,"y":0},{"x":19,"y":19}],"points":[{"x":10,"y":10}]});
        World::from_map(&AuthoritativeMap::from_json(&serde_json::to_vec(&input).unwrap()).unwrap())
            .unwrap()
    }
    #[test]
    fn preparation_preserves_deferred_registration_and_unchanged_key_actions() {
        let mut w = world();
        let mut memory = ObservedMemory::new(&w, 0, 7).unwrap();
        let frame = ObservationFrame {
            team: 0,
            skip_fog: false,
        };
        memory.preflight_refresh(&w, frame, &mut Seen).unwrap();
        assert!(w.pending(Recipient::Player(0)).is_none());
        assert_eq!(memory.key, None);
        let (count, receipt) = memory.refresh_impl(&w, frame, &mut Seen, true).unwrap();
        assert_eq!(count, 0);
        assert_eq!(memory.key, None);
        assert!(w.pending(Recipient::Player(0)).is_none());
        receipt.commit(&mut w).unwrap();
        assert!(w.pending(Recipient::Player(0)).unwrap().is_empty());

        w.set_cell(30, b'T').unwrap();
        let pending = w.pending(Recipient::Player(0)).unwrap().clone();
        let before = format!("{memory:?}");
        memory.preflight_refresh(&w, frame, &mut Seen).unwrap();
        assert_eq!(format!("{memory:?}"), before);
        assert_eq!(w.pending(Recipient::Player(0)), Some(&pending));
        // Drop a deferred acknowledgment to leave a nonempty pending set at
        // an unchanged collector key. Its next receipt must remain a no-op.
        let (count, receipt) = memory.refresh_impl(&w, frame, &mut Seen, false).unwrap();
        assert_eq!(count, 1);
        drop(receipt);
        let before = format!("{memory:?}");
        let (count, receipt) = memory.refresh_impl(&w, frame, &mut Seen, true).unwrap();
        assert_eq!(count, 0);
        assert_eq!(format!("{memory:?}"), before);
        receipt.commit(&mut w).unwrap();
        assert_eq!(w.pending(Recipient::Player(0)), Some(&pending));
    }
    #[test]
    fn exhaustion_and_invalid_input_leave_memory_and_pending_unchanged() {
        let mut w = world();
        let mut memory = ObservedMemory::new(&w, 0, 7).unwrap();
        w.register_recipient(Recipient::Player(0));
        w.set_cell(30, b'T').unwrap();
        w.set_cell(2, b'M').unwrap();
        let pending = w.pending(Recipient::Player(0)).unwrap().clone();
        memory.revision = u64::MAX - 1;
        let frame = ObservationFrame {
            team: 0,
            skip_fog: false,
        };
        assert_eq!(
            memory.refresh(&mut w, frame, &mut Seen),
            Err(ObservedError::RevisionExhausted)
        );
        assert!(memory.rows.is_empty());
        assert!(memory.outbox.is_empty());
        assert_eq!(memory.key, None);
        assert_eq!(memory.revision, u64::MAX - 1);
        assert_eq!(w.pending(Recipient::Player(0)), Some(&pending));
        memory.revision = 0;
        let saved = w.game.flags.pop().unwrap();
        assert_eq!(
            memory.refresh(&mut w, frame, &mut Seen),
            Err(ObservedError::InvalidShape)
        );
        w.game.flags.push(saved);
        w.materials.insert(30, "invalid".into());
        assert_eq!(
            memory.refresh(&mut w, frame, &mut Seen),
            Err(ObservedError::InvalidValue)
        );
        assert!(memory.rows.is_empty());
        assert!(memory.outbox.is_empty());
        assert_eq!(memory.key, None);
        assert_eq!(w.pending(Recipient::Player(0)), Some(&pending));
        w.materials.remove(&30);
        let mut other = world();
        assert_eq!(
            memory.refresh(&mut other, frame, &mut Seen),
            Err(ObservedError::InitialChanged)
        );
        w.game.mode = Some(ModeKind::World);
        assert_eq!(
            memory.refresh(&mut w, frame, &mut Seen),
            Err(ObservedError::UnsupportedWorld)
        );
    }
}
