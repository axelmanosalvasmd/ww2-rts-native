//! Capture-point and point-supply phases, isolated from match startup, income and victory.
//! Unit slices retain the caller's live-unit order. Supply terrain is explicit and uncached.

use crate::{
    catalog::{Catalog, CatalogError, Value},
    economy::Mode,
    game::{CELL, MOVE, Point, Unit, VBLOCK},
    grid::UnitId,
    math,
    navigation::TerrainView,
    unit_spawn::{SpawnError, SpawnPlacer},
};

/// The point-supply beat is separate from the logistics supply network.
/// Horde, Tutorial and World never run this flood, even when supply is enabled.
pub fn point_supply_due(mode: Mode, enabled: bool, tick: i64) -> Result<bool, Error> {
    if tick < 0 {
        return Err(Error::InvalidInput("point supply tick"));
    }
    if !enabled || matches!(mode, Mode::Horde | Mode::Tutorial | Mode::World) {
        return Ok(false);
    }
    let every = Catalog::compiled()?
        .table("CFG")?
        .get("supply")
        .and_then(|v| v.get("every"))
        .and_then(Value::number)
        .filter(|v| v.is_finite() && *v >= 1.0 && v.fract() == 0.0)
        .ok_or(Error::InvalidInput("point supply beat"))?;
    Ok(tick % every as i64 == 1)
}

#[derive(Clone, Debug, PartialEq)]
pub struct Objective {
    pub id: serde_json::Value,
    pub at: Point,
    pub vp: f64,
    pub mp: f64,
    pub kind: Option<String>,
    pub needs: Option<usize>,
    pub owner: Option<usize>,
    pub capper: Option<usize>,
    pub progress: f64,
    pub cut: bool,
    pub on_point: Vec<UnitId>,
    pub contested: bool,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ObjectivePlayer {
    pub team: i32,
    pub out: bool,
    pub spawn: Point,
}
#[derive(Clone, Copy, Debug)]
pub struct ObjectiveUnit<'a> {
    pub id: UnitId,
    pub owner: usize,
    pub kind: &'a str,
    pub at: Point,
    pub hp: f64,
    pub airborne: bool,
    pub riding: bool,
    pub retreating: bool,
    pub forced_reserve: bool,
}
impl<'a> ObjectiveUnit<'a> {
    /// The caller resolves the kind name and the unrepresented forced-reserve state.
    pub fn from_unit(unit: &Unit, kind: &'a str, forced_reserve: bool) -> Self {
        Self {
            id: unit.id,
            owner: unit.owner,
            kind,
            at: Point {
                x: unit.x,
                z: unit.z,
            },
            hp: unit.hp,
            airborne: unit.air.is_some(),
            riding: unit.riding,
            retreating: unit.retreating,
            forced_reserve,
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct Captured {
    pub objective_index: usize,
    pub objective_id: serde_json::Value,
    pub owner: usize,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TeamSupply {
    pub team: i32,
    pub closed: Vec<u8>,
    pub reached: Vec<u8>,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ObjectiveRules {
    pub capture_time: f64,
    pub point_radius: f64,
    pub zoc_radius: f64,
    pub free_radius: f64,
}
impl ObjectiveRules {
    pub fn compiled() -> Result<Self, Error> {
        Self::from_catalog(Catalog::compiled()?)
    }
    pub fn from_catalog(catalog: &Catalog) -> Result<Self, Error> {
        fn number(v: &Value, key: &str) -> Result<f64, Error> {
            v.get(key)
                .and_then(Value::number)
                .filter(|v| v.is_finite())
                .ok_or(Error::InvalidInput("objective rule"))
        }
        let cfg = catalog.table("CFG")?;
        let supply = cfg
            .get("supply")
            .ok_or(Error::InvalidInput("supply rules"))?;
        let rules = Self {
            capture_time: number(cfg, "captureTime")?,
            point_radius: number(cfg, "pointRadius")?,
            zoc_radius: number(supply, "zoc")?,
            free_radius: number(supply, "free")?,
        };
        rules.validate()?;
        Ok(rules)
    }
    fn validate(self) -> Result<(), Error> {
        if !self.capture_time.is_finite()
            || self.capture_time <= 0.0
            || [self.point_radius, self.zoc_radius, self.free_radius]
                .iter()
                .any(|v| !v.is_finite() || *v < 0.0)
        {
            Err(Error::InvalidInput("objective rules"))
        } else {
            Ok(())
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Catalog(CatalogError),
    Placement(SpawnError),
    InvalidInput(&'static str),
}
impl From<CatalogError> for Error {
    fn from(v: CatalogError) -> Self {
        Self::Catalog(v)
    }
}
impl From<SpawnError> for Error {
    fn from(v: SpawnError) -> Self {
        Self::Placement(v)
    }
}
fn finite_point(p: Point) -> bool {
    p.x.is_finite() && p.z.is_finite()
}
fn validate(
    points: &[Objective],
    units: &[ObjectiveUnit<'_>],
    players: &[ObjectivePlayer],
    rules: ObjectiveRules,
) -> Result<(), Error> {
    rules.validate()?;
    for p in players {
        if !finite_point(p.spawn) {
            return Err(Error::InvalidInput("player spawn"));
        }
    }
    for p in points {
        if !finite_point(p.at) || ![p.vp, p.mp, p.progress].iter().all(|v| v.is_finite()) {
            return Err(Error::InvalidInput("objective value"));
        }
        if p.owner.is_some_and(|v| v >= players.len())
            || p.capper.is_some_and(|v| v >= players.len())
            || p.needs.is_some_and(|v| v >= points.len())
        {
            return Err(Error::InvalidInput("objective index"));
        }
    }
    let catalog = Catalog::compiled()?;
    for u in units {
        if u.owner >= players.len() || !finite_point(u.at) || !u.hp.is_finite() {
            return Err(Error::InvalidInput("unit fact"));
        }
        catalog.unit(u.kind)?;
    }
    Ok(())
}
fn allied(players: &[ObjectivePlayer], a: Option<usize>, b: usize) -> bool {
    a.is_some_and(|a| players[a].team == players[b].team)
}
fn dist(a: Point, b: Point) -> f64 {
    math::hypot(a.x - b.x, a.z - b.z)
}
fn infantry(kind: &str) -> Result<bool, Error> {
    Ok(Catalog::compiled()?
        .unit(kind)?
        .get("infantry")
        .and_then(Value::boolean)
        .unwrap_or(false))
}
fn armed(kind: &str) -> Result<bool, Error> {
    Ok(Catalog::compiled()?
        .unit(kind)?
        .get("w")
        .and_then(|w| w.get("range"))
        .and_then(Value::number)
        .is_some_and(|r| r != 0.0))
}

/// Capture points sequentially: a prerequisite captured earlier in this slice is immediately usable.
/// No tally or notification is applied here; each completed neutral capture returns one event.
/// Before the match is decided, the caller must stamp its capture tick and tally the capture
/// when consuming this event, matching story::captured. Decided matches still change ownership
/// in the reference but do not update the story. This component does not decide the match.
pub fn capture_points(
    points: &mut [Objective],
    units: &[ObjectiveUnit<'_>],
    players: &[ObjectivePlayer],
    dt: f64,
) -> Result<Vec<Captured>, Error> {
    capture_points_with_rules(points, units, players, dt, ObjectiveRules::compiled()?)
}
pub fn capture_points_with_rules(
    points: &mut [Objective],
    units: &[ObjectiveUnit<'_>],
    players: &[ObjectivePlayer],
    dt: f64,
    rules: ObjectiveRules,
) -> Result<Vec<Captured>, Error> {
    validate(points, units, players, rules)?;
    if !dt.is_finite() || dt < 0.0 {
        return Err(Error::InvalidInput("capture duration"));
    }
    let rate = dt / rules.capture_time;
    if !rate.is_finite() {
        return Err(Error::InvalidInput("capture duration"));
    }
    let mut events = Vec::new();
    for index in 0..points.len() {
        let at = points[index].at;
        let mut on = Vec::new();
        for u in units {
            if u.hp > 0.0
                && !players[u.owner].out
                && !u.retreating
                && !u.forced_reserve
                && !u.riding
                && infantry(u.kind)?
                && dist(u.at, at) <= rules.point_radius
            {
                on.push(u);
            }
        }
        points[index].on_point = on.iter().map(|u| u.id).collect();
        points[index].contested = on.first().is_some_and(|first| {
            on.iter()
                .any(|u| !allied(players, Some(u.owner), first.owner))
        });
        if on.is_empty() || points[index].contested {
            continue;
        }
        let owner = on[0].owner;
        if points[index]
            .needs
            .is_some_and(|n| !allied(players, points[n].owner, owner))
        {
            continue;
        }
        let p = &mut points[index];
        if allied(players, p.owner, owner) {
            p.progress = 1.0;
        } else if p.owner.is_some() {
            p.progress -= rate;
            if p.progress <= 0.0 {
                p.owner = None;
                p.progress = 0.0;
                p.capper = Some(owner);
            }
        } else {
            if !allied(players, p.capper, owner) {
                p.capper = Some(owner);
                p.progress = 0.0;
            }
            p.progress += rate;
            if p.progress >= 1.0 {
                p.owner = Some(owner);
                p.progress = 1.0;
                events.push(Captured {
                    objective_index: index,
                    objective_id: p.id.clone(),
                    owner,
                });
            }
        }
    }
    Ok(events)
}
fn validate_terrain(view: TerrainView<'_>) -> Result<usize, Error> {
    let n = view
        .width
        .checked_mul(view.height)
        .ok_or(Error::InvalidInput("terrain shape"))?;
    if n == 0
        || n > i32::MAX as usize
        || view.flags.len() != n
        || view.heights.is_some_and(|h| h.len() != n)
    {
        return Err(Error::InvalidInput("terrain shape"));
    }
    Ok(n)
}
fn level(view: TerrainView<'_>, cell: usize) -> i16 {
    view.heights.map_or(0, |h| i16::from(h[cell]))
}
fn bounds(at: f64, radius: f64, size: usize) -> Option<(usize, usize)> {
    let lo = math::max(0.0, (at / CELL).floor() - radius);
    let hi = math::min(size as f64 - 1.0, (at / CELL).floor() + radius);
    (lo <= hi).then_some((lo as usize, hi as usize))
}
/// The radius is a circle; a team's held points carve circular safe areas out of enemy control.
pub fn zoc_closed(
    view: TerrainView<'_>,
    units: &[ObjectiveUnit<'_>],
    players: &[ObjectivePlayer],
    team: i32,
    held: &[Point],
    rules: ObjectiveRules,
) -> Result<Vec<u8>, Error> {
    let n = validate_terrain(view)?;
    validate(&[], units, players, rules)?;
    if held.iter().any(|p| !finite_point(*p)) {
        return Err(Error::InvalidInput("held objective"));
    }
    let mut closed = vec![0; n];
    let radius = (rules.zoc_radius / CELL).ceil();
    for u in units {
        if u.hp <= 0.0
            || u.airborne
            || u.riding
            || u.retreating
            || players[u.owner].team == team
            || !armed(u.kind)?
        {
            continue;
        }
        let (Some((x0, x1)), Some((z0, z1))) = (
            bounds(u.at.x, radius, view.width),
            bounds(u.at.z, radius, view.height),
        ) else {
            continue;
        };
        for z in z0..=z1 {
            for x in x0..=x1 {
                let at = Point {
                    x: (x as f64 + 0.5) * CELL,
                    z: (z as f64 + 0.5) * CELL,
                };
                if dist(u.at, at) <= rules.zoc_radius
                    && !held.iter().any(|p| dist(*p, at) <= rules.free_radius)
                {
                    closed[z * view.width + x] = 1;
                }
            }
        }
    }
    Ok(closed)
}
/// Recompute point supply from current terrain. Call only when the match's supply phase is due.
/// Seeds use MOVE-only nearest-free placement and are reached even when closed or VBLOCK.
/// Point connectivity is tested against the capture square, matching the reference.
pub fn supply_lines(
    view: TerrainView<'_>,
    points: &mut [Objective],
    units: &[ObjectiveUnit<'_>],
    players: &[ObjectivePlayer],
    placer: &mut SpawnPlacer,
) -> Result<Vec<TeamSupply>, Error> {
    supply_lines_with_rules(
        view,
        points,
        units,
        players,
        placer,
        ObjectiveRules::compiled()?,
    )
}
pub fn supply_lines_with_rules(
    view: TerrainView<'_>,
    points: &mut [Objective],
    units: &[ObjectiveUnit<'_>],
    players: &[ObjectivePlayer],
    placer: &mut SpawnPlacer,
    rules: ObjectiveRules,
) -> Result<Vec<TeamSupply>, Error> {
    let n = validate_terrain(view)?;
    validate(points, units, players, rules)?;
    let mut cuts: Vec<_> = points.iter().map(|p| p.owner.is_some() && p.cut).collect();
    let mut teams = Vec::new();
    for p in points.iter() {
        if let Some(owner) = p.owner {
            let team = players[owner].team;
            if !teams.contains(&team) {
                teams.push(team);
            }
        }
    }
    let mut reports = Vec::new();
    let radius = (rules.point_radius / CELL).floor();
    for team in teams {
        let held: Vec<_> = points
            .iter()
            .filter(|p| p.owner.is_some_and(|owner| players[owner].team == team))
            .map(|p| p.at)
            .collect();
        let closed = zoc_closed(view, units, players, team, &held, rules)?;
        let mut reached = vec![0; n];
        let mut queue = Vec::new();
        for player in players {
            if player.team == team && !player.out {
                let cell = placer.nearest_free(view, player.spawn, MOVE, None)?;
                if reached[cell] == 0 {
                    reached[cell] = 1;
                    queue.push(cell);
                }
            }
        }
        let mut head = 0;
        while head < queue.len() {
            let cell = queue[head];
            head += 1;
            let x = cell % view.width;
            let adjacent = [
                (x > 0).then(|| cell - 1),
                (x + 1 < view.width).then_some(cell + 1),
                cell.checked_sub(view.width),
                (cell + view.width < n).then_some(cell + view.width),
            ];
            for next in adjacent.into_iter().flatten() {
                if reached[next] != 0
                    || view.flags[next] & (MOVE | VBLOCK) != 0
                    || closed[next] != 0
                    || (level(view, next) - level(view, cell)).abs() > 1
                {
                    continue;
                }
                reached[next] = 1;
                queue.push(next);
            }
        }
        for (index, p) in points.iter().enumerate() {
            if !p.owner.is_some_and(|owner| players[owner].team == team) {
                continue;
            }
            cuts[index] = true;
            let (Some((x0, x1)), Some((z0, z1))) = (
                bounds(p.at.x, radius, view.width),
                bounds(p.at.z, radius, view.height),
            ) else {
                continue;
            };
            'scan: for z in z0..=z1 {
                for x in x0..=x1 {
                    if reached[z * view.width + x] != 0 {
                        cuts[index] = false;
                        break 'scan;
                    }
                }
            }
        }
        reports.push(TeamSupply {
            team,
            closed,
            reached,
        });
    }
    for (p, cut) in points.iter_mut().zip(cuts) {
        p.cut = cut;
    }
    Ok(reports)
}
