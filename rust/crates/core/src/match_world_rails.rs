//! World startup rail records from `setupRails` and `railYard` at reference `e4e579a`.
//! The caller supplies authoritative metadata and the current terrain before World base placement.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use crate::game::CELL;
use crate::{
    catalog::{Catalog, Value as CatalogValue},
    economy::Mode,
    game::{ModeKind, Point},
    grid::UnitId,
    match_commands::{Denial, MoveOutcome},
    match_runtime::Match,
    math,
    unit_publication::{PublicationParts, SpawnActor, StagedUnit, prepare_world_publication},
    unit_spawn::SpawnPlacer,
};

pub const RAIL_REACH: f64 = 30.0;
pub const RAIL_CARS: usize = 8;
pub const RAIL_LOAD: f64 = 8.0;
pub const RAIL_UNLOAD: f64 = 2.0;
pub const RAIL_EVERY: f64 = 20.0;

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RailPoint {
    pub cell: usize,
    pub at: Point,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RailRun {
    pub path: Vec<RailPoint>,
    pub next: usize,
    pub wait: f64,
    pub destination: usize,
    pub bridges: BTreeSet<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RailStep {
    Loading,
    Running,
    Unloading,
    Finished,
    Cut,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RailMotion {
    pub at: Point,
    pub rot: f64,
    pub speed: f64,
    pub step: RailStep,
}

pub const STATION_YARD: i64 = 5;
pub const RAIL_DIRS: [(i64, i64); 8] = [
    (1, 0),
    (1, 1),
    (0, 1),
    (-1, 1),
    (-1, 0),
    (-1, -1),
    (0, -1),
    (1, -1),
];

#[derive(Clone, Debug, PartialEq)]
pub struct WorldStation {
    pub region: usize,
    pub cell: usize,
    pub x: f64,
    pub z: f64,
    /// Mutable train departure deadlines by team. Startup has no pending departure.
    pub next: BTreeMap<i32, i64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorldRailLine {
    pub a: usize,
    pub b: usize,
    pub cells: Vec<usize>,
    pub regions: Vec<usize>,
    pub bridges: Vec<usize>,
}

impl WorldRailLine {
    /// A line carries this team's trains only while every region belongs to it
    /// and every original bridge cell still has a deck (`railOpen` at the pin).
    /// Read live terrain so collapse and repair cannot leave cached connectivity.
    pub fn is_open(&self, chars: &[u8], region_teams: &[i32], team: i32) -> bool {
        self.regions
            .iter()
            .all(|&id| region_teams.get(id) == Some(&team))
            && self.bridges.iter().all(|&c| chars.get(c) == Some(&b'='))
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct WorldRails {
    pub stations: Vec<WorldStation>,
    pub lines: Vec<WorldRailLine>,
    /// Source Map insertion order, with repeated track keys updating their existing mask.
    pub rail_at: Vec<(usize, u8)>,
    /// Source Map insertion order, with the last station at a cell supplying its region.
    pub station_at: Vec<(usize, usize)>,
    /// Source Set insertion order. Signed cells deliberately retain off-map and row-wrap values.
    pub yard: Vec<i64>,
}

fn array<'a>(value: &'a Value, field: &str) -> Result<&'a [Value], String> {
    value
        .as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| format!("World rails {field}: expected an array"))
}

fn index(value: &Value, limit: usize, field: &str) -> Result<usize, String> {
    value
        .as_f64()
        .filter(|v| v.is_finite() && v.fract() == 0. && *v >= 0. && *v < limit as f64)
        .map(|v| v as usize)
        .ok_or_else(|| format!("World rails {field}: index outside its source collection"))
}

fn indices(value: &Value, limit: usize, field: &str) -> Result<Vec<usize>, String> {
    array(value, field)?
        .iter()
        .map(|v| index(v, limit, field))
        .collect()
}

/// Preserve Map insertion order while replacing the value of a repeated key.
fn put<T>(
    rows: &mut Vec<(usize, T)>,
    positions: &mut BTreeMap<usize, usize>,
    key: usize,
    value: T,
) {
    if let Some(&position) = positions.get(&key) {
        rows[position].1 = value;
    } else {
        positions.insert(key, rows.len());
        rows.push((key, value));
    }
}

impl WorldRails {
    /// Source construction excludes track and the full signed station yard.
    pub fn excludes_footprint(&self, cells: &[usize]) -> bool {
        cells
            .iter()
            .any(|&cell| i64::try_from(cell).is_ok_and(|cell| self.yard.contains(&cell)))
    }

    /// Shortest currently open line sequence. Stable line order resolves equal costs.
    /// Region ownership and standing bridges are checked at command time.
    pub fn route(
        &self,
        team: i32,
        from: usize,
        to: usize,
        region_teams: &[i32],
        chars: &[u8],
        width: usize,
    ) -> Option<Vec<RailPoint>> {
        let source = self.stations.get(from)?.region;
        let target = self.stations.get(to)?.region;
        if width == 0 || source >= region_teams.len() || target >= region_teams.len() {
            return None;
        }
        let mut best: BTreeMap<usize, (usize, Option<(usize, usize)>)> = BTreeMap::new();
        best.insert(source, (0, None));
        let mut visited = BTreeSet::new();
        while let Some((cost, at)) = best
            .iter()
            .filter(|(region, _)| !visited.contains(*region))
            .map(|(&region, &(distance, _))| (distance, region))
            .min()
        {
            if at == target {
                break;
            }
            visited.insert(at);
            for (index, line) in self.lines.iter().enumerate() {
                let next = if line.a == at {
                    line.b
                } else if line.b == at {
                    line.a
                } else {
                    continue;
                };
                if next >= region_teams.len()
                    || line
                        .regions
                        .iter()
                        .any(|&r| region_teams.get(r) != Some(&team))
                    || line
                        .bridges
                        .iter()
                        .any(|&cell| chars.get(cell) != Some(&b'='))
                {
                    continue;
                }
                let distance = cost.checked_add(line.cells.len())?;
                if best.get(&next).is_some_and(|(old, _)| *old <= distance) {
                    continue;
                }
                best.insert(next, (distance, Some((at, index))));
            }
        }
        best.get(&target)?;
        let mut legs = Vec::new();
        let mut at = target;
        while at != source {
            let (_, Some((previous, line))) = best.get(&at)? else {
                return None;
            };
            let row = &self.lines[*line];
            let cells = if row.a == *previous {
                row.cells.clone()
            } else {
                row.cells.iter().rev().copied().collect()
            };
            legs.push(cells);
            at = *previous;
        }
        let mut path = Vec::new();
        for leg in legs.into_iter().rev() {
            for cell in leg {
                if path
                    .last()
                    .is_some_and(|point: &RailPoint| point.cell == cell)
                {
                    continue;
                }
                path.push(RailPoint {
                    cell,
                    at: Point {
                        x: (cell % width) as f64 * CELL + CELL * 0.5,
                        z: (cell / width) as f64 * CELL + CELL * 0.5,
                    },
                });
            }
        }
        Some(path)
    }
    /// Build startup state atomically from `WorldDefinition.metadata["rails"]`.
    /// Missing or source-falsy metadata leaves rails absent. Malformed records fail before
    /// returning any state. Region references are checked against the explicit geography count.
    pub fn from_metadata(
        rails: Option<&Value>,
        chars: &[u8],
        width: usize,
        height: usize,
        regions: usize,
    ) -> Result<Self, String> {
        let cells = width
            .checked_mul(height)
            .filter(|n| width > 0 && height > 0 && *n <= i32::MAX as usize)
            .ok_or_else(|| "World rails terrain: invalid dimensions".to_owned())?;
        if chars.len() != cells {
            return Err("World rails terrain: incorrect cell count".into());
        }
        let Some(rails) = rails else {
            return Ok(Self::default());
        };
        if rails.is_null()
            || rails == &Value::Bool(false)
            || rails.as_f64() == Some(0.)
            || rails.as_str() == Some("")
        {
            return Ok(Self::default());
        }
        if !rails.is_object() {
            return Err("World rails: expected an object".into());
        }
        let stations = array(&rails["stations"], "stations")?
            .iter()
            .map(|station| {
                let region = index(&station["region"], regions, "station region")?;
                let x = index(&station["x"], width, "station x")?;
                let y = index(&station["y"], height, "station y")?;
                Ok(WorldStation {
                    region,
                    cell: y * width + x,
                    x: (x as f64 + 0.5) * CELL,
                    z: (y as f64 + 0.5) * CELL,
                    next: BTreeMap::new(),
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let lines = array(&rails["lines"], "lines")?
            .iter()
            .map(|line| {
                let a = index(&line["a"], regions, "line a")?;
                let b = index(&line["b"], regions, "line b")?;
                let cells = indices(&line["cells"], cells, "line cells")?;
                let regions = indices(&line["regions"], regions, "line regions")?;
                let bridges = cells
                    .iter()
                    .copied()
                    .filter(|&c| chars[c] == b'=')
                    .collect();
                Ok(WorldRailLine {
                    a,
                    b,
                    cells,
                    regions,
                    bridges,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let mut out = Self {
            stations,
            lines,
            ..Self::default()
        };
        let mut track_positions: BTreeMap<usize, usize> = BTreeMap::new();
        for line in &out.lines {
            for pair in line.cells.windows(2) {
                let [a, b] = [pair[0], pair[1]];
                let delta = (
                    (b % width) as i64 - (a % width) as i64,
                    (b / width) as i64 - (a / width) as i64,
                );
                let Some(direction) = RAIL_DIRS.iter().position(|&d| d == delta) else {
                    // Source setup skips repeated and nonadjacent hops rather than joining them.
                    continue;
                };
                for (cell, bit) in [(a, direction), (b, (direction + 4) % 8)] {
                    let mask = track_positions
                        .get(&cell)
                        .map_or(0, |&position| out.rail_at[position].1);
                    put(
                        &mut out.rail_at,
                        &mut track_positions,
                        cell,
                        mask | 1 << bit,
                    );
                }
            }
        }
        let mut station_positions = BTreeMap::new();
        for station in &out.stations {
            put(
                &mut out.station_at,
                &mut station_positions,
                station.cell,
                station.region,
            );
        }
        out.yard = rail_yard(
            width as i64,
            out.rail_at.iter().map(|&(cell, _)| cell as i64),
            out.station_at.iter().map(|&(cell, _)| cell as i64),
        );
        Ok(out)
    }
}

impl RailRun {
    /// Advance the locomotive without consuming incidental container order.
    /// A cut is checked before crossing the next cell, including after loading.
    #[allow(clippy::too_many_arguments)]
    pub fn step(
        &mut self,
        at: Point,
        rot: f64,
        dt: f64,
        speed: f64,
        team: i32,
        chars: &[u8],
        mut region_team: impl FnMut(Point) -> Option<i32>,
    ) -> Result<RailMotion, String> {
        if !dt.is_finite()
            || dt <= 0.0
            || !speed.is_finite()
            || speed <= 0.0
            || !at.x.is_finite()
            || !at.z.is_finite()
            || !rot.is_finite()
            || !self.wait.is_finite()
            || self.path.is_empty()
            || self.next >= self.path.len()
        {
            return Err("invalid rail motion state".into());
        }
        if self.wait > 0.0 {
            self.wait -= dt;
            return Ok(RailMotion {
                at,
                rot,
                speed: 0.0,
                step: if self.wait <= 0.0 && self.next >= self.path.len() - 1 {
                    RailStep::Finished
                } else if self.next >= self.path.len() - 1 {
                    RailStep::Unloading
                } else {
                    RailStep::Loading
                },
            });
        }
        let mut position = at;
        let mut heading = rot;
        let mut budget = speed * dt;
        while budget > 0.0 && self.next < self.path.len() - 1 {
            let next = self.path[self.next + 1];
            if (self.bridges.contains(&next.cell) && chars.get(next.cell) != Some(&b'='))
                || region_team(next.at).is_some_and(|owner| owner != team)
            {
                return Ok(RailMotion {
                    at: position,
                    rot: heading,
                    speed: 0.0,
                    step: RailStep::Cut,
                });
            }
            let dx = next.at.x - position.x;
            let dz = next.at.z - position.z;
            let distance = math::hypot(dx, dz);
            let go = distance.min(budget);
            if distance > 1e-6 {
                heading = math::atan2(dz, dx);
                position.x += dx / distance * go;
                position.z += dz / distance * go;
            }
            budget -= go;
            if go >= distance {
                self.next += 1;
            } else {
                break;
            }
        }
        let step = if self.next >= self.path.len() - 1 {
            self.wait = RAIL_UNLOAD;
            RailStep::Unloading
        } else {
            RailStep::Running
        };
        Ok(RailMotion {
            at: position,
            rot: heading,
            speed: speed - budget / dt,
            step,
        })
    }
}

impl Match {
    /// Bind every World rail exception to a registered train and its actual cargo.
    pub(crate) fn validate_rail_tick_actor(&self, id: UnitId) -> Result<bool, String> {
        let unit = self
            .world
            .game
            .units
            .get(&id)
            .ok_or("missing rail validation actor")?;
        let kind = self
            .rules
            .kinds
            .get(unit.kind)
            .ok_or("invalid rail validation kind")?;
        let metadata = self
            .combat
            .get(id)
            .ok_or("missing rail validation combat")?;
        if kind.rail {
            let run = self.rail_runs.get(&id).ok_or("unregistered World train")?;
            if !unit.rail
                || !run.wait.is_finite()
                || run.path.is_empty()
                || run.next >= run.path.len()
            {
                return Err("invalid World rail run".into());
            }
            if metadata.carrier.is_some() || metadata.cargo.len() > RAIL_CARS {
                return Err("invalid World train cargo".into());
            }
            let mut seen = BTreeSet::new();
            for &rider in &metadata.cargo {
                let passenger = self
                    .world
                    .game
                    .units
                    .get(&rider)
                    .ok_or("missing World train passenger")?;
                if !seen.insert(rider)
                    || !passenger.riding
                    || self
                        .combat
                        .get(rider)
                        .is_none_or(|row| row.carrier != Some(id))
                {
                    return Err("invalid World train passenger binding".into());
                }
            }
            return Ok(true);
        }
        if unit.riding || metadata.carrier.is_some() {
            let train = metadata.carrier.ok_or("World rider lacks train")?;
            if !unit.riding
                || !self.rail_runs.contains_key(&train)
                || self
                    .world
                    .game
                    .units
                    .get(&train)
                    .is_none_or(|u| !self.rules.kinds[u.kind].rail)
                || self
                    .combat
                    .get(train)
                    .is_none_or(|row| !row.cargo.contains(&id))
            {
                return Err("invalid World rail rider relation".into());
            }
            return Ok(true);
        }
        if self.rail_runs.contains_key(&id) || !metadata.cargo.is_empty() {
            return Err("unbound World rail metadata".into());
        }
        Ok(false)
    }

    /// Source early rider branch: a live train carries the squad pose; a wreck ejects it.
    pub(crate) fn sync_rail_passenger(&mut self, id: UnitId, dt: f64) -> Result<bool, String> {
        let unit = self.world.game.units.get(&id).ok_or("missing rail rider")?;
        if !unit.riding {
            return Ok(false);
        }
        self.validate_rail_tick_actor(id)?;
        let carrier = self
            .combat
            .get(id)
            .and_then(|r| r.carrier)
            .ok_or("missing rail carrier")?;
        let train = self
            .world
            .game
            .units
            .get(&carrier)
            .ok_or("missing rail train")?;
        if train.hp > 0.0 {
            let pose = train.pose();
            let cooldown = self
                .abilities
                .get(id)
                .ok_or("missing rail rider ability")?
                .cooldown;
            if !cooldown.is_finite() || !dt.is_finite() || dt <= 0.0 {
                return Err("invalid rail rider timer".into());
            }
            let rider = self.world.game.units.get_mut(&id).unwrap();
            rider.x = pose.x;
            rider.z = pose.z;
            rider.rot = pose.rot;
            rider.supp = 0.0;
            rider.still = Some(0.0);
            self.firing
                .get_mut(id)
                .ok_or("missing rail rider firing")?
                .target_id = 0;
            self.abilities.get_mut(id).unwrap().cooldown -= dt;
            self.grid.update(id, rider);
            return Ok(true);
        }
        let source = unit.at();
        let kind = &self.rules.kinds[unit.kind];
        let cell = SpawnPlacer::new()
            .nearest_free(
                crate::match_runtime::terrain_view(&self.world.game),
                source,
                kind.block(),
                None,
            )
            .map_err(|e| format!("wrecked rail landing: {e:?}"))?;
        let at = Point {
            x: (cell % self.world.game.w) as f64 * CELL + CELL / 2.0,
            z: (cell / self.world.game.w) as f64 * CELL + CELL / 2.0,
        };
        let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
        let definition = catalog.unit(&kind.name).map_err(|e| e.to_string())?;
        let full = definition
            .get("models")
            .and_then(CatalogValue::number)
            .zip(definition.get("hpPer").and_then(CatalogValue::number))
            .map(|(a, b)| a * b)
            .ok_or("missing rail rider strength")?;
        let cfg = catalog.table("CFG").map_err(|e| e.to_string())?;
        let shore = cfg
            .get("shoreReach")
            .and_then(CatalogValue::number)
            .ok_or("missing shore reach")?;
        let evict = cfg
            .get("aid")
            .and_then(|v| v.get("evict"))
            .and_then(CatalogValue::number)
            .ok_or("missing ejection damage")?;
        if !full.is_finite() || !shore.is_finite() || !evict.is_finite() {
            return Err("invalid rail ejection rules".into());
        }
        self.combat
            .get_mut(carrier)
            .unwrap()
            .cargo
            .retain(|&other| other != id);
        self.combat.get_mut(id).unwrap().carrier = None;
        let rider = self.world.game.units.get_mut(&id).unwrap();
        rider.riding = false;
        rider.x = at.x;
        rider.z = at.z;
        rider.hp = if math::hypot(source.x - at.x, source.z - at.z) > shore {
            0.0
        } else {
            rider.hp - full * evict
        };
        self.grid.update(id, rider);
        Ok(false)
    }

    /// Death can happen after an earlier-ID passenger was already visited this turn.
    /// Eject before retirement so both carrier and any newly killed riders retire together.
    pub(crate) fn eject_dead_rail_passengers(
        &mut self,
    ) -> Result<Vec<crate::environment::Effect>, String> {
        let dead: Vec<_> = self
            .rail_runs
            .keys()
            .copied()
            .filter(|id| {
                self.world
                    .game
                    .units
                    .get(id)
                    .is_some_and(|unit| unit.hp <= 0.0)
            })
            .collect();
        let mut effects = Vec::new();
        for train in dead {
            let passengers = self
                .combat
                .get(train)
                .ok_or("missing dead train combat")?
                .cargo
                .clone();
            for rider in passengers {
                if self.world.game.units.contains_key(&rider) {
                    self.sync_rail_passenger(rider, self.rules.movement.tick)?;
                    let unit = &self.world.game.units[&rider];
                    effects.push(crate::environment::Effect::Hurt(crate::environment::Hurt {
                        unit: rider,
                        owner: unit.owner,
                        at: unit.at(),
                        killed: unit.hp <= 0.0,
                        from: None,
                        local: false,
                    }));
                }
            }
        }
        Ok(effects)
    }

    /// Generic combat retirement can remove a rider while its train remains live.
    pub(crate) fn retire_rail_links(&mut self, removed: &[UnitId]) {
        for &id in removed {
            self.rail_runs.remove(&id);
            for &train in self.rail_runs.keys() {
                if let Some(metadata) = self.combat.get_mut(train) {
                    metadata.cargo.retain(|&rider| rider != id);
                }
            }
        }
    }

    /// Admit and publish a World rail departure through the ordinary unit owners.
    /// The server command allowlist remains separate from this internal recorded intent.
    pub fn apply_rail_order(
        &mut self,
        owner: usize,
        ids: &[i64],
        at: Point,
    ) -> Result<Option<Denial>, String> {
        if self.options.mode != Mode::World
            || self.world.game.mode != Some(ModeKind::World)
            || self
                .world_runtime
                .as_ref()
                .is_none_or(|runtime| runtime.rails.stations.is_empty())
            || !at.x.is_finite()
            || !at.z.is_finite()
        {
            return Ok(Some(Denial::Blocked));
        }
        let Some(player) = self.world.game.players.get(owner) else {
            return Ok(Some(Denial::Blocked));
        };
        let team = player.team;
        let game = &self.world.game;
        let mut eligible = Vec::new();
        let mut commanded = Vec::new();
        let mut selected = BTreeSet::new();
        let command = crate::match_commands::Command::Rail {
            ids: Vec::new(),
            at,
        };
        for &raw in ids {
            let Some(id) = self.eligible_command_unit(raw, owner, &command) else {
                continue;
            };
            let unit = &game.units[&id];
            if !selected.insert(unit.id) {
                continue;
            }
            commanded.push(id);
            let kind = self
                .rules
                .kinds
                .get(unit.kind)
                .ok_or("invalid rail passenger kind")?;
            if unit.owner == owner
                && unit.hp > 0.0
                && !kind.air
                && !kind.naval
                && !kind.rail
                && !self.logistics.get(unit.id).is_some_and(|s| s.forced)
            {
                eligible.push(unit.id);
            }
        }
        if eligible.is_empty() {
            return Ok(self.rail_command_result(&commanded, Some(Denial::Needs)));
        }
        let runtime = self.world_runtime.as_ref().expect("checked World runtime");
        let stations = &runtime.rails.stations;
        let own: Vec<_> = stations
            .iter()
            .enumerate()
            .filter(|(_, s)| {
                runtime
                    .regions
                    .get(s.region)
                    .is_some_and(|r| r.team == team)
            })
            .collect();
        let origin = own.iter().copied().min_by(|(ai, a), (bi, b)| {
            let distance = |s: &WorldStation| {
                eligible
                    .iter()
                    .map(|id| {
                        let u = &game.units[id];
                        math::hypot(u.x - s.x, u.z - s.z)
                    })
                    .fold(f64::INFINITY, f64::min)
            };
            distance(a).total_cmp(&distance(b)).then(ai.cmp(bi))
        });
        let Some((origin_index, origin)) = origin else {
            return Ok(self.rail_command_result(&commanded, Some(Denial::Station)));
        };
        if eligible.iter().all(|id| {
            let u = &game.units[id];
            math::hypot(u.x - origin.x, u.z - origin.z) > RAIL_REACH
        }) {
            return Ok(self.rail_command_result(&commanded, Some(Denial::Station)));
        }
        let destination = own
            .iter()
            .copied()
            .filter(|(i, _)| *i != origin_index)
            .min_by(|(ai, a), (bi, b)| {
                math::hypot(a.x - at.x, a.z - at.z)
                    .total_cmp(&math::hypot(b.x - at.x, b.z - at.z))
                    .then(ai.cmp(bi))
            });
        let Some((destination_index, destination)) = destination else {
            return Ok(self.rail_command_result(&commanded, Some(Denial::Station)));
        };
        let destination_region = destination.region;
        if origin.next.get(&team).is_some_and(|&next| next > game.tick) {
            return Ok(self.rail_command_result(&commanded, Some(Denial::Cooldown)));
        }
        let teams: Vec<_> = runtime.regions.iter().map(|r| r.team).collect();
        let Some(path) = runtime.rails.route(
            team,
            origin_index,
            destination_index,
            &teams,
            &game.chars,
            game.w,
        ) else {
            return Ok(self.rail_command_result(&commanded, Some(Denial::RailCut)));
        };
        if path.is_empty() {
            return Ok(self.rail_command_result(&commanded, Some(Denial::RailCut)));
        }
        eligible.retain(|id| {
            let u = &game.units[id];
            math::hypot(u.x - origin.x, u.z - origin.z) <= RAIL_REACH
        });
        eligible.sort_by(|a, b| {
            let ua = &game.units[a];
            let ub = &game.units[b];
            math::hypot(ua.x - origin.x, ua.z - origin.z)
                .total_cmp(&math::hypot(ub.x - origin.x, ub.z - origin.z))
                .then(a.cmp(b))
        });
        eligible.truncate(RAIL_CARS);
        let mut garrison_exits = BTreeMap::new();
        for &id in &eligible {
            let unit = &game.units[&id];
            if unit.path_gen == u64::MAX
                || !self.commands.units.contains_key(&id)
                || self.combat.get(id).is_none()
                || self.firing.get(id).is_none()
                || unit.dig != self.field_works.is_active(id)
                || unit.entrench != self.field_works.membership(id).is_some()
            {
                return Err("invalid rail passenger metadata".into());
            }
            crate::match_construction::validate_build_activity(unit, &self.commands.units[&id])?;
            if unit.garrison.is_some() {
                let cell = SpawnPlacer::new()
                    .nearest_free(
                        crate::match_runtime::terrain_view(game),
                        unit.at(),
                        crate::game::MOVE,
                        None,
                    )
                    .map_err(|e| format!("rail garrison exit: {e:?}"))?;
                garrison_exits.insert(
                    id,
                    Point {
                        x: (cell % game.w) as f64 * CELL + CELL / 2.0,
                        z: (cell / game.w) as f64 * CELL + CELL / 2.0,
                    },
                );
            }
        }
        let kind = self.rules.kind("train").ok_or("missing train kind")?;
        let mut draft = StagedUnit::ordinary_with_actor(
            game,
            &self.rules,
            SpawnActor::Commander {
                owner,
                spawn: self
                    .start
                    .players
                    .get(owner)
                    .ok_or("missing rail commander")?
                    .spawn,
            },
            kind,
            self.next_unit_id,
            0,
            Some(Point {
                x: origin.x,
                z: origin.z,
            }),
        )
        .map_err(|e| format!("rail train staging: {e:?}"))?;
        draft.unit.rail = true;
        let train_id = draft.unit.id;
        if self.combat.get(train_id).is_some() || self.rail_runs.contains_key(&train_id) {
            return Err("rail train identity occupied".into());
        }
        let bridges = path
            .iter()
            .filter(|p| game.chars[p.cell] == b'=')
            .map(|p| p.cell)
            .collect();
        let command_tick = game.tick;
        prepare_world_publication(
            PublicationParts {
                units: &mut self.world.game.units,
                grid: &mut self.grid,
                controls: &mut self.commands.units,
                recruitment: &mut self.recruit_metadata,
                next_id: &mut self.next_unit_id,
            },
            &self.rules,
            self.start.players.len(),
            Some(ModeKind::World),
            vec![draft],
        )
        .map_err(|e| format!("rail train publication: {e:?}"))?
        .publish_with(|unit, _| {
            self.combat.initialize(unit.id);
            self.firing.initialize(unit.id);
            self.abilities.initialize(unit.id);
            self.recovery.initialize(unit.id);
        });
        self.rail_runs.insert(
            train_id,
            RailRun {
                path,
                next: 0,
                wait: RAIL_LOAD,
                destination: destination_region,
                bridges,
            },
        );
        let period = math::round(RAIL_EVERY / self.rules.movement.tick) as i64;
        self.world_runtime.as_mut().unwrap().rails.stations[origin_index]
            .next
            .insert(team, command_tick + period);
        for id in eligible {
            // Boarding uses the same identity/pose owners as ordinary carriers. The train
            // runs automatically, so a passenger's old path and commands cannot resume.
            if let Some(previous) = self.combat.get(id).and_then(|row| row.carrier)
                && let Some(row) = self.combat.get_mut(previous)
            {
                row.cargo.retain(|&rider| rider != id);
            }
            let unit = self
                .world
                .game
                .units
                .get_mut(&id)
                .ok_or("rail passenger vanished")?;
            let control = self
                .commands
                .units
                .get_mut(&id)
                .ok_or("missing rail passenger control")?;
            crate::match_runtime::cancel_owned_work_state(&mut self.field_works, unit, control)?;
            if let Some(position) = garrison_exits.get(&id) {
                unit.x = position.x;
                unit.z = position.z;
            }
            unit.garrison = None;
            unit.traffic = None;
            unit.traffic_wait = 0.0;
            unit.path.clear();
            unit.path_gen = unit
                .path_gen
                .checked_add(1)
                .ok_or("rail path generation exhausted")?;
            unit.world_goal = None;
            unit.amove = None;
            unit.attack = false;
            unit.board = 0;
            unit.retreating = false;
            unit.riding = true;
            self.grid.update(id, unit);
            control.orders.clear();
            control.attack_id = 0;
            control.follow = 0;
            control.face = None;
            control.nade = None;
            control.enter = None;
            control.fire_at = None;
            control.outcome = Some(MoveOutcome::Interrupted);
            control.outcome_tick = Some(command_tick);
            self.firing
                .get_mut(id)
                .ok_or("missing rail passenger firing")?
                .target_id = 0;
            self.combat
                .get_mut(id)
                .ok_or("missing rail passenger combat")?
                .carrier = Some(train_id);
            self.combat.get_mut(train_id).unwrap().cargo.push(id);
        }
        Ok(self.rail_command_result(&commanded, None))
    }

    /// Source mine() resets following before semantic refusals, including for
    /// air and naval units later excluded from the train. Native validation
    /// errors retain their pre-command state.
    fn rail_command_result(
        &mut self,
        commanded: &[UnitId],
        result: Option<Denial>,
    ) -> Option<Denial> {
        for id in commanded {
            self.commands.units.get_mut(id).unwrap().follow = 0;
        }
        result
    }

    /// One train's deterministic movement and final physical passenger placement.
    pub fn step_rail_train(&mut self, id: UnitId, dt: f64) -> Result<(), String> {
        let train = self.world.game.units.get(&id).ok_or("missing rail train")?;
        let team = self
            .world
            .game
            .players
            .get(train.owner)
            .ok_or("invalid rail owner")?
            .team;
        let mut run = self.rail_runs.get(&id).ok_or("missing rail run")?.clone();
        let at = train.at();
        let rot = train.rot;
        let speed = self
            .rules
            .kinds
            .get(train.kind)
            .ok_or("invalid train kind")?
            .speed;
        let motion = run.step(at, rot, dt, speed, team, &self.world.game.chars, |point| {
            self.world_region_at(point).ok().flatten().map(|r| r.team)
        })?;
        if matches!(motion.step, RailStep::Finished | RailStep::Cut) {
            self.unload_rail_train(id, motion.at, motion.rot)?;
            return Ok(());
        }
        self.rail_runs.insert(id, run);
        let train = self.world.game.units.get_mut(&id).unwrap();
        train.x = motion.at.x;
        train.z = motion.at.z;
        train.rot = motion.rot;
        train.move_speed = motion.speed;
        train.supp = 0.0;
        train.retreating = false;
        train.path.clear();
        self.grid.update(id, train);
        Ok(())
    }

    fn unload_rail_train(&mut self, id: UnitId, at: Point, rot: f64) -> Result<(), String> {
        let passengers = self
            .combat
            .get(id)
            .ok_or("missing train combat")?
            .cargo
            .clone();
        let count = passengers.len().max(1);
        let mut landings = Vec::new();
        for (i, &passenger) in passengers.iter().enumerate() {
            let Some(unit) = self.world.game.units.get(&passenger) else {
                continue;
            };
            let angle = rot
                + std::f64::consts::PI / 2.0
                + i as f64 / count as f64 * 2.0 * std::f64::consts::PI;
            let desired = Point {
                x: at.x + math::cos(angle) * 5.0,
                z: at.z + math::sin(angle) * 5.0,
            };
            let cell = SpawnPlacer::new()
                .nearest_free(
                    crate::match_runtime::terrain_view(&self.world.game),
                    desired,
                    self.rules.kinds[unit.kind].block(),
                    None,
                )
                .map_err(|e| format!("rail passenger landing: {e:?}"))?;
            landings.push((
                passenger,
                Point {
                    x: (cell % self.world.game.w) as f64 * CELL + CELL / 2.0,
                    z: (cell / self.world.game.w) as f64 * CELL + CELL / 2.0,
                },
            ));
        }
        for (passenger, position) in landings {
            let unit = self.world.game.units.get_mut(&passenger).unwrap();
            unit.x = position.x;
            unit.z = position.z;
            unit.riding = false;
            self.grid.update(passenger, unit);
            self.combat.get_mut(passenger).unwrap().carrier = None;
        }
        self.grid.remove_unit(id);
        self.world.game.units.remove(&id);
        self.commands.units.remove(&id);
        self.commands.requests.retire_unit(id);
        self.combat.remove(id);
        self.firing.remove(id);
        self.abilities.remove(id);
        self.recovery.remove(id);
        self.rail_runs.remove(&id);
        Ok(())
    }
}

/// Presentation yard cells in source Set insertion order, including signed off-map cells.
/// Inputs must come from the recipient's known tracks and stations.
pub fn rail_yard(
    width: i64,
    track: impl IntoIterator<Item = i64>,
    stations: impl IntoIterator<Item = i64>,
) -> Vec<i64> {
    let mut members = BTreeSet::new();
    let mut out = Vec::new();
    for cell in track {
        if members.insert(cell) {
            out.push(cell);
        }
    }
    for cell in stations {
        for dy in -STATION_YARD..=STATION_YARD {
            for dx in -STATION_YARD..=STATION_YARD {
                let yard_cell = cell + dy * width + dx;
                if members.insert(yard_cell) {
                    out.push(yard_cell);
                }
            }
        }
    }
    out
}

#[cfg(test)]
#[path = "match_world_rails_command_tests.rs"]
mod command_tests;

#[cfg(test)]
mod intent_tests {
    use super::*;
    use crate::{
        economy::{Faction, Mode},
        economy_state::ArmySize,
        map::AuthoritativeMap,
        match_runtime::{MatchOptions, PlayerConfig},
        random::MatchRandom,
        weather::WeatherChoice,
    };
    use serde_json::json;

    #[test]
    fn mov81_open_route_chooses_shortest_owned_lines_and_rejects_cut_bridge() {
        let mut rails = WorldRails {
            stations: [0, 2]
                .map(|region| WorldStation {
                    region,
                    cell: region * 2,
                    x: region as f64 * 4.0 + 1.0,
                    z: 1.0,
                    next: BTreeMap::new(),
                })
                .to_vec(),
            lines: vec![
                WorldRailLine {
                    a: 0,
                    b: 1,
                    cells: vec![0, 1, 2],
                    regions: vec![0, 1],
                    bridges: vec![],
                },
                WorldRailLine {
                    a: 1,
                    b: 2,
                    cells: vec![2, 3, 4],
                    regions: vec![1, 2],
                    bridges: vec![3],
                },
                WorldRailLine {
                    a: 0,
                    b: 2,
                    cells: vec![0, 8, 9, 10, 11, 12, 4],
                    regions: vec![0, 2],
                    bridges: vec![],
                },
            ],
            ..WorldRails::default()
        };
        let mut chars = vec![b'.'; 20];
        chars[3] = b'=';
        let cells: Vec<_> = rails
            .route(7, 0, 1, &[7, 7, 7], &chars, 10)
            .unwrap()
            .into_iter()
            .map(|point| point.cell)
            .collect();
        assert_eq!(cells, vec![0, 1, 2, 3, 4]);
        chars[3] = b'.';
        let bypass: Vec<_> = rails
            .route(7, 0, 1, &[7, 7, 7], &chars, 10)
            .unwrap()
            .into_iter()
            .map(|point| point.cell)
            .collect();
        assert_eq!(bypass, vec![0, 8, 9, 10, 11, 12, 4]);
        rails.lines.pop();
        assert!(rails.route(7, 0, 1, &[7, 7, 7], &chars, 10).is_none());
        assert!(rails.route(7, 0, 1, &[7, -1, 7], &[b'='; 20], 10).is_none());
    }

    #[test]
    fn mov82_train_waits_runs_cuts_before_a_changed_cell_then_finishes_unloading() {
        let points = (0..3)
            .map(|i| RailPoint {
                cell: i,
                at: Point {
                    x: 1.0 + i as f64 * 2.0,
                    z: 1.0,
                },
            })
            .collect::<Vec<_>>();
        let mut run = RailRun {
            path: points.clone(),
            next: 0,
            wait: RAIL_LOAD,
            destination: 2,
            bridges: BTreeSet::from([1]),
        };
        let mut chars = vec![b'.'; 3];
        chars[1] = b'=';
        let loading = run
            .step(points[0].at, 0.0, 4.0, 16.0, 1, &chars, |_| Some(1))
            .unwrap();
        assert_eq!(loading.step, RailStep::Loading);
        assert_eq!(loading.at, points[0].at);
        run.step(points[0].at, 0.0, 4.0, 16.0, 1, &chars, |_| Some(1))
            .unwrap();
        chars[1] = b'.';
        let cut = run
            .step(points[0].at, 0.0, 0.05, 16.0, 1, &chars, |_| Some(1))
            .unwrap();
        assert_eq!(cut.step, RailStep::Cut);
        assert_eq!(cut.at, points[0].at);
        chars[1] = b'=';
        let territory_cut = run
            .step(points[0].at, 0.0, 0.05, 16.0, 1, &chars, |_| Some(2))
            .unwrap();
        assert_eq!(territory_cut.step, RailStep::Cut);
        assert_eq!(territory_cut.at, points[0].at);
        let arrival = run
            .step(points[0].at, 0.0, 0.5, 16.0, 1, &chars, |_| Some(1))
            .unwrap();
        assert_eq!(arrival.step, RailStep::Unloading);
        assert_eq!(arrival.at, points[2].at);
        let done = run
            .step(arrival.at, arrival.rot, 2.0, 16.0, 1, &chars, |_| Some(1))
            .unwrap();
        assert_eq!(done.step, RailStep::Finished);
    }

    pub(super) fn world_for_rail_orders() -> Match {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/world-contact.json")).unwrap();
        let mut raw = fixture["map"].clone();
        let cells: Vec<_> = (32..=96).map(|x| 32 * 512 + x).collect();
        raw["world"]["rails"] = json!({
            "stations": [{"region":0,"x":32,"y":32},{"region":1,"x":96,"y":32}],
            "lines": [{"a":0,"b":1,"cells":cells,"regions":[0,1]}]
        });
        let map = AuthoritativeMap::from_json(&serde_json::to_vec(&raw).unwrap()).unwrap();
        let mut m = Match::new_world(
            &map,
            MatchOptions {
                seed: 701,
                mode: Mode::World,
                players: (0..2)
                    .map(|slot| PlayerConfig {
                        name: format!("p{slot}"),
                        team: slot,
                        faction: Faction::Usa,
                    })
                    .collect(),
                shuffle: false,
                army: ArmySize::Standard,
                weather: WeatherChoice::Map,
                weather_seed: None,
                living_weather: false,
                map_key: None,
                tech: true,
                supply: false,
                logistics: false,
                sandbox: false,
            },
            MatchRandom::new(701),
        )
        .unwrap();
        m.world_runtime.as_mut().unwrap().regions[1].team = 0;
        m
    }

    fn world_with_rail_passenger() -> (Match, UnitId, UnitId) {
        let mut m = world_for_rail_orders();
        let kind = m.rules.kind("conscript").unwrap();
        let id = m.next_unit_id;
        let draft = StagedUnit::ordinary_with_actor(
            &m.world.game,
            &m.rules,
            SpawnActor::Commander {
                owner: 0,
                spawn: m.start.players[0].spawn,
            },
            kind,
            id,
            0,
            Some(Point { x: 65.0, z: 65.0 }),
        )
        .unwrap();
        prepare_world_publication(
            PublicationParts {
                units: &mut m.world.game.units,
                grid: &mut m.grid,
                controls: &mut m.commands.units,
                recruitment: &mut m.recruit_metadata,
                next_id: &mut m.next_unit_id,
            },
            &m.rules,
            m.start.players.len(),
            Some(ModeKind::World),
            vec![draft],
        )
        .unwrap()
        .publish_with(|unit, _| {
            m.combat.initialize(unit.id);
            m.firing.initialize(unit.id);
            m.abilities.initialize(unit.id);
            m.recovery.initialize(unit.id);
        });
        assert_eq!(
            m.apply_rail_order(0, &[i64::from(id)], Point { x: 193.0, z: 65.0 })
                .unwrap(),
            None
        );
        let train = m.next_unit_id - 1;
        assert!(m.rail_runs.contains_key(&train));
        assert_eq!(m.world.game.units[&train].hp, 900.0);
        assert_eq!(m.combat.get(train).unwrap().cargo, vec![id]);
        assert_eq!(m.combat.get(id).unwrap().carrier, Some(train));
        assert_eq!(
            m.world_runtime.as_ref().unwrap().rails.stations[0].next[&0],
            400
        );
        assert_eq!(
            m.apply_rail_order(0, &[i64::from(id)], Point { x: 193.0, z: 65.0 })
                .unwrap(),
            Some(Denial::Cooldown)
        );
        (m, id, train)
    }

    #[test]
    fn mov82_world_tick_carries_and_unloads_a_real_passenger() {
        let (mut m, id, train) = world_with_rail_passenger();
        // The train disappears during the live ID loop. A later newborn actor must still turn.
        let sentinel = m.next_unit_id;
        let kind = m.rules.kind("conscript").unwrap();
        let draft = StagedUnit::ordinary_with_actor(
            &m.world.game,
            &m.rules,
            SpawnActor::Commander {
                owner: 0,
                spawn: m.start.players[0].spawn,
            },
            kind,
            sentinel,
            0,
            Some(Point { x: 67.0, z: 65.0 }),
        )
        .unwrap();
        prepare_world_publication(
            PublicationParts {
                units: &mut m.world.game.units,
                grid: &mut m.grid,
                controls: &mut m.commands.units,
                recruitment: &mut m.recruit_metadata,
                next_id: &mut m.next_unit_id,
            },
            &m.rules,
            m.start.players.len(),
            Some(ModeKind::World),
            vec![draft],
        )
        .unwrap()
        .publish_with(|unit, _| {
            m.combat.initialize(unit.id);
            m.firing.initialize(unit.id);
            m.abilities.initialize(unit.id);
            m.recovery.initialize(unit.id);
        });
        m.world.game.units.get_mut(&sentinel).unwrap().repath = 100.0;
        let mut sentinel_turned_after_train_left = false;
        let mut landing_train_pose = m.world.game.units[&train].pose();
        for _ in 0..440 {
            let before = m.world.game.units[&sentinel].repath;
            landing_train_pose = m.world.game.units[&train].pose();
            m.advance_tick().unwrap();
            if !m.rail_runs.contains_key(&train) {
                sentinel_turned_after_train_left = m.world.game.units[&sentinel].repath < before;
                break;
            }
        }
        assert!(sentinel_turned_after_train_left);
        assert!(!m.world.game.units.contains_key(&train));
        assert!(!m.world.game.units[&id].riding);
        let desired = Point {
            x: landing_train_pose.x
                + math::cos(landing_train_pose.rot + std::f64::consts::PI / 2.0) * 5.0,
            z: landing_train_pose.z
                + math::sin(landing_train_pose.rot + std::f64::consts::PI / 2.0) * 5.0,
        };
        let cell = SpawnPlacer::new()
            .nearest_free(
                crate::match_runtime::terrain_view(&m.world.game),
                desired,
                m.rules.kinds[m.world.game.units[&id].kind].block(),
                None,
            )
            .unwrap();
        // Ordinary post-turn settlement may move the rider inside its landing cell.
        let landed = &m.world.game.units[&id];
        assert_eq!(m.world.game.cell_of(landed.x, landed.z), Some(cell));
        assert_eq!(m.combat.get(id).unwrap().carrier, None);
    }

    #[test]
    fn mov82_unload_places_passenger_on_nearest_free_cell_of_five_metre_ring() {
        let (mut m, id, train) = world_with_rail_passenger();
        let at = Point { x: 193.0, z: 65.0 };
        let desired = Point {
            x: at.x,
            z: at.z + 5.0,
        };
        let cell = SpawnPlacer::new()
            .nearest_free(
                crate::match_runtime::terrain_view(&m.world.game),
                desired,
                m.rules.kinds[m.world.game.units[&id].kind].block(),
                None,
            )
            .unwrap();
        m.unload_rail_train(train, at, 0.0).unwrap();
        assert_eq!(
            m.world.game.units[&id].at(),
            Point {
                x: (cell % m.world.game.w) as f64 * CELL + CELL / 2.0,
                z: (cell / m.world.game.w) as f64 * CELL + CELL / 2.0,
            }
        );
        assert!(!m.world.game.units[&id].riding);
        assert!(!m.world.game.units.contains_key(&train));
        assert_eq!(m.combat.get(id).unwrap().carrier, None);
    }

    #[test]
    fn mov82_world_tick_ejects_and_retires_a_wrecked_train_without_stale_links() {
        use crate::match_combat::{DamageWorld, ExplosionSource};
        let (mut m, id, train) = world_with_rail_passenger();
        // A rider with a lower ID can take its early turn before the train is destroyed.
        assert!(m.sync_rail_passenger(id, m.rules.movement.tick).unwrap());
        let at = m.world.game.units[&train].at();
        let order: Vec<_> = m.world.game.units.keys().copied().collect();
        let mut damage = DamageWorld {
            world: &mut m.world,
            combat: &mut m.combat,
            grid: &mut m.grid,
            rules: &m.rules,
            logistics: &m.logistics,
            production: &m.production,
            mode: m.options.mode,
            tech_enabled: m.options.tech,
            environment: &mut m.environment,
            environment_seed: &mut m.start.environment_seed,
            next_id: &mut m.next_unit_id,
            unit_order: &order,
            wind_angle: m.start.wind.0,
            wind_strength: m.start.wind.1,
        };
        damage
            .explosion(
                at,
                0.5,
                ExplosionSource {
                    infantry: 0.0,
                    vehicle: 10_000.0,
                    terrain: None,
                    suppression: 0.0,
                    scar: 0.0,
                    anti_garrison: None,
                    weapon_kind: None,
                    off_map_support: false,
                },
                1,
                None,
                None,
                None,
                &mut m.firing,
            )
            .unwrap();
        assert!(m.world.game.units[&train].hp <= 0.0);
        m.advance_tick().unwrap();
        assert!(!m.rail_runs.contains_key(&train));
        assert!(!m.world.game.units.contains_key(&train));
        assert!(!m.world.game.units[&id].riding);
        assert_eq!(m.combat.get(id).unwrap().carrier, None);
        m.advance_tick().unwrap();
    }

    #[test]
    fn mov82_world_tick_prunes_a_dead_passenger_from_a_live_train() {
        let (mut m, id, train) = world_with_rail_passenger();
        m.world.game.units.get_mut(&id).unwrap().hp = 0.0;
        m.advance_tick().unwrap();
        assert!(!m.world.game.units.contains_key(&id));
        assert!(m.world.game.units.contains_key(&train));
        assert!(m.combat.get(train).unwrap().cargo.is_empty());
        m.advance_tick().unwrap();
    }
    #[test]
    fn loaded_world_train_elimination_retires_links_and_preserves_other_nations_trains() {
        for eliminated_owner in [0, 1] {
            let (mut m, rider, train) = world_with_rail_passenger();
            for region in &mut m.world_runtime.as_mut().unwrap().regions {
                region.team = 1 - eliminated_owner;
            }
            m.advance_tick().unwrap();
            assert!(m.start.economy.players[eliminated_owner as usize].out);
            if eliminated_owner == 0 {
                assert!(!m.world.game.units.contains_key(&rider));
                assert!(!m.world.game.units.contains_key(&train));
                assert!(!m.rail_runs.contains_key(&train));
                assert!(!m.commands.units.contains_key(&rider));
                assert!(!m.commands.units.contains_key(&train));
            } else {
                assert!(m.world.game.units[&rider].riding);
                assert_eq!(m.combat.get(rider).unwrap().carrier, Some(train));
                assert_eq!(m.combat.get(train).unwrap().cargo, vec![rider]);
                assert!(m.rail_runs.contains_key(&train));
            }
            m.advance_tick().unwrap();
        }
    }
}
