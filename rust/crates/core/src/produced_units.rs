//! Ground-unit door placement and production projections. IDs, admission, wallets,
//! registration, combat/logistics initialization and actual rally routing belong to Match.
use crate::{
    catalog::{Catalog, CatalogError, Value},
    economy::Mode,
    game::{CELL, Point, Unit},
    grid::UnitId,
    math,
    navigation::TerrainView,
    rules::Rules,
    unit_spawn::{self, SpawnError, SpawnPlacer},
    vehicle_navigation::VehicleNavigation,
};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Spawn(SpawnError),
    Catalog(CatalogError),
    InvalidInput(&'static str),
    UnsupportedAir,
}
impl From<SpawnError> for Error {
    fn from(e: SpawnError) -> Self {
        Self::Spawn(e)
    }
}
impl From<CatalogError> for Error {
    fn from(e: CatalogError) -> Self {
        Self::Catalog(e)
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Door {
    pub at: Point,
    pub radius: f64,
    pub heading: f64,
    pub origin: Option<Point>,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StepOutPlacement {
    pub cell: usize,
    pub at: Point,
    pub heading: f64,
    pub aim: f64,
    pub ghost_until: f64,
}
#[derive(Clone, Copy, Debug)]
pub struct ProductionBuilding {
    pub at: Point,
    pub kind: usize,
    pub rally: Option<Point>,
}
#[derive(Clone, Copy, Debug)]
pub struct ProduceInput {
    pub id: UnitId,
    pub owner: usize,
    pub kind: usize,
    pub scatter_index: u32,
    pub player_spawn: Point,
    pub player_rally: Option<Point>,
    pub building: ProductionBuilding,
    pub mode: Mode,
    pub tick: u64,
}
/// Internal home-spawn facts. Tutorial has no production building to impersonate.
pub(crate) struct TutorialProduceInput {
    pub id: UnitId,
    pub owner: usize,
    pub kind: usize,
    pub scatter_index: u32,
    pub player_spawn: Point,
    pub player_rally: Option<Point>,
    pub tick: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RallyKind {
    Building,
    Player,
}
/// The integrator must route from the returned unit to this exact destination,
/// replace its path, and preserve the rally drift/order even if the route is empty.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RallyRequest {
    pub kind: RallyKind,
    pub destination: Point,
}
#[derive(Clone, Debug)]
pub struct ProducedPlacement {
    pub unit: Unit,
    pub aim: f64,
    pub rally: Option<RallyRequest>,
}
#[derive(Default)]
pub struct ProducedPlacer {
    spawn: SpawnPlacer,
    vehicles: VehicleNavigation,
}
fn point(p: Point) -> Result<(), Error> {
    if p.x.is_finite() && p.z.is_finite() {
        Ok(())
    } else {
        Err(Error::InvalidInput("position"))
    }
}
fn center(view: TerrainView<'_>, c: usize) -> Point {
    Point {
        x: (c % view.width) as f64 * CELL + CELL / 2.0,
        z: (c / view.width) as f64 * CELL + CELL / 2.0,
    }
}
fn number(v: &Value, key: &str) -> Result<f64, Error> {
    v.get(key)
        .and_then(Value::number)
        .filter(|n| n.is_finite())
        .ok_or_else(|| Error::Catalog(CatalogError(format!("invalid production rule {key}"))))
}
impl ProducedPlacer {
    pub fn new() -> Self {
        Self::default()
    }
    /// Source's non-building Tutorial purchase: scatter once, then step out
    /// within the home region and honor the player's rally independently of bases.
    pub(crate) fn produce_tutorial(
        &mut self,
        rules: &Rules,
        catalog: &Catalog,
        view: TerrainView<'_>,
        units: &BTreeMap<UnitId, Unit>,
        input: TutorialProduceInput,
    ) -> Result<ProducedPlacement, Error> {
        let profile = rules
            .kinds
            .get(input.kind)
            .ok_or(Error::InvalidInput("unit kind"))?;
        if profile.air {
            return Err(Error::UnsupportedAir);
        }
        if units.contains_key(&input.id) {
            return Err(Error::InvalidInput("unit id already exists"));
        }
        point(input.player_spawn)?;
        if let Some(to) = input.player_rally {
            point(to)?;
        }
        let scatter =
            self.spawn
                .scatter_position(view, profile, input.player_spawn, input.scatter_index)?;
        let mut unit = unit_spawn::initial_unit(
            rules,
            catalog,
            input.kind,
            input.id,
            input.owner,
            scatter.point,
        )?;
        let to = input.player_rally.unwrap_or(Point {
            x: view.width as f64 * CELL / 2.0,
            z: view.height as f64 * CELL / 2.0,
        });
        let heading = math::atan2(to.z - input.player_spawn.z, to.x - input.player_spawn.x);
        let placement = self.step_out(
            view,
            rules,
            input.kind,
            input.id,
            units,
            Door {
                at: input.player_spawn,
                radius: 4.0,
                heading,
                origin: Some(input.player_spawn),
            },
            input.tick,
        )?;
        unit.x = placement.at.x;
        unit.z = placement.at.z;
        unit.rot = placement.heading;
        unit.ghost = placement.ghost_until;
        let rally = input
            .player_rally
            .filter(|_| !profile.structure)
            .map(|destination| RallyRequest {
                kind: RallyKind::Player,
                destination,
            });
        if let Some(request) = rally {
            unit.world_goal = Some(request.destination);
            unit.drift = true;
        }
        Ok(ProducedPlacement {
            unit,
            aim: placement.aim,
            rally,
        })
    }
    /// Thirty ordered ring candidates, followed by the source fallback, which can
    /// still overlap units or be blocked when no free location exists.
    #[allow(clippy::too_many_arguments)]
    pub fn step_out(
        &mut self,
        view: TerrainView<'_>,
        rules: &Rules,
        kind: usize,
        excluded: UnitId,
        units: &BTreeMap<UnitId, Unit>,
        door: Door,
        tick: u64,
    ) -> Result<StepOutPlacement, Error> {
        let profile = rules
            .kinds
            .get(kind)
            .ok_or(Error::InvalidInput("unit kind"))?;
        if profile.air {
            return Err(Error::UnsupportedAir);
        }
        point(door.at)?;
        if let Some(origin) = door.origin {
            point(origin)?;
        }
        if !door.radius.is_finite()
            || door.radius < 0.0
            || !door.heading.is_finite()
            || !profile.radius.is_finite()
            || profile.radius < 0.0
        {
            return Err(Error::InvalidInput("door geometry"));
        }
        if tick > 9_007_199_254_740_991 {
            return Err(Error::InvalidInput("tick"));
        }
        // Validate the source shape before the internal clearance cache touches it.
        self.spawn
            .nearest_free(view, door.at, profile.block(), None)?;
        let region = door
            .origin
            .map(|origin| self.spawn.region_membership(view, origin, profile.block()))
            .transpose()?;
        let mut occupants = Vec::new();
        for unit in units.values() {
            if unit.id == excluded || unit.air.is_some() || unit.riding || unit.hp <= 0.0 {
                continue;
            }
            point(unit.at())?;
            if !unit.hp.is_finite() {
                return Err(Error::InvalidInput("unit health"));
            }
            let other = rules
                .kinds
                .get(unit.kind)
                .ok_or(Error::InvalidInput("occupant kind"))?;
            if !other.radius.is_finite() || other.radius < 0.0 {
                return Err(Error::InvalidInput("occupant radius"));
            }
            if math::hypot(unit.x - door.at.x, unit.z - door.at.z) < door.radius + 12.0 {
                occupants.push((unit.at(), other.radius));
            }
        }
        let nav = if profile.ground_vehicle() {
            self.vehicles.view(view, profile.radius, profile.block())
        } else {
            view
        };
        let mut chosen = None;
        for k in 0..30u32 {
            let turn =
                door.heading + f64::from((k % 6) >> 1) * 0.6 * if k & 1 != 0 { -1.0 } else { 1.0 };
            let far = door.radius + profile.radius * (1.0 + 2.0 * f64::from(k / 6));
            let at = Point {
                x: door.at.x + math::cos(turn) * far,
                z: door.at.z + math::sin(turn) * far,
            };
            let spot = self.spawn.nearest_free(nav, at, profile.block(), None)?;
            let at = center(view, spot);
            if region.as_ref().is_none_or(|labels| labels[spot])
                && occupants
                    .iter()
                    .all(|(p, r)| math::hypot(p.x - at.x, p.z - at.z) > (*r + profile.radius) * 0.8)
            {
                chosen = Some(spot);
                break;
            }
        }
        let cell = if let Some(cell) = chosen {
            cell
        } else {
            let far = door.radius + profile.radius;
            self.spawn.nearest_free(
                if door.origin.is_some() { view } else { nav },
                Point {
                    x: door.at.x + math::cos(door.heading) * far,
                    z: door.at.z + math::sin(door.heading) * far,
                },
                profile.block(),
                door.origin,
            )?
        };
        let cfg = Catalog::compiled()?.table("CFG")?;
        let ghost = tick as f64
            + math::round(
                number(cfg, "spawnGhost")?
                    / Catalog::compiled()?
                        .table("TICK")?
                        .number()
                        .ok_or(Error::InvalidInput("tick duration"))?,
            );
        if !ghost.is_finite() {
            return Err(Error::InvalidInput("ghost time"));
        }
        Ok(StepOutPlacement {
            cell,
            at: center(view, cell),
            heading: door.heading,
            aim: door.heading,
            ghost_until: ghost,
        })
    }
    /// Production building coordinates and rallies are borrowed facts. The new unit
    /// is returned without insertion or ID allocation. Air-base state is unsupported.
    pub fn produce(
        &mut self,
        rules: &Rules,
        catalog: &Catalog,
        view: TerrainView<'_>,
        units: &BTreeMap<UnitId, Unit>,
        input: ProduceInput,
    ) -> Result<ProducedPlacement, Error> {
        let profile = rules
            .kinds
            .get(input.kind)
            .ok_or(Error::InvalidInput("unit kind"))?;
        if profile.air {
            return Err(Error::UnsupportedAir);
        }
        if units.contains_key(&input.id) {
            return Err(Error::InvalidInput("unit id already exists"));
        }
        point(input.building.at)?;
        point(input.player_spawn)?;
        for to in [input.building.rally, input.player_rally]
            .into_iter()
            .flatten()
        {
            point(to)?;
        }
        let building = rules
            .kinds
            .get(input.building.kind)
            .ok_or(Error::InvalidInput("building kind"))?;
        let to = input
            .building
            .rally
            .or(input.player_rally)
            .unwrap_or(Point {
                x: view.width as f64 * CELL / 2.0,
                z: view.height as f64 * CELL / 2.0,
            });
        let heading = math::atan2(to.z - input.building.at.z, to.x - input.building.at.x);
        let scatter =
            self.spawn
                .scatter_position(view, profile, input.player_spawn, input.scatter_index)?;
        let mut unit = unit_spawn::initial_unit(
            rules,
            catalog,
            input.kind,
            input.id,
            input.owner,
            scatter.point,
        )?;
        let placement = self.step_out(
            view,
            rules,
            input.kind,
            input.id,
            units,
            Door {
                at: input.building.at,
                radius: building.radius + 2.0,
                heading,
                origin: None,
            },
            input.tick,
        )?;
        unit.x = placement.at.x;
        unit.z = placement.at.z;
        unit.rot = placement.heading;
        unit.ghost = placement.ghost_until;
        let rally = if let Some(destination) = input.building.rally {
            Some(RallyRequest {
                kind: RallyKind::Building,
                destination,
            })
        } else if input.mode.is_skirmish() && !profile.structure {
            input.player_rally.map(|destination| RallyRequest {
                kind: RallyKind::Player,
                destination,
            })
        } else {
            None
        };
        if let Some(request) = rally {
            unit.world_goal = Some(request.destination);
            unit.drift = true;
        }
        Ok(ProducedPlacement {
            unit,
            aim: placement.aim,
            rally,
        })
    }
}
