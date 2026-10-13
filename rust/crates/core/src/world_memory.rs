//! World team memory with independent route and commander delivery cursors.
//! Rows enter memory only through the current fog mask (or explicit reveal).
//! This is internal knowledge, not a snapshot wire format or privacy contract.
use std::collections::{BTreeMap, BTreeSet};

use crate::{
    catalog::{Catalog, Value},
    fog::Fog,
    game::{ModeKind, Point, Wreck},
    observed_memory::ObservedRow,
    route_knowledge::{CoverBasis, LiveRouting, RouteKnowledge, RouteQuery},
    rules::Rules,
    world::World,
};

#[derive(Clone, Debug, PartialEq)]
pub struct WorldRow {
    pub observed: ObservedRow,
    pub initial: (u8, i8),
    pub biome: Option<u8>,
    pub landmark: Option<String>,
    /// Captured only when the source accepts a replacement row.
    pub cover: CoverBasis,
}
impl WorldRow {
    fn same_content(&self, other: &Self) -> bool {
        self.observed.row == other.observed.row
            && self.observed.mine_owned == other.observed.mine_owned
            && self.initial == other.initial
            && self.biome == other.biome
            && self.landmark == other.landmark
    }
    fn route_row(&self) -> Result<crate::route_knowledge::RememberedCell, String> {
        let mut row = self
            .observed
            .route_row()
            .map_err(|e| format!("World row: {e:?}"))?;
        row.cover_basis = Some(self.cover);
        Ok(row)
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct KnownRegion {
    pub id: i32,
    pub name: Option<String>,
    pub kind: Option<String>,
    pub team: i32,
    pub at: Point,
    pub bounds: Option<[f64; 4]>,
    pub runs: Vec<[usize; 3]>,
    pub progress: f64,
    pub capper: Option<usize>,
    pub locked: bool,
    pub contested: bool,
}

#[derive(Clone)]
pub struct TeamMemory {
    rows: Vec<WorldRow>,
    positions: BTreeMap<usize, usize>,
    version: u64,
    key: Option<(Option<i64>, u32, bool)>,
    seen: Option<Vec<u8>>,
    journal_at: Option<u64>,
    log: Vec<usize>,
    log_base: u64,
    path_at: Option<u64>,
    routes: RouteKnowledge,
    pub(crate) regions: Vec<KnownRegion>,
    pub(crate) wrecks: Vec<crate::recipient_lists::RememberedWreck>,
    pub(crate) wreck_queries: Vec<Wreck>,
}
impl TeamMemory {
    pub fn rows(&self) -> &[WorldRow] {
        &self.rows
    }
    pub fn contains_cell(&self, cell: usize) -> bool {
        self.positions.contains_key(&cell)
    }
    pub fn version(&self) -> u64 {
        self.version
    }
    pub fn regions(&self) -> &[KnownRegion] {
        &self.regions
    }
    pub fn remembered_wrecks(&self) -> &[Wreck] {
        &self.wreck_queries
    }
    pub fn remembered_wreck_rows(&self) -> Result<Vec<crate::recipient_lists::WreckRow>, String> {
        self.wrecks
            .iter()
            .map(|row| {
                row.display.clone().ok_or_else(|| {
                    format!(
                        "missing observed World wreck display facts: {}",
                        row.query.id.unwrap_or(0)
                    )
                })
            })
            .collect()
    }
    pub fn query(&self) -> RouteQuery<'_> {
        self.routes.query()
    }
    pub fn knowledge(&self) -> &RouteKnowledge {
        &self.routes
    }
    fn log_end(&self) -> Result<u64, String> {
        self.log_base
            .checked_add(self.log.len() as u64)
            .ok_or("World memory cursor exhausted".into())
    }
}
#[derive(Clone, Default)]
struct Sent {
    revisions: BTreeMap<usize, (i32, u64)>,
    at: Option<u64>,
}
#[derive(Clone, Copy)]
pub(crate) struct BoundUnit {
    pub id: u32,
    pub kind: usize,
    pub hp: f64,
    pub at: Point,
}
#[derive(Clone, Default)]
pub struct WorldLive {
    pub(crate) tick: Option<i64>,
    pub(crate) units: Vec<BoundUnit>,
    pub(crate) fires: Vec<bool>,
    pub(crate) bunkers: Vec<Point>,
    pub(crate) wrecks: Vec<Wreck>,
    pub(crate) wetness: f64,
    pub(crate) roads: bool,
}
impl WorldLive {
    pub fn live(&self) -> LiveRouting<'_> {
        LiveRouting {
            fires: &self.fires,
            bunkers: &self.bunkers,
            wetness: self.wetness,
            roads: self.roads,
        }
    }
    pub fn wrecks(&self) -> &[Wreck] {
        &self.wrecks
    }
}
/// Lazy source-order team memories. Local actor routing has no sent cursor.
#[derive(Clone, Default)]
pub struct WorldMemory {
    pub(crate) teams: Vec<(i32, TeamMemory)>,
    sent: BTreeMap<usize, Sent>,
    pub(crate) lives: BTreeMap<usize, WorldLive>,
    pub(crate) fog: Fog,
    binding: Option<(usize, usize, usize)>,
    visible_cells: BTreeMap<i32, (Option<i64>, Vec<usize>)>,
}
impl WorldMemory {
    pub(crate) fn validate_binding(&self, world: &World) -> Result<(), String> {
        let binding = (
            world.game.w,
            world.game.h,
            world.initial().chars.as_ptr() as usize,
        );
        if self.binding.is_some_and(|old| old != binding) {
            return Err("World memory belongs to another terrain".into());
        }
        Ok(())
    }
    pub fn team(&self, team: i32) -> Option<&TeamMemory> {
        self.teams.iter().find(|(t, _)| *t == team).map(|(_, m)| m)
    }
    pub(crate) fn team_mut(&mut self, team: i32) -> Option<&mut TeamMemory> {
        self.teams
            .iter_mut()
            .find(|(t, _)| *t == team)
            .map(|(_, m)| m)
    }
    pub fn live_for(&self, owner: usize) -> Option<&WorldLive> {
        self.lives.get(&owner)
    }
    pub fn refresh(
        &mut self,
        world: &mut World,
        rules: &Rules,
        team: i32,
        rails: &crate::match_world_rails::WorldRails,
    ) -> Result<usize, String> {
        let (count, receipt) = self.refresh_impl(world, rules, team, rails, None, false)?;
        receipt.commit(world)?;
        Ok(count)
    }
    pub(crate) fn refresh_impl(
        &mut self,
        world: &World,
        rules: &Rules,
        team: i32,
        rails: &crate::match_world_rails::WorldRails,
        observer: Option<crate::staged_observer::StagedObserver<'_>>,
        capture_frame: bool,
    ) -> Result<(usize, crate::world::ObservationReceipt), String> {
        validate(world, rules, team)?;
        if rails
            .rail_at
            .iter()
            .any(|(c, _)| *c >= world.game.chars.len())
            || rails
                .station_at
                .iter()
                .any(|(c, _)| *c >= world.game.chars.len())
        {
            return Err("invalid World rail observation cells".into());
        }
        let rail_at = rails.rail_at.iter().copied().collect::<BTreeMap<_, _>>();
        let station_at = rails.station_at.iter().copied().collect::<BTreeMap<_, _>>();
        let g = &world.game;
        let binding = (g.w, g.h, world.initial().chars.as_ptr() as usize);
        self.validate_binding(world)?;
        let end = world.journal_end().map_err(|e| e.to_string())?;
        let key = (g.vision_tick, g.terrain_version, g.reveal);
        let same_key = self.team(team).is_some_and(|m| m.key == Some(key));
        let fog = self.fog.team_with_observer(g, rules, team, observer);
        if same_key {
            return Ok((0, world.defer_journal_trim(None, capture_frame)?));
        }
        let vis = fog.vis.clone();
        let fog_key = fog.key;
        if self
            .visible_cells
            .get(&team)
            .is_none_or(|(k, _)| *k != fog_key)
        {
            self.visible_cells.insert(
                team,
                (
                    fog_key,
                    vis.iter()
                        .enumerate()
                        .filter_map(|(c, v)| (*v != 0).then_some(c))
                        .collect(),
                ),
            );
        }
        let visible_cells = &self.visible_cells[&team].1;
        let base = world.journal_base();
        let old = self.team(team);
        let full = g.reveal
            || old.is_none_or(|m| m.seen.is_none() || m.journal_at.is_none_or(|a| a < base));
        let mut visit = Vec::new();
        let mut visited = BTreeSet::new();
        if full && g.reveal {
            visit.extend(0..g.chars.len());
        } else {
            for &c in visible_cells {
                if full || old.is_none_or(|m| m.seen.as_ref().unwrap()[c] == 0) {
                    visited.insert(c);
                    visit.push(c);
                }
            }
        }
        if !full {
            let from = usize::try_from(old.unwrap().journal_at.unwrap() - base)
                .map_err(|_| "World journal cursor invalid")?;
            if from > world.journal().len() {
                return Err("World journal cursor is ahead of its source".into());
            }
            for &c in &world.journal()[from..] {
                if vis[c] != 0 && visited.insert(c) {
                    visit.push(c);
                }
            }
        }
        let mut updates = Vec::new();
        for c in visit {
            let row = seen_row(
                world,
                c,
                team,
                rail_at.get(&c).copied(),
                station_at.get(&c).copied(),
            )?;
            if old
                .and_then(|m| m.positions.get(&c).map(|i| &m.rows[*i]))
                .is_none_or(|r| !r.same_content(&row))
            {
                // Validate all route metadata before publishing any remembered row.
                row.route_row()?;
                updates.push(row);
            }
        }
        let count = updates.len() as u64;
        let version = old
            .map_or(0, |m| m.version)
            .checked_add(count)
            .ok_or("World memory revision exhausted")?;
        old.map_or(Ok(0), TeamMemory::log_end)?
            .checked_add(count)
            .ok_or("World memory cursor exhausted")?;
        let mut trim_at = end;
        for (t, m) in &self.teams {
            if *t != team
                && let Some(at) = m.journal_at
            {
                trim_at = trim_at.min(at);
            }
        }
        if world.journal().len() > 1 << 20 {
            trim_at = end;
        }
        // All possible arithmetic failures precede memory and source journal mutations.
        if world.journal().len() >= 65536 && trim_at > base && trim_at > end {
            return Err("World journal trim cursor invalid".into());
        }
        if old.is_none() {
            let routes =
                RouteKnowledge::world(g.w, g.h, (i64::from(team) + 2) as u64, rules.clone())
                    .map_err(|e| format!("World routes: {e:?}"))?;
            self.teams.push((
                team,
                TeamMemory {
                    rows: Vec::new(),
                    positions: BTreeMap::new(),
                    version: 0,
                    key: None,
                    seen: None,
                    journal_at: None,
                    log: Vec::new(),
                    log_base: 0,
                    path_at: None,
                    routes,
                    regions: Vec::new(),
                    wrecks: Vec::new(),
                    wreck_queries: Vec::new(),
                },
            ));
        }
        let m = self.team_mut(team).unwrap();
        for mut row in updates {
            m.version += 1;
            row.observed.revision = m.version;
            let c = row.observed.row.cell;
            m.log.push(c);
            if let Some(&i) = m.positions.get(&c) {
                m.rows[i] = row;
            } else {
                m.positions.insert(c, m.rows.len());
                m.rows.push(row);
            }
        }
        debug_assert_eq!(m.version, version);
        m.key = Some(key);
        m.seen = Some(vis);
        m.journal_at = Some(end);
        self.binding = Some(binding);
        let trim = (world.journal().len() >= 65536 && trim_at > base).then_some(trim_at);
        Ok((
            count as usize,
            world.defer_journal_trim(trim, capture_frame)?,
        ))
    }
    /// A full read preserves first insertion order; incremental reads resolve
    /// the current row identity at each log entry. Route reads use another cursor.
    pub fn terrain_for(
        &mut self,
        team: i32,
        owner: usize,
        full: bool,
    ) -> Result<Vec<WorldRow>, String> {
        let m = self
            .team(team)
            .ok_or("World memory has not been refreshed")?;
        let end = m.log_end()?;
        let threshold = m
            .rows
            .len()
            .checked_mul(4)
            .and_then(|v| v.checked_add(4096))
            .ok_or("World log limit exhausted")?;
        let sent = self.sent.get(&owner);
        let from = sent.and_then(|s| s.at).filter(|at| *at >= m.log_base);
        let indexes: Vec<_> = if let Some(at) = from.filter(|_| !full) {
            let start =
                usize::try_from(at - m.log_base).map_err(|_| "World sent cursor invalid")?;
            if start > m.log.len() {
                return Err("World sent cursor ahead of memory".into());
            }
            m.log[start..].iter().map(|c| m.positions[c]).collect()
        } else {
            (0..m.rows.len()).collect()
        };
        let mut revisions = sent.map_or_else(BTreeMap::new, |s| s.revisions.clone());
        let mut out = Vec::new();
        for i in indexes {
            let row = &m.rows[i];
            let c = row.observed.row.cell;
            if full || revisions.get(&c) != Some(&(team, row.observed.revision)) {
                out.push(row.clone());
                revisions.insert(c, (team, row.observed.revision));
            }
        }
        let trim = m.log.len() > threshold;
        self.sent.insert(
            owner,
            Sent {
                revisions,
                at: Some(end),
            },
        );
        if trim {
            let m = self.team_mut(team).unwrap();
            m.log_base = end;
            m.log.clear();
        }
        Ok(out)
    }
    pub fn apply_routes(&mut self, team: i32) -> Result<(), String> {
        let m = self
            .team_mut(team)
            .ok_or("World memory has not been refreshed")?;
        let end = m.log_end()?;
        let indexes: Vec<_> = if let Some(at) = m.path_at.filter(|a| *a >= m.log_base) {
            let start =
                usize::try_from(at - m.log_base).map_err(|_| "World path cursor invalid")?;
            if start > m.log.len() {
                return Err("World path cursor ahead of memory".into());
            }
            m.log[start..].iter().map(|c| m.positions[c]).collect()
        } else {
            (0..m.rows.len()).collect()
        };
        let rows = indexes
            .into_iter()
            .map(|i| m.rows[i].route_row())
            .collect::<Result<Vec<_>, _>>()?;
        m.routes
            .apply_rows(m.version, &rows)
            .map_err(|e| format!("World route rows: {e:?}"))?;
        m.path_at = Some(end);
        Ok(())
    }
}
fn compose(ground: u8, object: u8) -> u8 {
    if object == b'.' { ground } else { object }
}
fn seen_row(
    world: &World,
    c: usize,
    team: i32,
    rail: Option<u8>,
    station: Option<usize>,
) -> Result<WorldRow, String> {
    let mut row = world.cell_row(c).map_err(|e| e.to_string())?;
    row.data.rail = rail;
    row.data.station = station;
    let owner = world.mines.get(&c).copied().unwrap_or(-1);
    let owned = owner >= 0
        && world
            .game
            .players
            .get(owner as usize)
            .is_some_and(|p| p.team == team);
    let known = owned
        || world
            .mine_seen
            .get(&c)
            .is_some_and(|mask| mask & (1u32 << (team as u32 & 31)) != 0);
    let mine_owned = (row.ch == b'N' && known).then_some(owned);
    if row.ch == b'N' && !known {
        row.ch = compose(row.data.ground, row.data.object);
        row.data.mine = false;
        row.data.mine_material = None;
    }
    let initial = world.initial();
    let ch = if initial.chars[c] == b'N' {
        compose(initial.ground[c], initial.objects[c])
    } else {
        initial.chars[c]
    };
    let landmark = if world.game.house[c] >= 3 {
        Catalog::compiled()
            .map_err(|e| e.to_string())?
            .table("CFG")
            .map_err(|e| e.to_string())?
            .get("houses")
            .and_then(Value::array)
            .and_then(|v| v.get(world.game.house[c] as usize))
            .and_then(|v| v.get("name"))
            .and_then(Value::string)
            .map(str::to_owned)
    } else {
        None
    };
    Ok(WorldRow {
        observed: ObservedRow {
            row,
            mine_owned,
            revision: 0,
        },
        initial: (ch, initial.height.as_ref().map_or(0, |v| v[c])),
        biome: world
            .geography()
            .and_then(|w| w.biomes.as_ref())
            .map(|v| v[c])
            .filter(|v| *v < 255),
        landmark,
        cover: CoverBasis {
            house_kind: Some(world.game.house[c]),
            toughness: world.game.tough.as_ref().map_or(1.0, |v| v[c]),
        },
    })
}
pub(crate) fn validate(world: &World, rules: &Rules, team: i32) -> Result<(), String> {
    let g = &world.game;
    let n =
        g.w.checked_mul(g.h)
            .filter(|n| *n > 0 && *n <= i32::MAX as usize)
            .ok_or("invalid World shape")?;
    if g.mode != Some(ModeKind::World) || !(-1..=5).contains(&team) {
        return Err("invalid World knowledge team or mode".into());
    }
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
    .any(|v| *v != n)
        || g.ground.as_ref().is_none_or(|v| v.len() != n)
        || g.objects.as_ref().is_none_or(|v| v.len() != n)
        || g.wear.as_ref().is_none_or(|v| v.len() != n)
        || g.height.as_ref().is_some_and(|v| v.len() != n)
        || g.trench_front.as_ref().is_some_and(|v| v.len() != n)
        || g.tough
            .as_ref()
            .is_some_and(|v| v.len() != n || v.iter().any(|v| !v.is_finite() || *v < 0.0))
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
        || world.shoal.as_ref().is_some_and(|v| v.len() != n)
        || world.geography().is_some_and(|w| {
            w.biomes.as_ref().is_some_and(|v| v.len() != n)
                || w.region_map.as_ref().is_some_and(|v| {
                    v.len() != n
                        || v.iter()
                            .any(|i| *i < -1 || (*i >= 0 && *i as usize >= w.regions.len()))
                })
        })
        || g.house
            .iter()
            .any(|k| usize::from(*k) >= rules.movement.house_hp.len())
        || world.journal().iter().any(|c| *c >= n)
        || world.fires.iter().any(|(c, _)| *c >= n)
        || world.mines.iter().any(|(c, owner)| {
            *c >= n || *owner < -1 || (*owner >= 0 && *owner as usize >= g.players.len())
        })
        || world.mine_seen.keys().any(|c| *c >= n)
        || world
            .materials
            .keys()
            .chain(world.base_materials.keys())
            .chain(world.object_materials.keys())
            .chain(world.mine_materials.keys())
            .any(|c| *c >= n)
        || g.weather.is_some_and(|w| w >= rules.weather.len())
        || g.players.iter().any(|p| !(0..=5).contains(&p.team))
        || g.units.values().any(|u| {
            u.kind >= rules.kinds.len()
                || !u.x.is_finite()
                || !u.z.is_finite()
                || !u.hp.is_finite()
                || (u.owner >= g.players.len() && u.owner != crate::game::LOCAL_DEFENDER_OWNER)
        })
        || g.wet
            .is_some_and(|wet| !wet.is_finite() || !(0.0..=1.0).contains(&wet))
    {
        return Err("invalid World knowledge shape or facts".into());
    }
    world.journal_end().map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exhausted_revision_and_log_cursor_preserve_memory() {
        let input = serde_json::json!({"name":"guard map","w":20,"h":20,"rows":vec![".".repeat(20);20],"spawns":[{"x":0,"y":0},{"x":19,"y":19}],"points":[{"x":10,"y":10}]});
        let map =
            crate::map::AuthoritativeMap::from_json(&serde_json::to_vec(&input).unwrap()).unwrap();
        let mut world = World::from_map(&map).unwrap();
        world.game.mode = Some(ModeKind::World);
        world.game.reveal = true;
        let rules = Rules::compiled().unwrap();
        let mut memory = WorldMemory::default();
        memory
            .refresh(
                &mut world,
                &rules,
                0,
                &crate::match_world_rails::WorldRails::default(),
            )
            .unwrap();
        memory.team_mut(0).unwrap().version = u64::MAX;
        world.set_cell(0, b'T').unwrap();
        let before = memory.team(0).unwrap().rows.clone();
        let key = memory.team(0).unwrap().key;
        let journal = world.journal().to_vec();
        assert!(
            memory
                .refresh(
                    &mut world,
                    &rules,
                    0,
                    &crate::match_world_rails::WorldRails::default()
                )
                .is_err()
        );
        assert_eq!(memory.team(0).unwrap().rows, before);
        assert_eq!(memory.team(0).unwrap().key, key);
        assert_eq!(world.journal(), journal);
        memory.team_mut(0).unwrap().version = 400;
        memory.team_mut(0).unwrap().log_base = u64::MAX;
        assert!(
            memory
                .refresh(
                    &mut world,
                    &rules,
                    0,
                    &crate::match_world_rails::WorldRails::default()
                )
                .is_err()
        );
        assert_eq!(memory.team(0).unwrap().rows, before);
        assert_eq!(world.journal(), journal);
    }
}
