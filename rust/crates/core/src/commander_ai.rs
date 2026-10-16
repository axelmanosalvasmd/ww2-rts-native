//! A slot-bound quiet Conquest policy fed only by an owned observation.
//! Includes bounded research and MG purchase prefixes; combat and fortification remain separate.
pub use crate::game::Point;
use crate::{
    catalog::{Catalog, Value},
    economy::{Faction, Mode, TechState, Technology},
    game::{CELL, COVER, MOVE},
    match_commands::{Command, MoveOrder},
    math,
    random::{MatchRandom, StreamId},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Level {
    Easy,
    #[default]
    Normal,
    Hard,
}
impl Level {
    /// Lobby choices are exact names. A missing choice is handled by the seat default.
    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "easy" => Some(Self::Easy),
            "normal" => Some(Self::Normal),
            "hard" => Some(Self::Hard),
            _ => None,
        }
    }

    pub fn every(self) -> i64 {
        match self {
            Self::Easy => 120,
            Self::Normal => 40,
            Self::Hard => 20,
        }
    }
    fn hands(self) -> usize {
        match self {
            Self::Easy => 4,
            Self::Normal => 6,
            Self::Hard => 8,
        }
    }
    fn commit(self) -> f64 {
        match self {
            Self::Easy => 16.0,
            Self::Normal => 10.0,
            Self::Hard => 6.0,
        }
    }
    fn phase(self, tick: i64, slot: usize) -> i64 {
        let every = self.every();
        (tick.rem_euclid(every) + ((slot % every as usize) * 13) as i64) % every
    }
    pub fn due(self, tick: i64, slot: usize) -> bool {
        tick >= 0 && self.phase(tick, slot) == 0
    }
    pub fn wants_view(self, tick: i64, slot: usize) -> bool {
        tick >= 0 && (self.every() - self.phase(tick, slot)) % self.every() < 4
    }
}
pub fn due(tick: i64, slot: usize, level: Level) -> bool {
    level.due(tick, slot)
}
pub fn wants_view(tick: i64, slot: usize, level: Level) -> bool {
    level.wants_view(tick, slot)
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlayerObservation {
    pub team: i32,
    pub out: bool,
    pub spawn: Point,
}
#[derive(Clone, Debug, PartialEq)]
pub struct UnitObservation {
    pub id: u32,
    pub kind: String,
    pub owner: usize,
    pub at: Point,
    pub hp: f64,
    pub supp: f64,
    pub target_id: u32,
    pub cover: u8,
    pub path: Vec<Point>,
    pub attack_id: bool,
    pub retreating: bool,
    pub riding: bool,
    pub build: bool,
    pub dig: bool,
    pub entrench: bool,
    pub garrison: bool,
    pub enter: bool,
    pub fire_at: bool,
    pub nade: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub struct PointObservation {
    pub at: Point,
    pub vp: f64,
    pub mp: f64,
    pub owner: Option<usize>,
    pub cut: bool,
    pub locked: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ResearchObservation {
    pub faction: Faction,
    pub tech: TechState,
    pub busy: Vec<Technology>,
    /// Delivered own HQ and Barracks are complete, with no other own structures.
    pub stock_base: bool,
    pub armory_ready: bool,
    pub standard_army: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Input {
    pub width: usize,
    pub height: usize,
    pub tick: i64,
    pub slot: usize,
    pub players: Vec<PlayerObservation>,
    pub units: Vec<UnitObservation>,
    pub points: Vec<PointObservation>,
    pub flags: Vec<u16>,
    pub chars: Vec<u8>,
    pub manpower: f64,
    pub research: Option<ResearchObservation>,
    /// Delivered runtime facts. None retains the original quiet component envelope.
    pub runtime: Option<crate::commander_tactics::Context>,
}
#[derive(Clone, Copy, Debug, PartialEq)]
struct Kind {
    combat: bool,
    structure: bool,
    infantry: bool,
    garrisons: bool,
    health: f64,
    unsupported: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Observation {
    input: Input,
    kinds: Vec<Kind>,
    radius: f64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PolicyError {
    Dimensions,
    Tick,
    Players,
    UnitOrder,
    UnitOwner,
    UnitKind,
    UnitValue,
    Position,
    PrivateUnitFields,
    Points,
    SourceState,
    Catalog,
    NotOpening,
    EnemyContact,
    EnemyObjective,
    UnsupportedProfile,
    UnsupportedHealth,
    UnsupportedSuppression,
    UnsupportedHolder,
    UnsupportedResearch,
    UnsupportedShopping,
    RuntimeContext,
    InvalidFraction,
    ExhaustedFractions,
}
impl std::fmt::Display for PolicyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "quiet commander: {self:?}")
    }
}
impl std::error::Error for PolicyError {}
impl Observation {
    /// Validate before any policy state or purpose stream can change.
    /// Input units must already be ordered by unique, positive unit IDs.
    pub fn new(input: Input) -> Result<Self, PolicyError> {
        let cells = input
            .width
            .checked_mul(input.height)
            .ok_or(PolicyError::Dimensions)?;
        if !(20..=1024).contains(&input.width)
            || !(20..=1024).contains(&input.height)
            || input.flags.len() != cells
            || input.chars.len() != cells
            || !input.manpower.is_finite()
        {
            return Err(PolicyError::Dimensions);
        }
        if input.tick < 0 {
            return Err(PolicyError::Tick);
        }
        if !(1..=6).contains(&input.players.len())
            || input.slot >= input.players.len()
            || input.players.iter().any(|p| !(0..=5).contains(&p.team))
        {
            return Err(PolicyError::Players);
        }
        let within = |at: Point| {
            at.x.is_finite()
                && at.z.is_finite()
                && at.x >= 0.0
                && at.z >= 0.0
                && at.x <= input.width as f64 * CELL
                && at.z <= input.height as f64 * CELL
        };
        if input.players.iter().any(|p| !within(p.spawn)) {
            return Err(PolicyError::Position);
        }
        if input.points.len() > 1024
            || input.points.iter().any(|p| {
                !within(p.at)
                    || !p.vp.is_finite()
                    || !p.mp.is_finite()
                    || p.owner.is_some_and(|o| o >= input.players.len())
            })
        {
            return Err(PolicyError::Points);
        }
        if input.units.len() > 65_536
            || input.units.first().is_some_and(|u| u.id == 0)
            || input.units.windows(2).any(|w| w[0].id >= w[1].id)
        {
            return Err(PolicyError::UnitOrder);
        }
        let catalog = Catalog::compiled().map_err(|_| PolicyError::Catalog)?;
        let radius = catalog
            .table("CFG")
            .map_err(|_| PolicyError::Catalog)?
            .get("pointRadius")
            .and_then(Value::number)
            .filter(|v| v.is_finite() && *v >= 2.0)
            .ok_or(PolicyError::Catalog)?;
        if let Some(context) = &input.runtime {
            let finite_point = |p: Point| p.x.is_finite() && p.z.is_finite();
            if ![
                context.munitions,
                context.fuel,
                context.sight,
                context.caution,
                context.build_reserve,
                context.fort_reserve,
            ]
            .iter()
            .all(|v| v.is_finite())
                || context
                    .nodes
                    .iter()
                    .any(|n| !within(n.at) || !n.rate.is_finite() || n.rate < 0.0)
                || context
                    .claimed_nodes
                    .iter()
                    .any(|i| *i >= context.nodes.len())
                || context
                    .hidden_spawns
                    .iter()
                    .any(|i| *i >= input.players.len())
                || context.site_nodes.iter().any(|p| !within(*p))
                || context.caution <= 0.0
                || context.known_buildings.iter().any(|b| {
                    b.id == 0
                        || b.owner >= input.players.len()
                        || !within(b.at)
                        || !catalog
                            .unit(&b.kind)
                            .is_ok_and(|d| d.get("building").and_then(Value::boolean) == Some(true))
                })
                || context
                    .known_buildings
                    .windows(2)
                    .any(|w| w[0].id >= w[1].id)
                || context
                    .owned_nodes
                    .iter()
                    .any(|i| *i >= context.nodes.len())
                || context.known_depots.iter().any(|p| !finite_point(*p))
                || context.height.as_ref().is_some_and(|h| h.len() != cells)
                || context
                    .smokes
                    .iter()
                    .any(|s| !s.x.is_finite() || !s.z.is_finite() || !s.r.is_finite() || s.r < 0.0)
                || context.support.values().any(|v| !v.is_finite())
                || context.units.iter().any(|(id, f)| {
                    *id == 0
                        || !f.built.is_finite()
                        || f.amove.is_some_and(|p| !finite_point(p))
                        || f.stock.as_ref().is_some_and(|r| {
                            !r.provisions.is_finite()
                                || r.ammo.is_some_and(|v| !v.is_finite())
                                || r.fuel.is_some_and(|v| !v.is_finite())
                        })
                })
            {
                return Err(PolicyError::RuntimeContext);
            }
        }
        let mut kinds = Vec::with_capacity(input.units.len());
        for u in &input.units {
            if u.owner >= input.players.len() {
                return Err(PolicyError::UnitOwner);
            }
            if !within(u.at) || u.path.len() > 4096 || u.path.iter().any(|p| !within(*p)) {
                return Err(PolicyError::Position);
            }
            if !u.hp.is_finite() || !u.supp.is_finite() || u.hp < 0.0 || u.supp < 0.0 {
                return Err(PolicyError::UnitValue);
            }
            if u.owner != input.slot
                && (!u.path.is_empty() || u.attack_id || u.enter || u.fire_at || u.nade)
            {
                return Err(PolicyError::PrivateUnitFields);
            }
            let d = catalog.unit(&u.kind).map_err(|_| PolicyError::UnitKind)?;
            let is = |name| d.get(name).and_then(Value::boolean).unwrap_or(false);
            let health = d
                .get("models")
                .and_then(Value::number)
                .ok_or(PolicyError::Catalog)?
                * d.get("hpPer")
                    .and_then(Value::number)
                    .ok_or(PolicyError::Catalog)?;
            if !health.is_finite() || health <= 0.0 {
                return Err(PolicyError::Catalog);
            }
            let combat = !is("structure")
                && !is("air")
                && !is("naval")
                && !is("rail")
                && !is("logisticsTruck")
                && !is("medic")
                && u.kind != "engineer";
            kinds.push(Kind {
                combat,
                structure: is("structure"),
                infantry: is("infantry"),
                garrisons: is("garrisons"),
                health,
                unsupported: is("air") || is("naval") || is("medic"),
            });
        }
        Ok(Self {
            input,
            kinds,
            radius,
        })
    }
    pub(crate) fn unit_is_structure(&self, index: usize) -> bool {
        self.kinds[index].structure
    }
    pub(crate) fn point_radius(&self) -> f64 {
        self.radius
    }
    pub fn input(&self) -> &Input {
        &self.input
    }
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Aim {
    pub i: usize,
    pub until: f64,
}
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SourceState {
    pub aim: Option<Aim>,
    pub point_failures: Vec<u8>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Plan {
    pub commands: Vec<Command>,
    pub draws: usize,
}

/// Plan from the seat's private purpose stream. Errors commit neither state nor RNG.
pub fn plan(
    observation: &Observation,
    state: &mut SourceState,
    level: Level,
    random: &mut MatchRandom,
) -> Result<Plan, PolicyError> {
    let mut staged = random.clone();
    let mut next = state.clone();
    let result = plan_inner(observation, &mut next, level, || {
        Ok(staged.next_fraction(StreamId::Ai(observation.input.slot as u32)))
    })?;
    *state = next;
    *random = staged;
    Ok(result)
}
/// Component oracle seam: replay explicit fractions without replacing the production stream.
/// Errors leave private source state unchanged, including a short or invalid tape.
pub fn plan_with_fractions(
    observation: &Observation,
    state: &mut SourceState,
    level: Level,
    fractions: &[f64],
) -> Result<Plan, PolicyError> {
    if fractions
        .iter()
        .any(|f| !f.is_finite() || !(0.0..1.0).contains(f))
    {
        return Err(PolicyError::InvalidFraction);
    }
    let mut cursor = 0;
    let mut next = state.clone();
    let result = plan_inner(observation, &mut next, level, || {
        let value = fractions
            .get(cursor)
            .copied()
            .ok_or(PolicyError::ExhaustedFractions)?;
        cursor += 1;
        Ok(value)
    })?;
    *state = next;
    Ok(result)
}
fn distance(a: Point, b: Point) -> f64 {
    math::hypot(a.x - b.x, a.z - b.z)
}
fn move_command(orders: Vec<MoveOrder>, attack_move: bool) -> Command {
    Command::Move {
        orders,
        control_ids: None,
        attack_move,
        queue: false,
        together: false,
        face: None,
    }
}
fn plan_inner(
    observation: &Observation,
    state: &mut SourceState,
    level: Level,
    mut fraction: impl FnMut() -> Result<f64, PolicyError>,
) -> Result<Plan, PolicyError> {
    let input = &observation.input;
    let me = &input.players[input.slot];
    if state.point_failures.len() > input.points.len()
        || state
            .aim
            .is_some_and(|a| a.i >= input.points.len() || !a.until.is_finite() || a.until < 0.0)
    {
        return Err(PolicyError::SourceState);
    }
    if me.out {
        return Ok(Plan::default());
    }
    let research_commands = economic_prefix(observation)?;
    // Without neutral captures, support guns could start an unported staged assault.
    let rifle_reconnection = input.research.is_none()
        && input
            .units
            .iter()
            .all(|u| u.owner != input.slot || u.kind == "rifle")
        && input
            .points
            .iter()
            .any(|p| p.cut && p.owner.is_some_and(|o| input.players[o].team == me.team));
    if (input.tick >= 3600 && input.research.is_none())
        || !(input.points.iter().any(|p| p.owner.is_none()) || rifle_reconnection)
    {
        return Err(PolicyError::NotOpening);
    }
    if input
        .units
        .iter()
        .any(|u| u.hp > 0.0 && input.players[u.owner].team != me.team)
    {
        return Err(PolicyError::EnemyContact);
    }
    if input
        .points
        .iter()
        .any(|p| p.owner.is_some_and(|o| input.players[o].team != me.team))
    {
        return Err(PolicyError::EnemyObjective);
    }
    if input
        .units
        .iter()
        .zip(&observation.kinds)
        .any(|(u, d)| u.owner == input.slot && u.hp > 0.0 && d.unsupported)
    {
        return Err(PolicyError::UnsupportedProfile);
    }
    let mine: Vec<_> = input
        .units
        .iter()
        .zip(&observation.kinds)
        .filter(|(u, d)| u.owner == input.slot && d.combat && !u.build)
        .collect();
    for (u, d) in &mine {
        if u.hp != d.health {
            return Err(PolicyError::UnsupportedHealth);
        }
        if u.supp >= 50.0 {
            return Err(PolicyError::UnsupportedSuppression);
        }
    }
    let allied = |p: &PointObservation| p.owner.is_some_and(|o| input.players[o].team == me.team);
    let now = input.tick as f64 / 20.0;
    if state
        .aim
        .is_some_and(|a| now >= a.until || (allied(&input.points[a.i]) && !input.points[a.i].cut))
    {
        state.aim = None;
    }
    let point_of = |at| {
        input
            .points
            .iter()
            .position(|p| distance(at, p.at) <= observation.radius)
    };
    let extra = |i: usize| f64::from(state.point_failures.get(i).copied().unwrap_or(0).min(3));
    let mut load = vec![0usize; input.points.len()];
    for (u, _) in &mine {
        if let Some(i) = point_of(u.path.last().copied().unwrap_or(u.at)) {
            load[i] += 1;
        }
    }
    // The source chooses a push aim from its first scout before any unit's busy checks.
    if state.aim.is_none()
        && let Some((scout, _)) = mine.first()
    {
        let mut best = None;
        let mut score = f64::INFINITY;
        for (i, p) in input.points.iter().enumerate() {
            if (allied(p) && !p.cut) || p.locked {
                continue;
            }
            let candidate = distance(scout.at, p.at) + load[i] as f64 * 40.0
                - (p.vp - 1.0) * 25.0
                - p.mp * 10.0
                + extra(i) * 40.0;
            if candidate < score {
                score = candidate;
                best = Some(i);
            }
        }
        state.aim = best.map(|i| Aim {
            i,
            until: now + level.commit(),
        });
    }
    let mut holder_commands = Vec::new();
    let mut holding = vec![0usize; input.points.len()];
    let mut pending: Vec<Vec<u32>> = vec![Vec::new(); input.points.len()];
    for (u, d) in mine {
        if u.retreating
            || u.riding
            || !u.path.is_empty()
            || u.attack_id
            || u.target_id != 0
            || u.nade
            || u.dig
            || u.enter
            || u.fire_at
        {
            continue;
        }
        if let Some(i) = point_of(u.at)
            && d.infantry
        {
            if !allied(&input.points[i]) {
                continue;
            }
            holding[i] += 1;
            if holding[i] == 1 {
                if input.players.iter().any(|p| p.team != me.team) {
                    let p = input.points[i].at;
                    let r = observation.radius + 3.0;
                    let house = !u.garrison
                        && d.garrisons
                        && input.chars.iter().enumerate().any(|(c, ch)| {
                            if *ch != b'B' {
                                return false;
                            }
                            let x = (c % input.width) as f64;
                            let y = (c / input.width) as f64;
                            x + 1.0 > (p.x - r) / CELL
                                && x <= (p.x + r) / CELL
                                && y + 1.0 > (p.z - r) / CELL
                                && y <= (p.z + r) / CELL
                        });
                    if input.manpower != 0.0 || house {
                        return Err(PolicyError::UnsupportedHolder);
                    }
                    if !u.dig && !u.entrench && !u.garrison && !matches!(u.cover, 1 | 2) {
                        holder_commands.push(Command::Cover {
                            ids: vec![i64::from(u.id)],
                            queue: false,
                            at: None,
                        });
                    }
                }
                continue;
            }
        }
        let mut best = None;
        let mut score = f64::INFINITY;
        for (i, p) in input.points.iter().enumerate() {
            if (allied(p) && (!p.cut || distance(u.at, p.at) <= observation.radius + 16.0))
                || p.locked
            {
                continue;
            }
            let loyal = if state.aim.is_some_and(|a| a.i == i) {
                18.0
            } else {
                0.0
            };
            let candidate =
                distance(u.at, p.at) + load[i] as f64 * 40.0 - (p.vp - 1.0) * 25.0 - p.mp * 10.0
                    + f64::from(state.point_failures.get(i).copied().unwrap_or(0).min(3)) * 40.0
                    - loyal;
            if candidate < score {
                score = candidate;
                best = Some(i);
            }
        }
        if let Some(i) = best {
            if state.aim.is_none() {
                state.aim = Some(Aim {
                    i,
                    until: now + level.commit(),
                });
            }
            load[i] += 1;
            pending[i].push(u.id);
        }
    }
    let mut normal = Vec::new();
    let mut assault = Vec::new();
    let mut hands = 0;
    let mut draws = 0;
    for (i, group) in pending.into_iter().enumerate() {
        let p = &input.points[i];
        for id in group {
            if !p.cut {
                if hands >= level.hands() {
                    break;
                }
                hands += 1;
            }
            let at = spot_near(observation, p.at, fraction()?);
            draws += 1;
            let order = MoveOrder {
                unit: i64::from(id),
                at,
            };
            if p.cut {
                assault.push(order);
            } else {
                normal.push(order);
            }
        }
    }
    let mut commands = research_commands;
    commands.extend(holder_commands);
    if !normal.is_empty() {
        commands.push(move_command(normal, false));
    }
    if !assault.is_empty() {
        commands.push(move_command(assault, true));
    }
    Ok(Plan { commands, draws })
}
pub(crate) fn spot_near(observation: &Observation, at: Point, fraction: f64) -> Point {
    let input = &observation.input;
    let radius = ((observation.radius - 2.0) / CELL).floor() as isize;
    let cx = (at.x / CELL).floor() as isize;
    let cy = (at.z / CELL).floor() as isize;
    let mut cover = Vec::new();
    for y in cy - radius..=cy + radius {
        for x in cx - radius..=cx + radius {
            if x < 0 || y < 0 || x >= input.width as isize || y >= input.height as isize {
                continue;
            }
            let f = input.flags[y as usize * input.width + x as usize];
            if f & COVER != 0 && f & MOVE == 0 {
                cover.push(Point {
                    x: (x as f64 + 0.5) * CELL,
                    z: (y as f64 + 0.5) * CELL,
                });
            }
        }
    }
    if !cover.is_empty() {
        return cover[(fraction * cover.len() as f64).floor() as usize];
    }
    let angle = fraction * std::f64::consts::PI * 2.0;
    Point {
        x: at.x + math::cos(angle) * 4.0,
        z: at.z + math::sin(angle) * 4.0,
    }
}

/// A bounded later quiet frame. Prefix attempts still use the canonical dispatcher.
/// Only the exercised research and MG shopping rosters/budgets are admitted.
fn economic_prefix(observation: &Observation) -> Result<Vec<Command>, PolicyError> {
    let input = &observation.input;
    let Some(research) = &input.research else {
        return Ok(Vec::new());
    };
    let own: Vec<_> = input
        .units
        .iter()
        .zip(&observation.kinds)
        .filter(|(u, k)| u.owner == input.slot && k.combat)
        .collect();
    let count = |kind: &str| own.iter().filter(|(u, _)| u.kind == kind).count();
    let owned: Vec<_> = input
        .units
        .iter()
        .zip(&observation.kinds)
        .filter(|(u, _)| u.owner == input.slot)
        .collect();
    let lines = [Technology::Infantry, Technology::Guns, Technology::Armor];
    if research.faction == Faction::Germany {
        let before_buy = own.len() == 2 && count("rifle") == 2 && count("mg") == 0;
        let after_buy = own.len() == 3 && count("rifle") == 2 && count("mg") == 1;
        let price =
            crate::economy::price_of(Mode::Conquest, "mg").map_err(|_| PolicyError::Catalog)?;
        if !research.standard_army
            || !research.stock_base
            || research.armory_ready
            || research.tech.tier() != 3
            || !research.busy.is_empty()
            || lines.iter().any(|&k| research.tech.level(k) != 3)
            || owned.len() != own.len() + 2
            || owned.iter().any(|(u, k)| {
                u.hp != k.health || !["rifle", "mg", "hq", "barracks"].contains(&u.kind.as_str())
            })
            || owned.iter().filter(|(u, _)| u.kind == "hq").count() != 1
            || owned.iter().filter(|(u, _)| u.kind == "barracks").count() != 1
            || input
                .points
                .iter()
                .any(|p| p.owner.is_some() || p.cut || p.locked)
            || !(before_buy && input.manpower == price.mp || after_buy && input.manpower == 0.0)
        {
            return Err(PolicyError::UnsupportedShopping);
        }
        return Ok(if before_buy {
            vec![Command::Buy {
                kind: "mg".into(),
                from: None,
            }]
        } else {
            vec![]
        });
    }
    if owned.len() != 5
        || owned.iter().any(|(u, k)| {
            u.hp != k.health || !["rifle", "mg", "hq", "barracks"].contains(&u.kind.as_str())
        })
        || owned.iter().filter(|(u, _)| u.kind == "hq").count() != 1
        || owned.iter().filter(|(u, _)| u.kind == "barracks").count() != 1
        || research.faction != Faction::Usa
        || !research.stock_base
        || research.armory_ready
        || own.len() != 3
        || count("rifle") != 2
        || count("mg") != 1
        || lines.iter().any(|&k| research.tech.level(k) != 0)
        || research.busy.len() > 4
        || research
            .busy
            .iter()
            .enumerate()
            .any(|(i, k)| research.busy[..i].contains(k))
        || input
            .points
            .iter()
            .any(|p| p.owner.is_some() || p.cut || p.locked)
    {
        return Err(PolicyError::UnsupportedResearch);
    }
    let busy = |kind| research.busy.contains(&kind);
    let tier = research.tech.tier();
    let tier_due = match tier {
        1 => input.tick >= 2400,
        2 => input.tick >= 7800,
        _ => false,
    };
    let kind = if tier_due && !busy(Technology::Tier) {
        Some(Technology::Tier)
    } else {
        lines
            .into_iter()
            .filter(|&k| !busy(k) && research.tech.level(k) < tier)
            .min_by_key(|&k| research.tech.level(k))
    };
    let supported_budget = input.manpower == 0.0
        || (tier == 1 && kind == Some(Technology::Tier) && input.manpower == 250.0)
        || (tier == 2 && kind == Some(Technology::Tier) && input.manpower == 400.0)
        || (tier == 3 && kind == Some(Technology::Infantry) && input.manpower == 100.0);
    if !supported_budget {
        return Err(PolicyError::UnsupportedResearch);
    }
    let Some(kind) = kind else {
        return Ok(Vec::new());
    };
    let cost = crate::economy::tech_cost(research.tech, kind, Mode::Conquest)
        .map_err(|_| PolicyError::Catalog)?;
    Ok(cost
        .filter(|c| input.manpower >= c.mp)
        .map(|_| Command::Technology { kind })
        .into_iter()
        .collect())
}
