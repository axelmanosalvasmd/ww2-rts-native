//! Physical move destinations from `endSpots` and its helpers at `e4e579a`.
//! Server placement reads the authoritative game, as the source does. Capture
//! centers and hit times are explicit inputs absent from Game. Path searches
//! separately use recipient knowledge; placement does not choose their view.

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    catalog::{Catalog, CatalogError, Value},
    formation::{self, Item, Shape},
    game::{CELL, Game, MOVE, ModeKind, Point, Unit, VBLOCK, WIRE},
    grid::{SpatialGrid, UnitId},
    math,
    navigation::TerrainView,
    rules::Rules,
    sort,
};

const MAX_REQUESTS: usize = 100_000;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Request {
    pub id: UnitId,
    pub at: Point,
}

/// `grid` must index `game.units` at their current positions. Placement reads
/// the game's physical terrain, smoke and settled wrecks. Capture centers are
/// public map data.
pub struct Context<'a> {
    pub game: &'a Game,
    pub rules: &'a Rules,
    pub grid: &'a SpatialGrid,
    pub points: &'a [Point],
    /// Canonical `u.hitAt`, in ticks. A missing record has the source's old-time fallback.
    pub hit_at: &'a BTreeMap<UnitId, f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Error {
    InvalidShape,
    InvalidInput,
    UnsupportedWorld,
    MissingUnit(UnitId),
    InvalidUnit(UnitId),
    Catalog(CatalogError),
}
impl From<CatalogError> for Error {
    fn from(value: CatalogError) -> Self {
        Self::Catalog(value)
    }
}

#[derive(Clone, Copy)]
struct Behavior {
    gap: f64,
    cover_seek: f64,
    spacing: f64,
    vehicle_spacing: f64,
    point_radius: f64,
    threat_ticks: f64,
}
impl Behavior {
    fn compiled() -> Result<Self, Error> {
        let catalog = Catalog::compiled()?;
        let cfg = catalog.table("CFG")?;
        let b = cfg
            .get("behavior")
            .ok_or_else(|| CatalogError("missing rules behavior".into()))?;
        let number = |v: &Value, key: &str| {
            v.get(key)
                .and_then(Value::number)
                .ok_or_else(|| CatalogError(format!("missing rules number: {key}")))
        };
        let tick = catalog
            .table("TICK")?
            .number()
            .ok_or_else(|| CatalogError("missing rules tick".into()))?;
        Ok(Self {
            gap: number(b, "gap")?,
            cover_seek: number(b, "coverSeek")?,
            spacing: number(b, "spacing")?,
            vehicle_spacing: number(b, "vehicleSpacing")?,
            point_radius: number(cfg, "pointRadius")?,
            threat_ticks: number(b, "threatTime")? / tick,
        })
    }
    fn space(self, context: &Context<'_>, u: &Unit) -> f64 {
        if context.rules.kinds[u.kind].infantry {
            self.spacing
        } else {
            self.vehicle_spacing
        }
    }
}

fn finite(p: Point) -> bool {
    p.x.is_finite() && p.z.is_finite()
}
fn distance(a: Point, b: Point) -> f64 {
    math::hypot(a.x - b.x, a.z - b.z)
}
fn cell(view: TerrainView<'_>, p: Point) -> Option<usize> {
    let x = (p.x / CELL).floor();
    let y = (p.z / CELL).floor();
    (finite(p) && x >= 0.0 && y >= 0.0 && x < view.width as f64 && y < view.height as f64)
        .then(|| y as usize * view.width + x as usize)
}
fn level(view: TerrainView<'_>, p: Point) -> i16 {
    cell(view, p).map_or(0, |c| level_cell(view, c))
}
fn level_cell(view: TerrainView<'_>, c: usize) -> i16 {
    view.heights.map_or(0, |v| i16::from(v[c]))
}
fn terrain<'a>(context: &'a Context<'_>) -> TerrainView<'a> {
    let g = context.game;
    TerrainView {
        width: g.w,
        height: g.h,
        flags: &g.flags,
        heights: g.height.as_deref(),
        world_known: false,
        knowledge_id: 0,
        versions: Default::default(),
        changed: None,
        changes: &[],
    }
}
fn valid_owner(game: &Game, owner: usize) -> bool {
    if game.mode == Some(ModeKind::World) {
        crate::match_actor::checked_game_owner(game, owner).is_ok()
    } else {
        owner < game.players.len()
    }
}
fn validate(context: &Context<'_>, view: TerrainView<'_>) -> Result<(), Error> {
    let n = view
        .width
        .checked_mul(view.height)
        .ok_or(Error::InvalidShape)?;
    if n == 0
        || n > i32::MAX as usize
        || view.width != context.game.w
        || view.height != context.game.h
        || view.flags.len() != n
        || view.heights.is_some_and(|h| h.len() != n)
        || context.game.chars.len() != n
        || context.game.house.len() != n
        || context.game.cell_hp.len() != n
        || context.game.ground.as_ref().is_some_and(|v| v.len() != n)
        || context.game.objects.as_ref().is_some_and(|v| v.len() != n)
        || context.game.wear.as_ref().is_some_and(|v| v.len() != n)
        || context.game.tough.as_ref().is_some_and(|v| v.len() != n)
        || context
            .game
            .trench_front
            .as_ref()
            .is_some_and(|v| v.len() != n)
    {
        return Err(Error::InvalidShape);
    }
    if view.world_known {
        return Err(Error::UnsupportedWorld);
    }
    if context.points.iter().any(|p| !finite(*p))
        || context
            .game
            .wrecks
            .iter()
            .any(|w| !w.x.is_finite() || !w.z.is_finite())
        || context
            .game
            .smokes
            .iter()
            .any(|s| !s.x.is_finite() || !s.z.is_finite() || !s.r.is_finite() || s.r < 0.0)
    {
        return Err(Error::InvalidInput);
    }
    for u in context.game.units.values() {
        if !finite(u.at())
            || u.kind >= context.rules.kinds.len()
            || !valid_owner(context.game, u.owner)
            || !u.hp.is_finite()
            || u.path.iter().any(|p| !finite(*p))
            || u.hit_from.is_some_and(|p| !finite(p))
        {
            return Err(Error::InvalidUnit(u.id));
        }
    }
    Ok(())
}
fn unit<'a>(context: &'a Context<'_>, id: UnitId) -> Result<&'a Unit, Error> {
    context.game.units.get(&id).ok_or(Error::MissingUnit(id))
}
fn rank(context: &Context<'_>, at: Point, from: Option<Point>) -> u8 {
    crate::terrain::cover_rank(context.game, context.rules, at, from)
}
fn in_points(context: &Context<'_>, b: Behavior, at: Point) -> Vec<Point> {
    context
        .points
        .iter()
        .copied()
        .filter(|p| distance(*p, at) <= b.point_radius)
        .collect()
}

/// Source bucket-order ties are retained. Only visible armed enemies contribute.
pub fn threat_for(context: &Context<'_>, id: UnitId, at: Point) -> Result<Option<Point>, Error> {
    validate(context, terrain(context))?;
    if !finite(at) {
        return Err(Error::InvalidInput);
    }
    threat(context, Behavior::compiled()?, unit(context, id)?, at)
}
/// Physical placement uses the selected actor's canonical visibility, including World locals.
/// This borrowed input does not refresh visibility or choose a remembered routing view.
pub fn threat_for_with_visibility(
    context: &Context<'_>,
    id: UnitId,
    at: Point,
    visible: &[UnitId],
) -> Result<Option<Point>, Error> {
    validate(context, terrain(context))?;
    if !finite(at) {
        return Err(Error::InvalidInput);
    }
    threat_visible(
        context,
        Behavior::compiled()?,
        unit(context, id)?,
        at,
        |id| visible.contains(&id),
    )
}
fn threat(context: &Context<'_>, b: Behavior, u: &Unit, at: Point) -> Result<Option<Point>, Error> {
    let player = context
        .game
        .players
        .get(u.owner)
        .ok_or(Error::UnsupportedWorld)?;
    threat_visible(context, b, u, at, |id| player.sees(id))
}
fn threat_visible(
    context: &Context<'_>,
    b: Behavior,
    u: &Unit,
    at: Point,
    sees: impl Fn(UnitId) -> bool,
) -> Result<Option<Point>, Error> {
    let g = context.game;
    let mut best = None;
    let mut best_distance = 50.0;
    for id in context
        .grid
        .unordered(&g.units, (at.x, at.z), 50.0, |id, v| {
            v.hp > 0.0
                && v.air.is_none()
                && context.rules.kinds[v.kind].weapon
                && !g.allied(v.owner, u.owner)
                && sees(id)
        })
    {
        let v = &g.units[&id];
        let d = distance(v.at(), at);
        if d < best_distance {
            best_distance = d;
            best = Some(v.at());
        }
    }
    if best.is_some() {
        return Ok(best);
    }
    let hit_at = context.hit_at.get(&u.id).copied().unwrap_or(-1e9);
    if !hit_at.is_finite() {
        return Err(Error::InvalidUnit(u.id));
    }
    if let Some(from) = u.hit_from
        && g.tick as f64 - hit_at <= b.threat_ticks
    {
        return Ok(Some(from));
    }
    let d = distance(u.at(), at);
    Ok((d > 0.5).then(|| Point {
        x: at.x + (at.x - u.x) / d * 20.0,
        z: at.z + (at.z - u.z) / d * 20.0,
    }))
}

/// Held destinations shared by explicit orders and voluntary reactions.
pub fn held_destinations(
    context: &Context<'_>,
    skip: &BTreeSet<UnitId>,
    at: Point,
    radius: f64,
    owner: usize,
) -> Result<Vec<Point>, Error> {
    validate(context, terrain(context))?;
    if !finite(at) || !radius.is_finite() || radius < 0.0 || !valid_owner(context.game, owner) {
        return Err(Error::InvalidInput);
    }
    Ok(held_spots(
        context,
        Behavior::compiled()?,
        skip,
        at,
        radius,
        owner,
    ))
}
fn held_spots(
    context: &Context<'_>,
    b: Behavior,
    skip: &BTreeSet<UnitId>,
    at: Point,
    radius: f64,
    owner: usize,
) -> Vec<Point> {
    let reach = radius + b.vehicle_spacing;
    context
        .game
        .units
        .values()
        .filter_map(|v| {
            if skip.contains(&v.id)
                || v.hp <= 0.0
                || v.air.is_some()
                || v.riding
                || v.garrison.is_some()
                || context.rules.kinds[v.kind].structure
                || !context.game.allied(v.owner, owner)
            {
                return None;
            }
            let end = v.path.last().copied().unwrap_or_else(|| v.at());
            ((end.x - at.x).abs() <= reach
                && (end.z - at.z).abs() <= reach
                && distance(end, at) <= reach)
                .then_some(end)
        })
        .collect()
}

/// A reachable free cell. `space=None` selects this unit's normal spacing.
#[allow(clippy::too_many_arguments)]
pub fn spot_near(
    context: &Context<'_>,
    id: UnitId,
    at: Point,
    radius: f64,
    held: &[Point],
    from: Option<Point>,
    beat: u8,
    space: Option<f64>,
) -> Result<Option<Point>, Error> {
    let view = terrain(context);
    validate(context, view)?;
    if !finite(at)
        || !radius.is_finite()
        || radius < 0.0
        || held.iter().any(|p| !finite(*p))
        || from.is_some_and(|p| !finite(p))
        || space.is_some_and(|s| !s.is_finite() || s < 0.0)
    {
        return Err(Error::InvalidInput);
    }
    near(
        context,
        view,
        Behavior::compiled()?,
        unit(context, id)?,
        at,
        radius,
        held,
        from,
        beat,
        space,
    )
}
#[allow(clippy::too_many_arguments)]
fn near(
    context: &Context<'_>,
    view: TerrainView<'_>,
    b: Behavior,
    u: &Unit,
    at: Point,
    radius: f64,
    held: &[Point],
    from: Option<Point>,
    beat: u8,
    space: Option<f64>,
) -> Result<Option<Point>, Error> {
    let r = (radius / CELL).ceil();
    let cx = (at.x / CELL).floor();
    let cy = (at.z / CELL).floor();
    let lv = level(view, at);
    let mask = block(context, u);
    let points = in_points(context, b, at);
    let space = space.unwrap_or_else(|| b.space(context, u));
    let mut best = None;
    let mut best_score = f64::INFINITY;
    let min_y = math::max(0.0, cy - r) as usize;
    let max_y = math::min(view.height as f64 - 1.0, cy + r);
    let min_x = math::max(0.0, cx - r) as usize;
    let max_x = math::min(view.width as f64 - 1.0, cx + r);
    if max_y < min_y as f64 || max_x < min_x as f64 {
        return Ok(None);
    }
    for y in min_y..=max_y as usize {
        for x in min_x..=max_x as usize {
            let c = y * view.width + x;
            let p = Point {
                x: (x as f64 + 0.5) * CELL,
                z: (y as f64 + 0.5) * CELL,
            };
            let d = distance(p, at);
            if view.flags[c] & mask != 0
                || d > radius
                || d >= best_score
                || (level_cell(view, c) - lv).abs() > 1
            {
                continue;
            }
            let rank = if beat > 3 { 0 } else { rank(context, p, from) };
            let mut score = d + f64::from(rank) * 4.0;
            if rank >= beat
                || score >= best_score
                || held.iter().any(|h| {
                    if space != 0.0 {
                        distance(*h, p) < space
                    } else {
                        cell(view, *h) == Some(c)
                    }
                })
                || points
                    .iter()
                    .any(|q| distance(*q, p) > b.point_radius - 1.0)
            {
                continue;
            }
            if rank == 2
                && from.is_some_and(|from| {
                    !crate::vision::los(context.game, context.rules, (p.x, p.z), (from.x, from.z))
                })
            {
                score += 3.0;
                if score >= best_score {
                    continue;
                }
            }
            if !straight(context, u, at, p) {
                continue;
            }
            best_score = score;
            best = Some(p);
        }
    }
    Ok(best)
}

/// Source facing line, with each output kept in request order.
pub fn spread_facing(
    context: &Context<'_>,
    group: &[Request],
    face: f64,
) -> Result<Vec<Request>, Error> {
    validate(context, terrain(context))?;
    validate_requests(context, group, Some(face))?;
    let mut out = group.to_vec();
    spread(context, &mut out, face)?;
    Ok(out)
}

/// Spread shared queued destinations now, since they replay one unit at a time.
/// Different clicks and air requests keep their own positions.
pub fn spread_queued_facing(
    context: &Context<'_>,
    requests: &[Request],
    face: f64,
) -> Result<Vec<Request>, Error> {
    validate(context, terrain(context))?;
    validate_requests(context, requests, Some(face))?;
    let mut out = requests.to_vec();
    for group in groups(context, requests, true, true) {
        if group.len() < 2 {
            continue;
        }
        let mut rows: Vec<_> = group.iter().map(|&i| out[i]).collect();
        spread(context, &mut rows, face)?;
        for (i, row) in group.into_iter().zip(rows) {
            out[i] = row;
        }
    }
    Ok(out)
}
fn spread(context: &Context<'_>, group: &mut [Request], face: f64) -> Result<(), Error> {
    let Some(first) = group.first().copied() else {
        return Ok(());
    };
    let items = group
        .iter()
        .map(|o| {
            let u = unit(context, o.id)?;
            let kind = &context.rules.kinds[u.kind];
            Ok(Item {
                id: u.id,
                x: u.x,
                z: u.z,
                size: formation::slot_size(kind.infantry, kind.radius),
                back: false,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let spots: BTreeMap<_, _> =
        formation::facing_spots(&items, first.at.x, first.at.z, face, 0.0, Shape::Line, 1.0)
            .into_iter()
            .map(|s| (s.id, Point { x: s.x, z: s.z }))
            .collect();
    for o in group {
        let p = spots.get(&o.id).ok_or(Error::InvalidInput)?;
        o.at = Point {
            x: math::min(context.game.w as f64 * CELL, math::max(0.0, p.x)),
            z: math::min(context.game.h as f64 * CELL, math::max(0.0, p.z)),
        };
    }
    Ok(())
}
fn validate_requests(
    context: &Context<'_>,
    go: &[Request],
    face: Option<f64>,
) -> Result<(), Error> {
    if go.len() > MAX_REQUESTS || face.is_some_and(|a| !a.is_finite()) {
        return Err(Error::InvalidInput);
    }
    for o in go {
        if !finite(o.at)
            || o.at.x < 0.0
            || o.at.z < 0.0
            || o.at.x > context.game.w as f64 * CELL
            || o.at.z > context.game.h as f64 * CELL
        {
            return Err(Error::InvalidInput);
        }
        unit(context, o.id)?;
    }
    Ok(())
}

fn groups(
    context: &Context<'_>,
    requests: &[Request],
    exact: bool,
    ground_only: bool,
) -> Vec<Vec<usize>> {
    let mut groups: Vec<Vec<usize>> = Vec::new();
    let mut group_indices: BTreeMap<(u64, u64), usize> = BTreeMap::new();
    for (i, o) in requests.iter().enumerate() {
        if ground_only && context.game.units[&o.id].air.is_some() {
            continue;
        }
        let key = if exact {
            o.at
        } else {
            Point {
                x: math::round(o.at.x),
                z: math::round(o.at.z),
            }
        };
        // Numeric keys reproduce source string equality, including +0 and -0.
        let bits = |v: f64| if v == 0.0 { 0 } else { v.to_bits() };
        let key = (bits(key.x), bits(key.z));
        if let Some(&group) = group_indices.get(&key) {
            groups[group].push(i);
        } else {
            group_indices.insert(key, groups.len());
            groups.push(vec![i]);
        }
    }
    groups
}

/// Choose all destinations before mutating units. Group and claimed-spot order
/// match the supplied requests, including holds and allies' remote route ends.
pub fn end_spots(
    context: &Context<'_>,
    go: &[Request],
    face: Option<f64>,
) -> Result<Vec<Request>, Error> {
    let view = terrain(context);
    validate(context, view)?;
    validate_requests(context, go, face)?;
    // An unseen World click must not select spots using hidden physical terrain.
    // One unseen destination bypasses settling for the whole command, as in Source.
    if context.game.mode == Some(ModeKind::World) {
        for request in go {
            let owner = unit(context, request.id)?.owner;
            let team = crate::match_actor::checked_game_team(context.game, owner)
                .map_err(|_| Error::InvalidUnit(request.id))?;
            if !crate::vision::team_sees(
                context.game,
                context.rules,
                context.grid,
                team,
                request.at,
            ) {
                return Ok(go.to_vec());
            }
        }
    }
    let b = Behavior::compiled()?;
    let mut out = go.to_vec();
    let skip: BTreeSet<_> = go.iter().map(|o| o.id).collect();
    for mut group in groups(context, go, face.is_some(), false) {
        if group.len() < 2 {
            continue;
        }
        if let Some(face) = face {
            let mut rows: Vec<_> = group.iter().map(|&i| out[i]).collect();
            spread(context, &mut rows, face)?;
            for (i, row) in group.into_iter().zip(rows) {
                out[i] = row;
            }
            continue;
        }
        let n = group.len() as f64;
        let cx = group
            .iter()
            .fold(0.0, |a, &i| a + context.game.units[&out[i].id].x)
            / n;
        let cz = group
            .iter()
            .fold(0.0, |a, &i| a + context.game.units[&out[i].id].z)
            / n;
        let click = out[group[0]].at;
        let length = crate::vision::or_one(math::hypot(click.x - cx, click.z - cz));
        let dx = (click.x - cx) / length;
        let dz = (click.z - cz) / length;
        let cols = n.sqrt().ceil() as usize;
        sort::sort_by(&mut group, |a, c| {
            let a = &context.game.units[&out[a].id];
            let c = &context.game.units[&out[c].id];
            (a.x - cx) * -dz + (a.z - cz) * dx - ((c.x - cx) * -dz + (c.z - cz) * dx)
        });
        let points = in_points(context, b, click);
        let shift = if points.is_empty() {
            0.0
        } else {
            ((n / cols as f64).ceil() - 1.0) / 2.0
        };
        for (i, &index) in group.iter().enumerate() {
            let col = (i % cols) as f64 - (cols.min(group.len()) as f64 - 1.0) / 2.0;
            let row = (i / cols) as f64 - shift;
            let mut p = Point {
                x: math::min(
                    view.width as f64 * CELL - 1.0,
                    math::max(1.0, click.x - dz * col * b.gap - dx * row * b.gap),
                ),
                z: math::min(
                    view.height as f64 * CELL - 1.0,
                    math::max(1.0, click.z + dx * col * b.gap - dz * row * b.gap),
                ),
            };
            for q in &points {
                let d = distance(p, *q);
                let max = b.point_radius - 1.5;
                if d > max {
                    p.x = q.x + (p.x - q.x) / d * max;
                    p.z = q.z + (p.z - q.z) / d * max;
                }
            }
            if straight(context, unit(context, out[index].id)?, click, p) {
                out[index].at = p;
            } else {
                out[index].at = click;
            }
        }
    }
    let mut claimed = Vec::new();
    for o in &mut out {
        let u = unit(context, o.id)?;
        let mut at = o.at;
        if u.hold_pos {
            claimed.push(at);
            continue;
        }
        let mut held = claimed.clone();
        held.extend(held_spots(context, b, &skip, at, b.cover_seek, u.owner));
        let crowded = |p| held.iter().any(|h| distance(*h, p) < b.space(context, u));
        let infantry = context.rules.kinds[u.kind].infantry;
        if face.is_none() && infantry {
            let from = threat(context, b, u, at)?;
            if rank(context, at, from) == 3 || crowded(at) {
                at = near(context, view, b, u, at, b.cover_seek, &held, from, 3, None)?
                    .unwrap_or(at);
            }
        }
        if crowded(at) {
            if face.is_none() && infantry {
                let from = threat(context, b, u, at)?;
                let rank = rank(context, at, from);
                if let Some(p) = near(
                    context,
                    view,
                    b,
                    u,
                    at,
                    b.cover_seek + 2.0,
                    &held,
                    from,
                    3.min(rank + 1),
                    None,
                )? {
                    at = p;
                } else if rank == 3 {
                    at = near(
                        context,
                        view,
                        b,
                        u,
                        at,
                        b.cover_seek + 2.0,
                        &held,
                        None,
                        4,
                        None,
                    )?
                    .unwrap_or(at);
                }
            } else {
                at = near(
                    context,
                    view,
                    b,
                    u,
                    at,
                    b.cover_seek + 2.0,
                    &held,
                    None,
                    4,
                    None,
                )?
                .unwrap_or(at);
            }
        }
        claimed.push(at);
        o.at = at;
    }
    Ok(out)
}

fn block(context: &Context<'_>, u: &Unit) -> u16 {
    if context.rules.kinds[u.kind].infantry {
        MOVE | WIRE
    } else {
        MOVE | VBLOCK
    }
}
fn straight(context: &Context<'_>, u: &Unit, from: Point, to: Point) -> bool {
    crate::terrain::clear(
        context.game,
        context.rules,
        (from.x, from.z),
        (to.x, to.z),
        block(context, u),
        true,
    ) && crate::terrain::segment_cliff_clear(context.game, from, to)
}
