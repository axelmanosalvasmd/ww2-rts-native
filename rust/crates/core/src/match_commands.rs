//! Canonical control intent and tick-boundary commands for native Conquest.

use crate::{
    catalog::Catalog,
    economy::{self, AbilityCost, Mode},
    game::{Point, Unit},
    grid::UnitId,
    orders::OrderQueue,
    path_requests::PathRequests,
    pathfinding::Pathfinding,
    rules::Rules,
};
use std::collections::{BTreeMap, VecDeque};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Facing {
    pub angle: f64,
    pub at: Point,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoveOutcome {
    Interrupted,
    Moving,
    Unreachable,
    Arrived,
}
impl MoveOutcome {
    pub fn name(self) -> &'static str {
        match self {
            Self::Interrupted => "interrupted",
            Self::Moving => "moving",
            Self::Unreachable => "unreachable",
            Self::Arrived => "completed",
        }
    }
}
/// The optional source result tail survives later moving/interrupted intents.
/// Only the actual terminal path/completion operations replace it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoveResult {
    Unreachable { tick: i64 },
    Completed { tick: i64 },
}
impl MoveResult {
    pub fn parts(self) -> (&'static str, i64) {
        match self {
            Self::Unreachable { tick } => ("unreachable", tick),
            Self::Completed { tick } => ("completed", tick),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MoveOrder {
    pub unit: i64,
    pub at: Point,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stance {
    HoldFire,
    HoldPosition,
    AutoRetreat,
}

/// Internal recorded input. This is not a WebSocket or C interface.
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    /// Bounded internal compound order, with queue inherited by every part.
    Group {
        commands: Vec<serde_json::Value>,
        queue: bool,
    },
    FireAt {
        ids: Vec<i64>,
        at: Point,
        queue: bool,
    },
    Build {
        ids: Vec<i64>,
        kind: String,
        at: Option<Point>,
        direction: Option<f64>,
        queue: bool,
    },
    Assist {
        ids: Vec<i64>,
        target: i64,
        at: Option<Point>,
        queue: bool,
    },
    Destroy {
        ids: Vec<i64>,
        id: Option<i64>,
    },
    Board {
        ids: Vec<i64>,
        target: Option<i64>,
    },
    Unload {
        ids: Vec<i64>,
    },
    Entrench {
        ids: Vec<i64>,
        order: crate::field_works::ProjectOrder,
        queue: bool,
    },
    Dig {
        ids: Vec<i64>,
        kind: crate::field_works::FortKind,
        at: Point,
        direction: Option<f64>,
        queue: bool,
    },
    Technology {
        kind: crate::economy::Technology,
    },
    Buy {
        kind: String,
        from: Option<u32>,
    },
    /// Trusted room binding. Raw gameplay JSON cannot select a journal identity.
    CompanionJoin {
        identity: crate::recorded_input::CompanionJournalId,
    },
    CompanionInput {
        identity: crate::recorded_input::CompanionJournalId,
        input: serde_json::Value,
    },
    Support {
        kind: String,
        at: Point,
        direction: Option<f64>,
    },
    Move {
        orders: Vec<MoveOrder>,
        control_ids: Option<Vec<i64>>,
        attack_move: bool,
        queue: bool,
        together: bool,
        face: Option<f64>,
    },
    Cover {
        ids: Vec<i64>,
        queue: bool,
        /// Display anchor for a waiting order, never its eventual cover destination.
        at: Option<Point>,
    },
    Stop {
        ids: Vec<i64>,
    },
    Attack {
        ids: Vec<i64>,
        target: i64,
        queue: bool,
        /// Waiting display anchor, never the current attack destination.
        at: Option<Point>,
    },
    Escort {
        ids: Vec<i64>,
        target: i64,
    },
    Retreat {
        ids: Vec<i64>,
    },
    Stance {
        ids: Vec<i64>,
        stance: Stance,
        on: bool,
    },
    Ability {
        ids: Vec<i64>,
        at: Option<Point>,
        queue: bool,
        automatic: bool,
    },
    Autocast {
        ids: Vec<i64>,
        on: bool,
    },
    Unsupported,
}

/// Only fields absent from Unit live here. Routes, positions and hold_pos remain in Unit.
#[derive(Clone, Debug, PartialEq)]
pub struct UnitControl {
    /// Seconds until another idle reaction may choose a destination (`u.react`).
    pub react: f64,
    pub aim: f64,
    pub auto_cast: bool,
    pub auto_retreat: bool,
    pub attack_id: UnitId,
    pub follow: UnitId,
    pub hold_fire: bool,
    pub face: Option<Facing>,
    pub orders: VecDeque<Command>,
    pub outcome: Option<MoveOutcome>,
    pub outcome_tick: Option<i64>,
    pub move_result: Option<MoveResult>,
    pub route_end: Option<Point>,
    pub supply_started: bool,
    pub supply_retry: Option<u64>,
    pub supply_hold: bool,
    pub supply_escape: bool,
    pub enter: Option<usize>,
    /// Physical entry used for supply handoff while inside a house.
    pub garrison_entry: Option<Point>,
    pub fire_at: Option<usize>,
    pub nade: Option<Point>,
    /// Recruitment marks a rally walk so queued orders do not interrupt it.
    pub rally_drift: bool,
    /// Operatives reject ordinary squad orders.
    pub operative: bool,
    /// World local guards return here in the guard phase, owned by tick scheduling.
    pub guard_home: Option<Point>,
    /// Source `u.build` site identity. Unit.build is its synchronized activity projection.
    pub build_target: Option<UnitId>,
}
impl UnitControl {
    /// Prepare before publishing a spawned unit or charging a wallet.
    pub fn new(unit: &Unit, rules: &Rules) -> Result<Self, String> {
        Self::new_for_mode(unit, rules, Mode::Default)
    }
    /// Paid construction-mode abilities start off; free abilities start on.
    pub fn new_for_mode(unit: &Unit, rules: &Rules, mode: Mode) -> Result<Self, String> {
        let kind = rules
            .kinds
            .get(unit.kind)
            .ok_or("unknown control unit kind")?;
        Self::new_for_kind(kind, unit.rot, mode)
    }
    /// The same canonical defaults admit prospective births before placement.
    pub(crate) fn new_for_kind(
        kind: &crate::rules::UnitKind,
        aim: f64,
        mode: Mode,
    ) -> Result<Self, String> {
        let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
        let definition = catalog.unit(&kind.name).map_err(|e| e.to_string())?;
        let ability = definition.get("ab").ok_or("missing unit ability")?;
        let ability_id = ability
            .get("id")
            .and_then(|v| v.string())
            .ok_or("missing unit ability")?;
        let cost = economy::ab_cost(
            mode,
            AbilityCost {
                id: ability_id,
                munitions: ability.get("mun").and_then(|v| v.number()),
            },
        )
        .map_err(|e| e.to_string())?;
        if !aim.is_finite() {
            return Err("nonfinite unit aim".into());
        }
        Ok(Self {
            react: 0.0,
            aim,
            auto_cast: ability_id != "none" && cost == 0.0,
            auto_retreat: !kind.air,
            attack_id: 0,
            follow: 0,
            hold_fire: false,
            face: None,
            orders: VecDeque::new(),
            outcome: None,
            outcome_tick: None,
            move_result: None,
            route_end: None,
            supply_started: false,
            supply_retry: None,
            supply_hold: false,
            supply_escape: false,
            enter: None,
            garrison_entry: None,
            fire_at: None,
            nade: None,
            rally_drift: false,
            operative: false,
            guard_home: None,
            build_target: None,
        })
    }
}

pub struct MatchCommands {
    pub units: BTreeMap<UnitId, UnitControl>,
    pub(crate) queue: OrderQueue<Command>,
    pub(crate) paths: Pathfinding,
    pub(crate) requests: PathRequests,
    path_tick: Option<i64>,
}
impl MatchCommands {
    pub fn new(capacity: usize) -> Self {
        let mut queue = OrderQueue::new(capacity);
        // Match startup is tick zero; commands first execute at tick one.
        queue.advance().expect("initial empty tick");
        Self {
            units: BTreeMap::new(),
            queue,
            paths: Pathfinding::new(),
            requests: PathRequests::new(),
            path_tick: None,
        }
    }
    /// Publish an already prepared row after the caller commits its spawn transaction.
    pub fn insert(&mut self, id: UnitId, row: UnitControl) {
        self.units.insert(id, row);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Denial {
    Blocked,
    Needs,
    QueueFull,
    NoCover,
    Retreating,
    Unsupported,
    Ammo,
    Cooldown,
    Munitions,
    Manpower,
    Tier,
    Fuel,
    Population,
    Maximum,
    SuddenDeath,
    Unseen,
    NotWaiting,
    NoBuilders,
    NotVisible,
    Coast,
}
impl Denial {
    pub fn name(self) -> &'static str {
        match self {
            Self::Blocked => "blocked",
            Self::Needs => "needs",
            Self::QueueFull => "queueFull",
            Self::NoCover => "noCover",
            Self::Retreating => "retreating",
            Self::Unsupported => "unsupported",
            Self::Ammo => "ammo",
            Self::Cooldown => "cooldown",
            Self::Munitions => "mun",
            Self::Manpower => "mp",
            Self::Tier => "tier",
            Self::Fuel => "fuel",
            Self::Population => "pop",
            Self::Maximum => "max",
            Self::SuddenDeath => "suddenDeath",
            Self::Unseen => "unseen",
            Self::NotWaiting => "notWaiting",
            Self::NoBuilders => "noBuilders",
            Self::NotVisible => "notVisible",
            Self::Coast => "coast",
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct CommandResult {
    pub key: crate::orders::OrderKey,
    pub denial: Option<Denial>,
}

impl From<crate::production::Denial> for Denial {
    fn from(value: crate::production::Denial) -> Self {
        use crate::production::Denial as P;
        match value {
            P::Blocked => Self::Blocked,
            P::Needs => Self::Needs,
            P::Tier => Self::Tier,
            P::Mp => Self::Manpower,
            P::Mun => Self::Munitions,
            P::Fuel => Self::Fuel,
            P::Pop => Self::Population,
            P::Max => Self::Maximum,
            P::QueueFull => Self::QueueFull,
            P::SuddenDeath => Self::SuddenDeath,
            P::Unseen => Self::Unseen,
            P::NotWaiting => Self::NotWaiting,
        }
    }
}

impl Command {
    /// Validate every row before unit selection or the source's row cap.
    pub fn from_json(value: &serde_json::Value) -> Result<Self, Denial> {
        let object = value.as_object().ok_or(Denial::Blocked)?;
        let safe_id = |value: &serde_json::Value| {
            value
                .as_f64()
                .filter(|n| n.is_finite() && n.fract() == 0.0 && n.abs() <= 9007199254740991.0)
                .map(|n| n as i64)
        };
        let mut orders = Vec::new();
        if let Some(raw) = object.get("orders") {
            for row in raw.as_array().ok_or(Denial::Blocked)? {
                let row = row
                    .as_array()
                    .filter(|r| r.len() >= 3)
                    .ok_or(Denial::Blocked)?;
                orders.push(MoveOrder {
                    unit: safe_id(&row[0]).ok_or(Denial::Blocked)?,
                    at: Point {
                        x: row[1]
                            .as_f64()
                            .filter(|n| n.is_finite())
                            .ok_or(Denial::Blocked)?,
                        z: row[2]
                            .as_f64()
                            .filter(|n| n.is_finite())
                            .ok_or(Denial::Blocked)?,
                    },
                });
            }
        }
        let ids = object
            .get("ids")
            .and_then(|v| v.as_array())
            .map_or_else(Vec::new, |rows| {
                rows.iter().map(|v| safe_id(v).unwrap_or(-1)).collect()
            });
        match object.get("t").and_then(|v| v.as_str()) {
            Some("build" | "assist") => {
                let at = object
                    .get("x")
                    .and_then(|v| v.as_f64())
                    .filter(|v| v.is_finite())
                    .zip(
                        object
                            .get("z")
                            .and_then(|v| v.as_f64())
                            .filter(|v| v.is_finite()),
                    )
                    .map(|(x, z)| Point { x, z });
                let queue = object.get("queue").and_then(|v| v.as_bool()) == Some(true);
                if object["t"] == "build" {
                    Ok(Self::Build {
                        ids,
                        kind: object
                            .get("kind")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .into(),
                        at,
                        direction: object
                            .get("dir")
                            .and_then(|v| v.as_f64())
                            .filter(|v| v.is_finite()),
                        queue,
                    })
                } else {
                    Ok(Self::Assist {
                        ids,
                        target: object.get("id").and_then(safe_id).unwrap_or(-1),
                        at,
                        queue,
                    })
                }
            }

            Some("destroy") => Ok(Self::Destroy {
                ids,
                id: object.get("id").and_then(safe_id),
            }),
            Some("board") => Ok(Self::Board {
                ids,
                target: object
                    .contains_key("target")
                    .then(|| object.get("target").and_then(safe_id).unwrap_or(-1)),
            }),
            Some("unload") => Ok(Self::Unload { ids }),
            Some("entrench") => {
                use crate::field_works::{Pattern, ProjectMaterial, ProjectOrder};
                let number = |key: &str| {
                    object
                        .get(key)
                        .and_then(|v| v.as_f64())
                        .filter(|v| v.is_finite())
                };
                let at = number("x").zip(number("z")).map(|(x, z)| Point { x, z });
                let order = if object.contains_key("join") {
                    ProjectOrder::Join {
                        project: object
                            .get("join")
                            .and_then(safe_id)
                            .and_then(|id| u64::try_from(id).ok())
                            .unwrap_or(0),
                        at,
                    }
                } else {
                    ProjectOrder::New {
                        pattern: object
                            .get("pattern")
                            .and_then(|v| v.as_str())
                            .and_then(Pattern::parse),
                        material: ProjectMaterial::parse(
                            object.get("fort").and_then(|v| v.as_str()).unwrap_or(
                                if object.get("fort").is_none_or(|v| v.is_null()) {
                                    "trench"
                                } else {
                                    ""
                                },
                            ),
                        ),
                        from: at,
                        to: at.map(|p| Point {
                            x: number("x2").unwrap_or(p.x),
                            z: number("z2").unwrap_or(p.z),
                        }),
                    }
                };
                Ok(Self::Entrench {
                    ids,
                    order,
                    queue: object.get("queue").and_then(|v| v.as_bool()) == Some(true),
                })
            }
            Some("dig") => {
                let at = Point {
                    x: object
                        .get("x")
                        .and_then(|v| v.as_f64())
                        .filter(|n| n.is_finite())
                        .ok_or(Denial::Blocked)?,
                    z: object
                        .get("z")
                        .and_then(|v| v.as_f64())
                        .filter(|n| n.is_finite())
                        .ok_or(Denial::Blocked)?,
                };
                let name = match object.get("kind") {
                    None | Some(serde_json::Value::Null) => "trench",
                    Some(value) => value.as_str().ok_or(Denial::Blocked)?,
                };
                let kind = match crate::field_works::FortKind::parse(name) {
                    Ok(kind) => kind,
                    Err(crate::field_works::Error::UnsupportedKind) => {
                        return Ok(Self::Unsupported);
                    }
                    Err(_) => return Err(Denial::Blocked),
                };
                Ok(Self::Dig {
                    ids,
                    kind,
                    at,
                    direction: object
                        .get("dir")
                        .and_then(|v| v.as_f64())
                        .filter(|n| n.is_finite()),
                    queue: object.get("queue").and_then(|v| v.as_bool()) == Some(true),
                })
            }
            Some("tech") => Ok(Self::Technology {
                kind: match object.get("kind").and_then(|v| v.as_str()) {
                    Some("tier") => crate::economy::Technology::Tier,
                    Some("inf") => crate::economy::Technology::Infantry,
                    Some("guns") => crate::economy::Technology::Guns,
                    Some("armor") => crate::economy::Technology::Armor,
                    _ => return Err(Denial::Blocked),
                },
            }),
            Some("buy") => Ok(Self::Buy {
                kind: object
                    .get("unit")
                    .and_then(|v| v.as_str())
                    .ok_or(Denial::Blocked)?
                    .into(),
                // Present invalid maker values cannot select the default maker.
                // No canonical unit has ID zero, so source admission returns Needs
                // at maker selection after checking resources and population.
                from: object.get("from").map(|value| {
                    value
                        .as_f64()
                        .filter(|n| {
                            n.is_finite() && n.fract() == 0.0 && *n > 0.0 && *n <= u32::MAX as f64
                        })
                        .map_or(0, |n| n as u32)
                }),
            }),
            Some("support") => Ok(Self::Support {
                kind: object
                    .get("kind")
                    .and_then(|v| v.as_str())
                    .ok_or(Denial::Blocked)?
                    .into(),
                at: Point {
                    x: object
                        .get("x")
                        .and_then(|v| v.as_f64())
                        .filter(|v| v.is_finite())
                        .ok_or(Denial::Blocked)?,
                    z: object
                        .get("z")
                        .and_then(|v| v.as_f64())
                        .filter(|v| v.is_finite())
                        .ok_or(Denial::Blocked)?,
                },
                direction: object
                    .get("dir")
                    .and_then(|v| v.as_f64())
                    .filter(|v| v.is_finite()),
            }),
            Some("move" | "amove") if object.contains_key("orders") => {
                let face = object
                    .get("face")
                    .map(|v| {
                        v.as_f64()
                            .and_then(crate::formation::normalize_face)
                            .ok_or(Denial::Blocked)
                    })
                    .transpose()?;
                Ok(Self::Move {
                    orders,
                    control_ids: object.get("ids").and_then(|v| v.as_array()).map(|_| ids),
                    attack_move: object["t"] == "amove",
                    queue: object.get("queue").and_then(|v| v.as_bool()) == Some(true),
                    together: object.get("together").and_then(|v| v.as_bool()) == Some(true),
                    face,
                })
            }
            Some("orders") => Ok(Self::Group {
                commands: object
                    .get("commands")
                    .and_then(|v| v.as_array())
                    .ok_or(Denial::Blocked)?
                    .iter()
                    .take(8)
                    .filter(|v| {
                        matches!(
                            v["t"].as_str(),
                            Some(
                                "move"
                                    | "amove"
                                    | "attack"
                                    | "garrison"
                                    | "fireat"
                                    | "escort"
                                    | "assist"
                                    | "rally"
                                    | "entrench"
                                    | "board"
                            )
                        )
                    })
                    .cloned()
                    .collect(),
                queue: object.get("queue").and_then(|v| v.as_bool()) == Some(true),
            }),
            Some("fireat") => Ok(Self::FireAt {
                ids,
                at: Point {
                    x: object
                        .get("x")
                        .and_then(|v| v.as_f64())
                        .filter(|v| v.is_finite())
                        .ok_or(Denial::Blocked)?,
                    z: object
                        .get("z")
                        .and_then(|v| v.as_f64())
                        .filter(|v| v.is_finite())
                        .ok_or(Denial::Blocked)?,
                },
                queue: object.get("queue").and_then(|v| v.as_bool()) == Some(true),
            }),
            Some("cover") => Ok(Self::Cover {
                ids,
                queue: object.get("queue").and_then(|v| v.as_bool()) == Some(true),
                at: None,
            }),
            Some("ability") => {
                let at = object
                    .get("x")
                    .and_then(|x| x.as_f64())
                    .filter(|x| x.is_finite())
                    .zip(
                        object
                            .get("z")
                            .and_then(|z| z.as_f64())
                            .filter(|z| z.is_finite()),
                    )
                    .map(|(x, z)| Point { x, z });
                Ok(Self::Ability {
                    ids,
                    at,
                    queue: object.get("queue").and_then(|v| v.as_bool()) == Some(true),
                    automatic: false,
                })
            }
            Some("autocast") => Ok(Self::Autocast {
                ids,
                on: object
                    .get("on")
                    .and_then(|v| v.as_bool())
                    .ok_or(Denial::Blocked)?,
            }),
            Some("stop") => Ok(Self::Stop { ids }),
            Some("attack") => Ok(Self::Attack {
                ids,
                target: object.get("target").and_then(safe_id).unwrap_or(-1),
                queue: object.get("queue").and_then(|v| v.as_bool()) == Some(true),
                at: None,
            }),
            Some("escort") => Ok(Self::Escort {
                ids,
                target: object.get("target").and_then(safe_id).unwrap_or(-1),
            }),
            Some("retreat") => Ok(Self::Retreat { ids }),
            Some("stance") => {
                let stance = match object.get("key").and_then(|v| v.as_str()) {
                    Some("holdFire") => Stance::HoldFire,
                    Some("holdPos") => Stance::HoldPosition,
                    Some("autoRetreat") => Stance::AutoRetreat,
                    _ => return Err(Denial::Blocked),
                };
                Ok(Self::Stance {
                    ids,
                    stance,
                    on: object
                        .get("on")
                        .and_then(|v| v.as_bool())
                        .ok_or(Denial::Blocked)?,
                })
            }
            _ => Ok(Self::Unsupported),
        }
    }
    fn valid(&self) -> bool {
        match self {
            Self::Group { commands, .. } => {
                commands.len() <= 8
                    && commands.iter().all(|v| {
                        v.is_object()
                            && matches!(
                                v["t"].as_str(),
                                Some(
                                    "move"
                                        | "amove"
                                        | "attack"
                                        | "garrison"
                                        | "fireat"
                                        | "escort"
                                        | "assist"
                                        | "rally"
                                        | "entrench"
                                        | "board"
                                )
                            )
                    })
            }
            Self::FireAt { at, .. } => at.x.is_finite() && at.z.is_finite(),
            Self::Attack { at, .. } => at.is_none_or(|p| p.x.is_finite() && p.z.is_finite()),
            Self::Dig {
                ids, at, direction, ..
            } => {
                ids.iter().all(|id| id.unsigned_abs() <= 9007199254740991)
                    && at.x.is_finite()
                    && at.z.is_finite()
                    && direction.is_none_or(|v| v.is_finite())
            }
            Self::Support { at, direction, .. } => {
                at.x.is_finite() && at.z.is_finite() && direction.is_none_or(|v| v.is_finite())
            }
            Self::Move { orders, face, .. } => {
                orders.iter().all(|o| {
                    o.unit.unsigned_abs() <= 9007199254740991
                        && o.at.x.is_finite()
                        && o.at.z.is_finite()
                }) && face.is_none_or(|a| a.is_finite())
            }
            Self::Cover { at, .. } => at.is_none_or(|p| p.x.is_finite() && p.z.is_finite()),
            _ => true,
        }
    }
}

impl crate::match_runtime::Match {
    pub fn enqueue_command(
        &mut self,
        key: crate::orders::OrderKey,
        command: Command,
    ) -> Result<(), crate::orders::QueueError> {
        self.commands.queue.enqueue(key, command)
    }
    pub fn validate_command_tick(&self, tick: u64) -> Result<(), String> {
        if self.commands.queue.next_tick() != Some(tick) {
            return Err("command tick is not the next queued tick".into());
        }
        Ok(())
    }
    /// Apply recorded boundary input before the Match runner advances its clock.
    pub fn apply_commands(
        &mut self,
        tick: u64,
        winner_decided: bool,
    ) -> Result<Vec<CommandResult>, String> {
        self.apply_commands_observing(tick, winner_decided, &mut |_, _| Ok(()))
    }
    /// Observe each real dispatch before its result leaves the sole input boundary.
    pub(crate) fn apply_commands_observing(
        &mut self,
        tick: u64,
        winner_decided: bool,
        observer: &mut impl FnMut(&mut Self, &Command) -> Result<(), String>,
    ) -> Result<Vec<CommandResult>, String> {
        self.validate_command_tick(tick)?;
        if self.world.game.tick < 0 || (self.world.game.tick as u64).checked_add(1) != Some(tick) {
            return Err("command tick differs from Match clock".into());
        }
        let batch = self
            .commands
            .queue
            .advance()
            .map_err(|e| format!("command queue: {e:?}"))?;
        let mut results = Vec::with_capacity(batch.orders.len());
        for order in batch.orders {
            let result =
                self.execute_command(order.key.issuer as usize, &order.value, winner_decided);
            observer(self, &order.value)?;
            let denial = result?;
            results.push(CommandResult {
                key: order.key,
                denial,
            });
        }
        Ok(results)
    }
    pub(crate) fn eligible_command_unit(
        &self,
        id: i64,
        owner: usize,
        command: &Command,
    ) -> Option<UnitId> {
        let id = u32::try_from(id).ok()?;
        let u = self.world.game.units.get(&id)?;
        let kind = self.rules.kinds.get(u.kind)?;
        let control = self.commands.units.get(&id)?;
        let forced = self.logistics.get(id).is_some_and(|r| r.forced);
        let allowed_forced =
            matches!(
                command,
                Command::Move {
                    attack_move: false,
                    ..
                } | Command::Stop { .. }
                    | Command::Retreat { .. }
            ) || matches!(command, Command::Attack { .. } | Command::Ability { .. })
                && self.logistics.get(id).is_some_and(|r| r.stranded);
        (u.owner == owner
            && !kind.structure
            && !kind.rail
            && !u.rail
            && !u.riding
            && !control.operative
            && (!forced || allowed_forced))
            .then_some(id)
    }
    pub fn execute_command(
        &mut self,
        owner: usize,
        command: &Command,
        winner_decided: bool,
    ) -> Result<Option<Denial>, String> {
        let winner_decided = winner_decided || self.finish.is_some();
        // FPS inputs have source gates of their own, including away commanders.
        // Resolve the safe journal key through the one canonical registry. Its
        // internal CompanionId is allocated at dispatch and need not match it.
        match command {
            Command::CompanionJoin { identity } => {
                let key = identity.registry_key();
                if self
                    .companions
                    .get(&key)
                    .is_some_and(|row| row.owner != owner)
                {
                    return Ok(None);
                }
                self.join_companion(&key, owner, winner_decided)?;
                return Ok(None);
            }
            Command::CompanionInput { identity, input } => {
                let key = identity.registry_key();
                if self
                    .companions
                    .get(&key)
                    .is_some_and(|row| row.owner == owner)
                {
                    self.companion_input(&key, input, winner_decided);
                }
                return Ok(None);
            }
            _ => {}
        }
        let Some(player) = self.start.economy.players.get(owner) else {
            return Ok(Some(Denial::Blocked));
        };
        if winner_decided || player.out || player.away || !command.valid() {
            return Ok(Some(Denial::Blocked));
        }
        if matches!(command, Command::Unsupported) {
            return Ok(Some(Denial::Unsupported));
        }
        if let Command::Group { commands, queue } = command {
            let mut done = false;
            let mut reason = None;
            for part in commands.iter().take(8) {
                let mut part = part.clone();
                part["queue"] = serde_json::Value::Bool(*queue);
                let denial = match Command::from_json(&part) {
                    Ok(Command::Unsupported) => Some(Denial::Blocked),
                    Ok(part) => self.execute_command(owner, &part, winner_decided)?,
                    Err(denial) => Some(denial),
                };
                if let Some(denial) = denial {
                    reason = reason.or(Some(denial));
                } else {
                    done = true;
                }
            }
            return Ok(if done {
                None
            } else {
                Some(reason.unwrap_or(Denial::Blocked))
            });
        }
        self.validate_building_activity_rows()?;
        if let Command::FireAt { ids, at, queue } = command {
            return self.ground_fire_at_command(owner, ids, *at, *queue);
        }
        if let Command::Build {
            ids,
            kind,
            at,
            direction,
            queue,
        } = command
        {
            return self.execute_build(owner, ids, kind, *at, *direction, *queue);
        }
        if let Command::Assist {
            ids, target, queue, ..
        } = command
        {
            return self.execute_assist(owner, ids, *target, *queue);
        }
        if let Command::Attack {
            ids, target, queue, ..
        } = command
            && !ids
                .iter()
                .take(50.max(self.world.game.units.len()))
                .any(|raw| {
                    self.eligible_command_unit(*raw, owner, command)
                        .is_some_and(|id| self.is_aircraft_command_unit(id))
                })
        {
            return self.ground_attack_command(owner, ids, *target, *queue);
        }
        if let Command::Retreat { ids } = command
            && !ids
                .iter()
                .take(50.max(self.world.game.units.len()))
                .any(|raw| {
                    self.eligible_command_unit(*raw, owner, command)
                        .is_some_and(|id| self.is_aircraft_command_unit(id))
                })
        {
            return self.ground_retreat_command(owner, ids);
        }
        if let Command::Destroy { ids, id } = command {
            return self.execute_destroy(owner, ids, *id);
        }
        if let Command::Board { ids, target } = command {
            return self.carrier_board_command(owner, ids, *target, winner_decided);
        }
        if let Command::Unload { ids } = command {
            return self.carrier_unload_command(owner, ids, winner_decided);
        }
        if let Command::Entrench { ids, order, queue } = command {
            return self.execute_entrench(owner, ids, *order, *queue, winner_decided);
        }
        if let Command::Cover { ids, queue, .. } = command {
            return self.execute_cover_command(owner, ids, *queue);
        }
        if let Command::Dig {
            ids,
            kind,
            at,
            direction,
            queue,
        } = command
        {
            if *queue {
                return Ok(Some(Denial::Unsupported));
            }
            return self.execute_dig(owner, ids, *kind, *at, *direction, winner_decided);
        }
        if let Command::Technology { kind } = command {
            return match self.request_research(owner, *kind) {
                Ok(_) => Ok(None),
                Err(crate::match_runtime::ProductionStepError::Production(
                    crate::production::Error::Denied(d),
                )) => Ok(Some(d.into())),
                Err(e) => Err(format!("recorded research: {e:?}")),
            };
        }
        if let Command::Buy { kind, from } = command {
            if kind == "worldbase" {
                return Ok(Some(Denial::Blocked));
            }
            if self.options.mode == Mode::World {
                if Catalog::compiled()
                    .and_then(|catalog| catalog.table("UNITS"))
                    .map_err(|e| e.to_string())?
                    .get(kind)
                    .is_none()
                {
                    return Ok(Some(Denial::Blocked));
                }
                return match self.recorded_world_buy(owner, kind, *from) {
                    Ok(_) => Ok(None),
                    Err(crate::match_runtime::ProductionStepError::Production(
                        crate::production::Error::Denied(d),
                    )) => Ok(Some(d.into())),
                    Err(e) => Err(format!("recorded World buy: {e:?}")),
                };
            }
            return match self.recorded_buy(owner, kind, *from) {
                Ok(_) => Ok(None),
                Err(crate::match_runtime::RecruitError::Admission(
                    crate::production::Error::Denied(d),
                )) => Ok(Some(d.into())),
                Err(crate::match_runtime::RecruitError::Placement(
                    crate::produced_units::Error::UnsupportedAir,
                )) => Ok(Some(Denial::Unsupported)),
                Err(e) => Err(format!("recorded buy: {e:?}")),
            };
        }
        if let Command::Support {
            kind,
            at,
            direction,
        } = command
        {
            return match self.admit_support(owner, kind, *at, *direction) {
                Ok(denial) => Ok(denial.map(|d| match d {
                    crate::match_support::SupportDenial::Blocked => Denial::Blocked,
                    crate::match_support::SupportDenial::Cooldown => Denial::Cooldown,
                    crate::match_support::SupportDenial::Manpower => Denial::Manpower,
                    crate::match_support::SupportDenial::Unseen => Denial::Unseen,
                    crate::match_support::SupportDenial::Population => Denial::Population,
                })),
                Err(crate::match_support::SupportError::Unsupported(_)) => {
                    Ok(Some(Denial::Unsupported))
                }
                Err(e) => Err(e.to_string()),
            };
        }
        if matches!(command, Command::Ability { .. } | Command::Autocast { .. }) {
            self.validate_abilities()?;
        }
        let limit = 50.max(self.world.game.units.len());
        let controlled: Vec<_> = match command {
            Command::Move {
                orders,
                control_ids,
                ..
            } => control_ids
                .clone()
                .unwrap_or_else(|| orders.iter().map(|o| o.unit).collect()),
            Command::Stop { ids }
            | Command::Stance { ids, .. }
            | Command::Ability { ids, .. }
            | Command::Autocast { ids, .. }
            | Command::Attack { ids, .. }
            | Command::Escort { ids, .. }
            | Command::Retreat { ids } => ids.clone(),
            Command::Group { .. }
            | Command::FireAt { .. }
            | Command::Build { .. }
            | Command::Assist { .. }
            | Command::Destroy { .. }
            | Command::Board { .. }
            | Command::Unload { .. }
            | Command::Entrench { .. }
            | Command::Dig { .. }
            | Command::Technology { .. }
            | Command::Buy { .. }
            | Command::CompanionJoin { .. }
            | Command::CompanionInput { .. }
            | Command::Support { .. }
            | Command::Cover { .. }
            | Command::Unsupported => unreachable!(),
        };
        let selected_ids: Vec<_> = match command {
            Command::Move { orders, .. } => orders.iter().map(|o| o.unit).collect(),
            _ => controlled.clone(),
        };
        let selected: Vec<_> = selected_ids
            .iter()
            .take(limit)
            .filter_map(|&id| self.eligible_command_unit(id, owner, command))
            .collect();
        if matches!(command, Command::Ability { .. } | Command::Autocast { .. })
            && selected.iter().any(|id| self.is_aircraft_command_unit(*id))
        {
            return Ok(Some(Denial::Unsupported));
        }
        if let Command::Ability {
            at,
            queue,
            automatic,
            ..
        } = command
        {
            if self.unrepresented_ability_work(&selected, *at, *queue, *automatic)? {
                return Ok(Some(Denial::Unsupported));
            }
            self.preflight_ability_selection(&selected, *at, *queue, *automatic)?;
        }
        // New variants currently actuate their aircraft branches only.
        match command {
            Command::Attack { target, .. } => {
                let target = u32::try_from(*target)
                    .ok()
                    .and_then(|id| self.world.game.units.get(&id));
                let Some(target) = target.filter(|t| {
                    !self.world.game.allied(owner, t.owner)
                        && self.world.game.players[owner].sees(t.id)
                }) else {
                    return Ok(Some(Denial::Unseen));
                };
                if target.air.is_none() {
                    return Ok(Some(Denial::Unsupported));
                }
                if target.hp <= 0.0 {
                    return Ok(Some(Denial::Unseen));
                }
            }
            Command::Escort { target, .. } => {
                let target = u32::try_from(*target)
                    .ok()
                    .and_then(|id| self.world.game.units.get(&id));
                if target.is_none_or(|t| {
                    !self.world.game.allied(owner, t.owner)
                        || self.rules.kinds[t.kind].structure
                        || t.air.is_some()
                }) {
                    return Ok(Some(Denial::Unseen));
                }
            }
            Command::Retreat { .. }
                if selected
                    .iter()
                    .any(|id| !self.is_aircraft_command_unit(*id)) =>
            {
                return Ok(Some(Denial::Unsupported));
            }
            _ => {}
        }
        if selected.iter().any(|id| self.is_aircraft_command_unit(*id))
            && matches!(command, Command::Ability { .. } | Command::Autocast { .. })
        {
            return Ok(Some(Denial::Unsupported));
        }
        use crate::match_aircraft_commands::AircraftOrder;
        let aircraft = (|| -> Result<Vec<_>, crate::match_aviation::AviationError> {
            let mut prepared = Vec::new();
            if let Command::Move { orders, queue, .. } = command {
                for row in orders.iter().take(limit) {
                    let Some(id) = self
                        .eligible_command_unit(row.unit, owner, command)
                        .filter(|id| self.is_aircraft_command_unit(*id))
                    else {
                        continue;
                    };
                    if *queue {
                        return Err(crate::match_aviation::AviationError::Unsupported(
                            "queued aircraft commands are unavailable",
                        ));
                    }
                    let at = Point {
                        x: crate::math::max(
                            0.0,
                            crate::math::min(
                                self.world.game.w as f64 * crate::game::CELL,
                                row.at.x,
                            ),
                        ),
                        z: crate::math::max(
                            0.0,
                            crate::math::min(
                                self.world.game.h as f64 * crate::game::CELL,
                                row.at.z,
                            ),
                        ),
                    };
                    prepared.push(self.prepare_aircraft_order(
                        owner,
                        id,
                        AircraftOrder::Move(at),
                    )?);
                }
            } else {
                for &id in &selected {
                    if !self.is_aircraft_command_unit(id) {
                        continue;
                    }
                    let order = match command {
                        Command::Stop { .. } => AircraftOrder::Stop,
                        Command::Attack { target, queue, .. } => {
                            if *queue {
                                return Err(crate::match_aviation::AviationError::Unsupported(
                                    "queued aircraft commands are unavailable",
                                ));
                            }
                            AircraftOrder::Attack(
                                u32::try_from(*target).expect("validated attack target"),
                            )
                        }
                        Command::Escort { target, .. } => AircraftOrder::Escort(
                            u32::try_from(*target).expect("validated escort target"),
                        ),
                        Command::Retreat { .. } => AircraftOrder::Retreat,
                        Command::Stance { stance, on, .. } => {
                            self.validate_aviation()?;
                            if *stance == Stance::HoldFire && !*on {
                                return Err(crate::match_aviation::AviationError::Unsupported(
                                    "aircraft automatic weapons are unavailable",
                                ));
                            }
                            continue;
                        }
                        _ => unreachable!(),
                    };
                    prepared.push(self.prepare_aircraft_order(owner, id, order)?);
                }
            }
            Ok(prepared)
        })();
        let aircraft = match aircraft {
            Ok(plan) => plan,
            Err(crate::match_aviation::AviationError::Unsupported(_)) => {
                return Ok(Some(Denial::Unsupported));
            }
            Err(e) => return Err(e.to_string()),
        };
        if matches!(
            command,
            Command::Stop { .. } | Command::Move { queue: false, .. }
        ) {
            let rows: Vec<_> = match command {
                Command::Move { orders, .. } => orders.iter().take(limit).map(|o| o.unit).collect(),
                _ => controlled.iter().take(limit).copied().collect(),
            };
            let mut advances: BTreeMap<UnitId, u64> = BTreeMap::new();
            for raw in rows {
                if let Some(id) = self
                    .eligible_command_unit(raw, owner, command)
                    .filter(|id| !self.is_aircraft_command_unit(*id))
                {
                    if self.world.game.units[&id].dig != self.field_works.is_active(id)
                        || self.world.game.units[&id].entrench
                            != self.field_works.membership(id).is_some()
                    {
                        return Err("unrepresented or inconsistent field-work activity".into());
                    }
                    *advances.entry(id).or_default() += 1;
                }
            }
            for (id, count) in advances {
                if self.world.game.units[&id]
                    .path_gen
                    .checked_add(count)
                    .is_none()
                {
                    return Err("unit path identity exhausted".into());
                }
            }
        }
        if !self.intercept_convoy_movement(owner, command, &controlled)? {
            return Ok(Some(Denial::Blocked));
        }
        // The source applies forced supply holds before ordinary ownership filtering.
        for raw in &controlled {
            if let Ok(id) = u32::try_from(*raw)
                && self
                    .world
                    .game
                    .units
                    .get(&id)
                    .is_some_and(|u| u.owner == owner)
                && self.logistics.get(id).is_some_and(|r| r.forced)
                && let Some(control) = self.commands.units.get_mut(&id)
            {
                match command {
                    Command::Stop { .. } => control.supply_hold = true,
                    Command::Move {
                        attack_move: false, ..
                    }
                    | Command::Retreat { .. } => control.supply_hold = false,
                    _ => {}
                }
            }
        }

        if let Command::Ability {
            at,
            queue,
            automatic,
            ..
        } = command
        {
            if self.unrepresented_ability_work(&selected, *at, *queue, *automatic)? {
                return Ok(Some(Denial::Unsupported));
            }
            self.preflight_ability_selection(&selected, *at, *queue, *automatic)?;
        }
        for id in &selected {
            self.commands
                .units
                .get_mut(id)
                .expect("selected control")
                .follow = 0;
        }
        match command {
            Command::Stop { .. } => {
                if selected.is_empty() {
                    return Ok(Some(Denial::Needs));
                }
                for prepared in aircraft {
                    self.apply_aircraft_order(prepared);
                }
                for id in selected
                    .into_iter()
                    .filter(|id| !self.is_aircraft_command_unit(*id))
                    .collect::<Vec<_>>()
                {
                    self.cancel_field_work(id)?;
                    self.stop_command_unit(id);
                }
            }
            Command::Stance { stance, on, .. } => {
                if selected.is_empty() {
                    return Ok(Some(Denial::Needs));
                }
                for id in selected {
                    let c = self.commands.units.get_mut(&id).expect("control");
                    match stance {
                        Stance::HoldFire => c.hold_fire = *on,
                        Stance::AutoRetreat => c.auto_retreat = *on,
                        Stance::HoldPosition => {
                            self.world.game.units.get_mut(&id).expect("unit").hold_pos = *on
                        }
                    }
                }
            }
            Command::Move {
                orders,
                attack_move,
                queue,
                together,
                face,
                ..
            } => {
                for prepared in aircraft {
                    self.apply_aircraft_order(prepared);
                }
                let ground: Vec<_> = orders
                    .iter()
                    .take(limit)
                    .filter(|row| {
                        u32::try_from(row.unit)
                            .ok()
                            .and_then(|id| self.world.game.units.get(&id))
                            .is_none_or(|u| !self.is_aircraft_command_unit(u.id))
                    })
                    .cloned()
                    .collect();
                if ground.iter().take(limit).any(|row| {
                    self.eligible_command_unit(row.unit, owner, command)
                        .is_some()
                }) {
                    let denial =
                        self.move_command(owner, &ground, *attack_move, *queue, *together, *face)?;
                    return Ok(
                        if selected.iter().any(|id| self.is_aircraft_command_unit(*id)) {
                            None
                        } else {
                            denial
                        },
                    );
                }
                return Ok(if selected.is_empty() {
                    Some(Denial::Blocked)
                } else {
                    None
                });
            }
            Command::Attack { .. } | Command::Escort { .. } | Command::Retreat { .. } => {
                if aircraft.is_empty() {
                    return Ok(Some(Denial::Needs));
                }
                for prepared in aircraft {
                    self.apply_aircraft_order(prepared);
                }
            }
            Command::Ability {
                ids,
                at,
                queue,
                automatic,
            } => {
                let ordered = ids
                    .iter()
                    .take(limit)
                    .map(|id| self.eligible_command_unit(*id, owner, command))
                    .collect::<Vec<_>>();
                return self.apply_ability_selection(owner, &ordered, *at, *queue, *automatic);
            }
            Command::Autocast { on, .. } => {
                let ready: Vec<_> = selected
                    .into_iter()
                    .filter(|id| {
                        self.ability_kind(*id)
                            .is_ok_and(|kind| kind != crate::match_abilities::AbilityKind::None)
                    })
                    .collect();
                if ready.is_empty() {
                    return Ok(Some(Denial::Needs));
                }
                for id in ready {
                    self.commands.units.get_mut(&id).unwrap().auto_cast = *on;
                }
            }
            Command::Group { .. }
            | Command::FireAt { .. }
            | Command::Build { .. }
            | Command::Assist { .. }
            | Command::Destroy { .. }
            | Command::Board { .. }
            | Command::Unload { .. }
            | Command::Entrench { .. }
            | Command::Dig { .. }
            | Command::Technology { .. }
            | Command::Buy { .. }
            | Command::CompanionJoin { .. }
            | Command::CompanionInput { .. }
            | Command::Support { .. }
            | Command::Cover { .. }
            | Command::Unsupported => unreachable!(),
        }
        Ok(None)
    }
    fn stop_command_unit(&mut self, id: UnitId) {
        let u = self.world.game.units.get_mut(&id).expect("selected unit");
        let c = self.commands.units.get_mut(&id).expect("selected control");
        u.traffic = None;
        u.traffic_wait = 0.0;
        u.world_goal = None;
        u.entrench = false;
        u.path.clear();
        u.path_gen += 1;
        u.attack = false;
        u.retreating = false;
        u.dig = false;
        u.board = 0;
        u.amove = None;
        u.build = false;
        c.build_target = None;
        c.orders.clear();
        c.face = None;
        c.attack_id = 0;
        c.nade = None;
        c.enter = None;
        c.fire_at = None;
        c.outcome = Some(MoveOutcome::Interrupted);
        c.outcome_tick = Some(self.world.game.tick);
    }
}

impl MatchCommands {
    pub fn next_tick(&self) -> Option<u64> {
        self.queue.next_tick()
    }
    pub fn pending_commands(&self) -> usize {
        self.queue.len()
    }
    /// Inspect canonical inputs at one boundary without consuming or cloning them.
    pub fn pending_tick(
        &self,
        tick: u64,
    ) -> impl Iterator<Item = (&crate::orders::OrderKey, &Command)> {
        self.queue.pending_tick(tick)
    }
    pub fn path_requests(&self) -> &PathRequests {
        &self.requests
    }
    #[cfg(test)]
    pub(crate) fn inspected_path_tick(&self) -> Option<i64> {
        self.path_tick
    }
}
impl crate::match_runtime::Match {
    fn move_command(
        &mut self,
        owner: usize,
        orders: &[MoveOrder],
        attack_move: bool,
        queued: bool,
        together: bool,
        face: Option<f64>,
    ) -> Result<Option<Denial>, String> {
        use crate::command_destinations::{Context, Request};
        let face = face.and_then(crate::formation::normalize_face);
        let command = Command::Move {
            control_ids: None,
            orders: orders.to_vec(),
            attack_move,
            queue: queued,
            together,
            face,
        };
        let mut requests: Vec<_> = orders
            .iter()
            .take(50.max(self.world.game.units.len()))
            .filter_map(|o| {
                self.eligible_command_unit(o.unit, owner, &command)
                    .map(|id| Request {
                        id,
                        at: Point {
                            x: crate::math::max(
                                0.0,
                                crate::math::min(
                                    self.world.game.w as f64 * crate::game::CELL,
                                    o.at.x,
                                ),
                            ),
                            z: crate::math::max(
                                0.0,
                                crate::math::min(
                                    self.world.game.h as f64 * crate::game::CELL,
                                    o.at.z,
                                ),
                            ),
                        },
                    })
            })
            .collect();
        if requests.is_empty() {
            return Ok(Some(Denial::Blocked));
        }
        let points: Vec<_> = self.start.points.iter().map(|p| p.at).collect();
        let hit_at = self.combat.iter().map(|(id, c)| (id, c.hit_at)).collect();
        let context = Context {
            game: &self.world.game,
            rules: &self.rules,
            grid: &self.grid,
            points: &points,
            hit_at: &hit_at,
        };
        if queued {
            if let Some(face) = face {
                requests =
                    crate::command_destinations::spread_queued_facing(&context, &requests, face)
                        .map_err(|e| format!("queued facing: {e:?}"))?;
            }
            let mut any = false;
            for r in requests {
                let control = self.commands.units.get_mut(&r.id).expect("control");
                if control.orders.len() < 8 {
                    control.orders.push_back(Command::Move {
                        control_ids: None,
                        orders: vec![MoveOrder {
                            unit: i64::from(r.id),
                            at: r.at,
                        }],
                        attack_move,
                        queue: false,
                        together: false,
                        face,
                    });
                    any = true;
                }
            }
            return Ok((!any).then_some(Denial::QueueFull));
        }
        let destinations = crate::command_destinations::end_spots(&context, &requests, face)
            .map_err(|e| format!("move destination: {e:?}"))?;
        let pace = if together && requests.len() > 1 {
            requests
                .iter()
                .map(|r| self.rules.kinds[self.world.game.units[&r.id].kind].speed)
                .fold(f64::INFINITY, crate::math::min)
        } else {
            0.0
        };
        let behavior = Catalog::compiled()
            .map_err(|e| e.to_string())?
            .table("CFG")
            .map_err(|e| e.to_string())?
            .get("behavior")
            .ok_or("missing command behavior")?;
        let threat = behavior
            .get("threatTime")
            .and_then(|v| v.number())
            .ok_or("missing threat time")?
            / self.rules.movement.tick;
        let reverse_max = behavior
            .get("reverseMax")
            .and_then(|v| v.number())
            .ok_or("missing reverse distance")?;
        for r in destinations {
            self.cancel_field_work(r.id)?;
            let mut u = self.world.game.units[&r.id].clone();
            let engaged = self
                .firing
                .units
                .get(&r.id)
                .is_some_and(|s| s.target_id != 0)
                || self.world.game.tick as f64 - self.combat.get(r.id).map_or(-1e9, |s| s.hit_at)
                    <= threat;
            if u.garrison.is_some() {
                // Leaving a house is physical placement; route searches use the owner view.
                let nav = physical_navigation(&self.world.game);
                let c = crate::unit_spawn::SpawnPlacer::new()
                    .nearest_free(nav, u.at(), crate::game::MOVE, None)
                    .map_err(|e| format!("garrison exit: {e:?}"))?;
                u.x = (c % nav.width) as f64 * 2.0 + 1.0;
                u.z = (c / nav.width) as f64 * 2.0 + 1.0;
                u.garrison = None;
                self.world.game.units.insert(r.id, u.clone());
                self.grid.update(r.id, &u);
            }
            let result = self.command_route(owner, &u, r.at)?;
            self.commands.requests.clear_failures(r.id);
            u.pace = pace;
            u.traffic = None;
            u.traffic_wait = 0.0;
            u.traffic_entered = false;
            u.traffic_trail.clear();
            u.entrench = false;
            u.attack = false;
            u.stuck = 0.0;
            u.retreating = false;
            u.dig = false;
            u.board = 0;
            u.build = false;
            u.amove = attack_move.then_some(r.at);
            u.drift = false;
            u.world_goal = (!attack_move).then_some(r.at);
            u.path = result.waypoints;
            u.path_gen += 1;
            u.reverse = None;
            if !self.rules.kinds[u.kind].infantry
                && engaged
                && let Some(end) = u.path.last().copied()
                && crate::math::hypot(end.x - u.x, end.z - u.z) <= reverse_max
                && crate::math::cos(crate::math::atan2(end.z - u.z, end.x - u.x) - u.rot) < -0.5
            {
                u.reverse = Some(end);
            }

            if let Some(reserve) = self.logistics.get_mut(r.id).filter(|r| r.forced) {
                u.retreating = !u.path.is_empty();
                reserve.stranded = u.path.is_empty();
            }
            let c = self.commands.units.get_mut(&r.id).expect("control");
            c.build_target = None;
            c.orders.clear();
            c.face = face.map(|angle| Facing { angle, at: r.at });
            c.attack_id = 0;
            c.nade = None;
            c.enter = None;
            c.fire_at = None;
            c.rally_drift = false;
            c.route_end = Some(result.route_end);
            c.outcome = Some(MoveOutcome::Moving);
            c.outcome_tick = Some(self.world.game.tick);
            if self.logistics.get(r.id).is_some_and(|s| s.forced) {
                c.supply_escape = true;
            }
            if let Some(firing) = self.firing.units.get_mut(&r.id) {
                firing.target_id = 0;
            }
            self.world.game.units.insert(r.id, u);
            self.grid.update(r.id, &self.world.game.units[&r.id]);
        }
        Ok(None)
    }
    pub(crate) fn command_route(
        &mut self,
        owner: usize,
        u: &Unit,
        to: Point,
    ) -> Result<crate::pathfinding::PathResult, String> {
        if owner != u.owner {
            return Err("route owner mismatch".into());
        }
        self.search_unit_route(u, to).map(|route| route.result)
    }
}
pub(crate) struct CommandRoute {
    pub(crate) result: crate::pathfinding::PathResult,
    pub(crate) failed: bool,
}
pub(crate) fn search_route(
    paths: &mut Pathfinding,
    terrain: crate::pathfinding::PathTerrain<'_>,
    rules: crate::pathfinding::PathRules,
    kind: &crate::rules::UnitKind,
    u: &Unit,
    to: Point,
) -> Result<CommandRoute, String> {
    let mut result = paths
        .find_path(terrain, rules, kind, u.at(), to, None)
        .map_err(|e| format!("command path: {e:?}"))?;
    let live_route_end = result.route_end;
    let mut failed = result.status == crate::pathfinding::PathStatus::Unreachable;
    if result.waypoints.first().is_some_and(|p| {
        !crate::pathfinding::walkable(terrain.navigation, u.at(), *p, kind.block(), 0.9)
    }) {
        let nav = terrain.navigation;
        let c = cell_at(nav, u.at()).unwrap_or(0);
        let center = Point {
            x: (c % nav.width) as f64 * 2.0 + 1.0,
            z: (c / nav.width) as f64 * 2.0 + 1.0,
        };
        for at in [
            Point {
                x: center.x,
                z: u.z,
            },
            Point {
                x: u.x,
                z: center.z,
            },
            center,
        ] {
            if crate::math::hypot(u.x - at.x, u.z - at.z) < 0.05
                || !crate::pathfinding::walkable(nav, u.at(), at, kind.block(), 0.9)
            {
                continue;
            }
            let next = paths
                .find_path(terrain, rules, kind, at, to, None)
                .map_err(|e| format!("bank path: {e:?}"))?;
            failed |= next.status == crate::pathfinding::PathStatus::Unreachable;
            if next
                .waypoints
                .first()
                .is_some_and(|p| crate::pathfinding::walkable(nav, at, *p, kind.block(), 0.9))
            {
                result = next;
                result.route_end = live_route_end;
                result.waypoints.insert(0, at);
                break;
            }
        }
    }
    Ok(CommandRoute { result, failed })
}
pub(crate) fn physical_navigation(game: &crate::game::Game) -> crate::navigation::TerrainView<'_> {
    crate::navigation::TerrainView {
        width: game.w,
        height: game.h,
        flags: &game.flags,
        heights: game.height.as_deref(),
        world_known: false,
        knowledge_id: 0,
        versions: crate::navigation::Versions::default(),
        changed: None,
        changes: &[],
    }
}

fn cell_at(v: crate::navigation::TerrainView<'_>, at: Point) -> Option<usize> {
    let x = (at.x / 2.0).floor();
    let y = (at.z / 2.0).floor();
    (x >= 0.0 && y >= 0.0 && x < v.width as f64 && y < v.height as f64)
        .then(|| y as usize * v.width + x as usize)
}

struct RequestContext<'a> {
    match_state: &'a mut crate::match_runtime::Match,
    shootable: BTreeMap<UnitId, bool>,
    error: Option<String>,
}
impl crate::path_requests::PathRequestContext for RequestContext<'_> {
    type Error = String;
    fn current_goal(
        &mut self,
        id: UnitId,
        kind: crate::path_requests::RequestKind,
    ) -> Option<Point> {
        use crate::path_requests::RequestKind;
        if kind == RequestKind::FireAt {
            return match self.match_state.current_fire_at_goal(id) {
                Ok(goal) => goal,
                Err(error) => {
                    self.error = Some(error);
                    None
                }
            };
        }
        if kind == RequestKind::Dig {
            return self.match_state.current_dig_goal(id);
        }
        if kind == RequestKind::Build {
            return match self.match_state.current_build_goal(id) {
                Ok(goal) => goal,
                Err(error) => {
                    self.error = Some(error);
                    None
                }
            };
        }
        if kind == RequestKind::AttackId {
            return match self.match_state.current_attack_goal(id) {
                Ok(goal) => goal,
                Err(error) => {
                    self.error = Some(error);
                    None
                }
            };
        }
        if kind == RequestKind::Board {
            return match self.match_state.current_board_goal(id) {
                Ok(goal) => goal,
                Err(error) => {
                    self.error = Some(error);
                    None
                }
            };
        }
        let u = self.match_state.world.game.units.get(&id)?;
        if u.hp <= 0.0 {
            return None;
        }
        match kind {
            RequestKind::Recovery => (!u.path.is_empty() && u.stuck > 0.75 && u.traffic.is_none())
                .then(|| u.world_goal.or(u.amove).or_else(|| u.path.last().copied()))
                .flatten(),
            RequestKind::World => u.world_goal.filter(|at| {
                u.path.is_empty() && crate::math::hypot(at.x - u.x, at.z - u.z) > 0.25
            }),
            RequestKind::Nade => self
                .match_state
                .commands
                .units
                .get(&id)
                .and_then(|c| c.nade),
            RequestKind::Amove if !u.retreating => u.amove.filter(|at| {
                u.path.is_empty()
                    && crate::math::hypot(at.x - u.x, at.z - u.z) >= 2.5
                    && !self.shootable.get(&id).copied().unwrap_or(false)
            }),
            _ => None,
        }
    }
    fn unit_incarnation(&self, id: UnitId) -> Option<u64> {
        self.match_state
            .world
            .game
            .units
            .contains_key(&id)
            .then_some(u64::from(id))
    }
    fn knowledge_version(&self, id: UnitId) -> u64 {
        self.match_state
            .world
            .game
            .units
            .get(&id)
            .map_or(0, |u| self.match_state.route_failure_version(u.owner))
    }
    fn search(
        &mut self,
        r: crate::path_requests::Request,
    ) -> Result<crate::path_requests::SearchOutcome, String> {
        let u = self.match_state.world.game.units[&r.unit].clone();
        let outcome = self.match_state.search_unit_route(&u, r.goal)?;
        let route = outcome.result;
        self.match_state
            .commands
            .units
            .get_mut(&r.unit)
            .ok_or("missing route control")?
            .route_end = Some(route.route_end);
        Ok(crate::path_requests::SearchOutcome {
            failed: outcome.failed,
            path: route.waypoints,
        })
    }
    fn apply(&mut self, update: crate::path_requests::PathUpdate) {
        if update.dropped.is_some()
            && update.request.kind == crate::path_requests::RequestKind::Build
            && let Err(error) = self
                .match_state
                .drop_construction_order(update.request.unit)
        {
            self.error = Some(error);
            return;
        }
        if update.dropped.is_some()
            && update.request.kind == crate::path_requests::RequestKind::Dig
            && let Err(error) = self.match_state.drop_field_work_job(update.request.unit)
        {
            self.error = Some(error);
            return;
        }
        let u = self
            .match_state
            .world
            .game
            .units
            .get_mut(&update.request.unit)
            .expect("searched unit");
        u.repath = update.repath;
        if let Some(path) = update.path {
            u.path = path;
            u.path_gen += 1;
        }
        if let Some(drop) = update.dropped {
            match update.request.kind {
                crate::path_requests::RequestKind::World => u.world_goal = None,
                crate::path_requests::RequestKind::Amove => u.amove = None,
                crate::path_requests::RequestKind::Board => u.board = 0,
                crate::path_requests::RequestKind::AttackId => {
                    u.attack = false;
                    self.match_state
                        .commands
                        .units
                        .get_mut(&u.id)
                        .unwrap()
                        .attack_id = 0
                }
                crate::path_requests::RequestKind::FireAt => {
                    self.match_state
                        .commands
                        .units
                        .get_mut(&u.id)
                        .unwrap()
                        .fire_at = None
                }
                crate::path_requests::RequestKind::Nade => {
                    self.match_state.commands.units.get_mut(&u.id).unwrap().nade = None;
                    let ability = self.match_state.abilities.get_mut(u.id).unwrap();
                    ability.aimed_auto = false;
                    ability.aimed_walked = false;
                }
                _ => {}
            }
            u.stuck = 0.0;
            if let Some(c) = self.match_state.commands.units.get_mut(&u.id) {
                c.outcome = Some(MoveOutcome::Unreachable);
                c.outcome_tick = Some(drop.tick as i64);
                c.move_result = Some(MoveResult::Unreachable {
                    tick: drop.tick as i64,
                });
            }
        }
    }
}
impl crate::match_runtime::Match {
    fn command_shootable(&self) -> Result<BTreeMap<UnitId, bool>, String> {
        if self.uses_world_routes() {
            self.validate_world_path_service()?;
        }
        self.world
            .game
            .units
            .keys()
            .map(|&id| {
                let target = self.firing.units.get(&id).map_or(0, |f| f.target_id);
                Ok((id, target != 0 && self.can_shoot_target(id, target)?))
            })
            .collect()
    }
    pub fn begin_path_tick(
        &mut self,
        budget: usize,
        retry_limit: u32,
    ) -> Result<crate::path_requests::TickReport, String> {
        if self.uses_world_routes() {
            self.validate_world_path_service()?;
        } else {
            self.validate_building_activity_rows()?;
        }
        if self.world.game.tick < 0 {
            return Err("negative path tick".into());
        }
        if self
            .world
            .game
            .units
            .values()
            .any(|u| u.path_gen == u64::MAX)
        {
            return Err("unit path identity exhausted".into());
        }
        // current_goal cannot report errors through the scheduler's Option API.
        // Validate Board and Attack callbacks before observation or FIFO service can write.
        for request in self.commands.requests.queued_requests() {
            if request.kind == crate::path_requests::RequestKind::FireAt {
                self.current_fire_at_goal(request.unit)?;
            }
            if request.kind == crate::path_requests::RequestKind::AttackId {
                self.current_attack_goal(request.unit)?;
            }
            if request.kind == crate::path_requests::RequestKind::Board {
                self.current_board_goal(request.unit)?;
            }
        }
        if !self.uses_world_routes() {
            for owner in 0..self.start.players.len() {
                self.refresh_recipient_routing(owner)?;
            }
        }
        let shootable = self.command_shootable()?;
        let config = crate::path_requests::TickConfig {
            budget,
            retry_limit,
            tick: self.world.game.tick as u64,
            tick_seconds: self.rules.movement.tick,
        };
        let mut requests = std::mem::take(&mut self.commands.requests);
        let mut context = RequestContext {
            match_state: self,
            shootable,
            error: None,
        };
        let result = requests.begin_tick(config, &mut context);
        let error = context.error.take();
        self.commands.requests = requests;
        if let Some(error) = error {
            return Err(error);
        }
        if result.is_ok() {
            self.commands.path_tick = Some(self.world.game.tick);
        }
        result.map_err(|e| format!("path tick: {e:?}"))
    }
    /// Source pre-timer world completion/retry and per-unit waiting-order phase.
    pub fn prepare_unit_commands(
        &mut self,
        id: UnitId,
        winner_decided: bool,
    ) -> Result<(), String> {
        self.prepare_unit_commands_observing(id, winner_decided, &mut |_, _| Ok(()))
    }
    pub(crate) fn prepare_unit_commands_observing(
        &mut self,
        id: UnitId,
        winner_decided: bool,
        observer: &mut impl FnMut(&mut Self, &Command) -> Result<(), String>,
    ) -> Result<(), String> {
        let Some(u) = self.world.game.units.get(&id) else {
            return Ok(());
        };
        if u.hp <= 0.0
            || self.rules.kinds[u.kind].structure
            || self.commands.units.get(&id).is_some_and(|c| c.operative)
        {
            return Ok(());
        }
        if u.air.is_some() {
            return self.start_queued_orders(id, winner_decided);
        }
        if let Some(goal) = u.world_goal {
            let end = self
                .commands
                .units
                .get(&id)
                .and_then(|c| c.route_end)
                .unwrap_or(goal);
            let completed = match self.world_retreat_completion(id, goal)? {
                Some(completed) => completed,
                None => {
                    crate::math::hypot(u.x - end.x, u.z - end.z) <= 0.25
                        && u.path.is_empty()
                        && u.move_speed.abs() < 0.1
                }
            };
            if completed {
                let u = self.world.game.units.get_mut(&id).expect("unit");
                u.world_goal = None;
                if u.retreating {
                    u.path.clear();
                    u.retreating = false;
                }
                let c = self
                    .commands
                    .units
                    .get_mut(&id)
                    .ok_or("missing completion control")?;
                c.outcome = Some(MoveOutcome::Arrived);
                c.outcome_tick = Some(self.world.game.tick);
                c.move_result = Some(MoveResult::Completed {
                    tick: self.world.game.tick,
                });
            } else if u.path.is_empty()
                && crate::math::hypot(u.x - goal.x, u.z - goal.z) > 0.25
                && u.repath <= 0.0
            {
                self.request_command_path(id, goal, crate::path_requests::RequestKind::World)?;
            }
        }
        self.start_queued_orders_observing(id, winner_decided, observer)
    }
    pub fn start_queued_orders(&mut self, id: UnitId, winner_decided: bool) -> Result<(), String> {
        self.start_queued_orders_observing(id, winner_decided, &mut |_, _| Ok(()))
    }
    fn start_queued_orders_observing(
        &mut self,
        id: UnitId,
        winner_decided: bool,
        observer: &mut impl FnMut(&mut Self, &Command) -> Result<(), String>,
    ) -> Result<(), String> {
        let Some(u) = self.world.game.units.get(&id) else {
            return Ok(());
        };
        if u.air.is_some() {
            return if self
                .commands
                .units
                .get(&id)
                .ok_or("missing queued control")?
                .orders
                .is_empty()
            {
                Ok(())
            } else {
                Err("queued aircraft commands are unsupported".into())
            };
        }
        let control = self
            .commands
            .units
            .get(&id)
            .ok_or("missing queued control")?;
        crate::match_construction::validate_build_activity(u, control)?;
        if !control.orders.is_empty() && u.drift && !control.rally_drift {
            if u.path_gen.checked_add(2).is_none() {
                return Err("unit path identity exhausted".into());
            }
            let u = self.world.game.units.get_mut(&id).expect("unit");
            u.drift = false;
            u.path.clear();
            u.path_gen += 1;
            u.reverse = None;
        }
        loop {
            let u = &self.world.game.units[&id];
            let c = &self.commands.units[&id];
            let attack = c.attack_id != 0
                && self.world.game.players[u.owner].sees(c.attack_id)
                && self.world.game.units.contains_key(&c.attack_id);
            if u.retreating
                || attack
                || c.fire_at.is_some()
                || c.nade.is_some()
                || u.dig
                || self.project_goal(id)?.is_some()
                || c.build_target
                    .is_some_and(|site| self.world.game.units.contains_key(&site))
                || c.enter.is_some()
                || u.board != 0
                || u.amove.is_some()
                || u.world_goal.is_some()
                || !u.path.is_empty()
                || c.orders.is_empty()
            {
                break;
            }
            let owner = u.owner;
            let c = self.commands.units.get_mut(&id).expect("control");
            let order = c.orders.pop_front().expect("nonempty");
            let remaining = std::mem::take(&mut c.orders);
            let result = self.execute_command(owner, &order, winner_decided);
            self.commands.units.get_mut(&id).expect("control").orders = remaining;
            observer(self, &order)?;
            result?;
        }
        Ok(())
    }
    /// Source attack-move branch after timer decrement, before physical movement.
    pub fn apply_attack_move(&mut self, id: UnitId) -> Result<(), String> {
        let Some(u) = self.world.game.units.get(&id) else {
            return Ok(());
        };
        let Some(goal) = u.amove else {
            return Ok(());
        };
        if crate::math::hypot(u.x - goal.x, u.z - goal.z) < 2.5 {
            self.world.game.units.get_mut(&id).expect("unit").amove = None;
            return Ok(());
        }
        let target = self.firing.units.get(&id).map_or(0, |f| f.target_id);
        if target != 0 && self.can_shoot_target(id, target)? {
            if u.path_gen == u64::MAX {
                return Err("unit path identity exhausted".into());
            }
            let u = self.world.game.units.get_mut(&id).expect("unit");
            u.path.clear();
            u.path_gen += 1;
        } else if u.path.is_empty() && u.repath <= 0.0 {
            self.request_command_path(id, goal, crate::path_requests::RequestKind::Amove)?;
        }
        Ok(())
    }
    /// Source Board overflow eligibility follows the carrier's current canonical pose.
    /// Seat count and ownership are checked by the unit phase, after early queue service.
    pub fn current_board_goal(&self, id: UnitId) -> Result<Option<Point>, String> {
        self.validate_carrier_world()?;
        let Some(unit) = self.world.game.units.get(&id) else {
            return Ok(None);
        };
        self.validate_carrier_unit(id)?;
        if unit.hp <= 0.0 || unit.retreating || !unit.path.is_empty() || unit.board == 0 {
            return Ok(None);
        }
        if self.carrier_boundary(id) {
            return Err("unsupported queued boarding actor".into());
        }
        let Some(carrier) = self.world.game.units.get(&unit.board) else {
            return Ok(None);
        };
        self.validate_carrier_unit(carrier.id)?;
        if carrier.hp <= 0.0 {
            return Ok(None);
        }
        let reach = Catalog::compiled()
            .map_err(|e| e.to_string())?
            .table("CFG")
            .map_err(|e| e.to_string())?
            .get("aid")
            .and_then(|v| v.get("board"))
            .and_then(|v| v.number())
            .ok_or("missing boarding reach")?;
        if crate::math::hypot(unit.x - carrier.x, unit.z - carrier.z) <= reach {
            return Ok(None);
        }
        if self.carrier_boundary(carrier.id) {
            return Err("unsupported queued boarding carrier".into());
        }
        Ok(Some(carrier.at()))
    }
    /// Checked unit-loop handoff. Boarding skips the rest of the turn; a route request
    /// uses the existing owner-bound scheduler and never stores another carrier pose.
    pub fn apply_unit_boarding_with_routes(
        &mut self,
        id: UnitId,
    ) -> Result<crate::match_carriers::BoardingOutcome, String> {
        if self.commands.path_tick != Some(self.world.game.tick) {
            return Err("begin_path_tick must run before boarding".into());
        }
        let outcome = self.apply_unit_boarding(id)?;
        if let crate::match_carriers::BoardingOutcome::NeedsPath { at, .. } = outcome {
            self.request_command_path(id, at, crate::path_requests::RequestKind::Board)?;
        }
        Ok(outcome)
    }
    pub fn request_command_path(
        &mut self,
        id: UnitId,
        goal: Point,
        kind: crate::path_requests::RequestKind,
    ) -> Result<crate::path_requests::Disposition, String> {
        if self.uses_world_routes() {
            Self::validate_world_request_kind(kind)?;
        }
        if !matches!(
            kind,
            crate::path_requests::RequestKind::Recovery
                | crate::path_requests::RequestKind::World
                | crate::path_requests::RequestKind::Amove
                | crate::path_requests::RequestKind::Nade
                | crate::path_requests::RequestKind::Dig
                | crate::path_requests::RequestKind::Build
                | crate::path_requests::RequestKind::Board
                | crate::path_requests::RequestKind::AttackId
                | crate::path_requests::RequestKind::FireAt
        ) {
            return Err("unsupported command path request".into());
        }
        if self.commands.path_tick != Some(self.world.game.tick) {
            return Err("begin_path_tick must run before path requests".into());
        }
        if self
            .world
            .game
            .units
            .get(&id)
            .is_some_and(|u| u.path_gen == u64::MAX)
        {
            return Err("unit path identity exhausted".into());
        }
        if kind == crate::path_requests::RequestKind::AttackId {
            self.current_attack_goal(id)?;
        }
        if kind == crate::path_requests::RequestKind::FireAt {
            self.current_fire_at_goal(id)?;
        }
        if kind == crate::path_requests::RequestKind::Board {
            self.current_board_goal(id)?;
        }
        let shootable = self.command_shootable()?;
        let mut requests = std::mem::take(&mut self.commands.requests);
        let mut context = RequestContext {
            match_state: self,
            shootable,
            error: None,
        };
        let result = requests.request(
            crate::path_requests::Request {
                unit: id,
                goal,
                kind,
            },
            &mut context,
        );
        let error = context.error.take();
        self.commands.requests = requests;
        if let Some(error) = error {
            return Err(error);
        }
        result.map_err(|e| format!("command path request: {e:?}"))
    }
}

#[cfg(test)]
mod tests;

pub(crate) struct MovingRequestContext<'a, 'terrain> {
    pub(crate) unit: &'a mut Unit,
    pub(crate) control: &'a mut UnitControl,
    pub(crate) paths: &'a mut Pathfinding,
    pub(crate) terrain: crate::pathfinding::PathTerrain<'terrain>,
    pub(crate) profile: &'a crate::rules::UnitKind,
    pub(crate) rules: crate::pathfinding::PathRules,
    pub(crate) failure_version: u64,
    pub(crate) terrain_read: Option<&'a std::cell::Cell<bool>>,
}
impl crate::path_requests::PathRequestContext for MovingRequestContext<'_, '_> {
    type Error = String;
    fn current_goal(
        &mut self,
        id: UnitId,
        kind: crate::path_requests::RequestKind,
    ) -> Option<Point> {
        let u = &self.unit;
        (id == u.id
            && u.hp > 0.0
            && kind == crate::path_requests::RequestKind::Recovery
            && !u.path.is_empty()
            && u.stuck > 0.75
            && u.traffic.is_none())
        .then(|| u.world_goal.or(u.amove).or_else(|| u.path.last().copied()))
        .flatten()
    }
    fn unit_incarnation(&self, id: UnitId) -> Option<u64> {
        (id == self.unit.id).then_some(u64::from(id))
    }
    fn knowledge_version(&self, _id: UnitId) -> u64 {
        self.failure_version
    }
    fn search(
        &mut self,
        request: crate::path_requests::Request,
    ) -> Result<crate::path_requests::SearchOutcome, String> {
        if let Some(read) = self.terrain_read {
            read.set(true);
        }
        let route = search_route(
            self.paths,
            self.terrain,
            self.rules,
            self.profile,
            self.unit,
            request.goal,
        )?;
        self.control.route_end = Some(route.result.route_end);
        Ok(crate::path_requests::SearchOutcome {
            path: route.result.waypoints,
            failed: route.failed,
        })
    }
    fn apply(&mut self, update: crate::path_requests::PathUpdate) {
        self.unit.repath = update.repath;
        if let Some(path) = update.path {
            self.unit.path = path;
            self.unit.path_gen += 1;
        }
    }
}
impl crate::match_runtime::Match {
    /// Execute one movement block after begin_path_tick on this tick.
    /// A search error aborts the tick; movement may already have changed the unit.
    pub fn move_unit_with_command_routes(
        &mut self,
        id: UnitId,
        suppression_speed: f64,
    ) -> Result<Option<crate::movement::Moved>, String> {
        if self.commands.path_tick != Some(self.world.game.tick) {
            return Err("begin_path_tick must run before movement".into());
        }
        if !suppression_speed.is_finite() || suppression_speed < 0.0 {
            return Err("invalid suppression speed".into());
        }
        let Some(unit) = self.world.game.units.get(&id) else {
            return Ok(None);
        };
        if unit.path_gen.checked_add(2).is_none() {
            return Err("unit path identity exhausted".into());
        }
        if self.uses_world_routes() {
            return self.move_world_unit_with_routes(id, suppression_speed);
        }
        let owner = unit.owner;
        let kind = unit.kind;
        let before = unit.at();
        let dt = self.rules.movement.tick;
        let fuel_speed = self.logistics.get(id).and_then(|r| {
            r.fuel.map(|fuel| {
                let available = fuel
                    + if r.forced {
                        r.emergency.unwrap_or(0.0)
                    } else {
                        0.0
                    };
                crate::math::min(1.0, available / dt)
            })
        });
        if fuel_speed.is_some() {
            self.validate_supply()?;
        }
        self.refresh_recipient_routing(owner)?;
        let query = self.recipients[owner].query();
        let terrain = query
            .path_terrain(self.recipients[owner].live())
            .map_err(|e| format!("movement recipient path: {e:?}"))?;
        let path_rules = crate::pathfinding::PathRules::compiled().map_err(|e| e.to_string())?;
        let profile = self.rules.kinds.get(kind).ok_or("unknown moving kind")?;
        let control = self
            .commands
            .units
            .get_mut(&id)
            .ok_or("missing moving control")?;
        let mut error = None;
        let traffic_knowledge = crate::traffic_knowledge::TrafficKnowledge::observed(
            query.navigation(),
            self.recipients[owner].knowledge(),
            self.recipients[owner].wrecks(),
        )
        .map_err(|e| format!("movement recipient traffic: {e:?}"))?;
        // The final multiplier applies after retreat/sprint select their speed.
        let moved = crate::movement::move_unit_with_march(
            &mut self.world.game,
            &self.rules,
            &mut self.grid,
            id,
            suppression_speed,
            &mut |_, _, u, to, why| {
                if error.is_some() {
                    return;
                }
                match why {
                    crate::movement::RouteRequest::Recovery => {
                        let mut context = MovingRequestContext {
                            unit: u,
                            control,
                            paths: &mut self.commands.paths,
                            terrain,
                            profile,
                            rules: path_rules,
                            failure_version: terrain.navigation.versions.knowledge,
                            terrain_read: None,
                        };
                        if let Err(e) = self.commands.requests.request(
                            crate::path_requests::Request {
                                unit: id,
                                goal: to,
                                kind: crate::path_requests::RequestKind::Recovery,
                            },
                            &mut context,
                        ) {
                            error = Some(format!("movement recovery: {e:?}"));
                        }
                    }
                    crate::movement::RouteRequest::Immediate => {
                        self.commands.requests.clear_failures(id);
                        match search_route(
                            &mut self.commands.paths,
                            terrain,
                            path_rules,
                            profile,
                            u,
                            to,
                        ) {
                            Ok(route) => {
                                control.route_end = Some(route.result.route_end);
                                if !route.result.waypoints.is_empty() {
                                    u.path = route.result.waypoints;
                                    u.path_gen += 1;
                                }
                                u.repath = 1.0;
                            }
                            Err(e) => error = Some(e),
                        }
                    }
                }
            },
            &traffic_knowledge,
            fuel_speed.unwrap_or(1.0),
        )
        .map_err(|e| format!("movement recipient traffic: {e:?}"))?;
        if fuel_speed.is_some()
            && let Some(result) = &moved
        {
            let unit = &self.world.game.units[&id];
            let distance = crate::math::hypot(unit.x - before.x, unit.z - before.z);
            let driving = crate::math::min(dt, distance / crate::math::max(result.speed, 0.001));
            self.consume_unit_driving_fuel(id, driving)?;
        }
        if let Some(error) = error {
            return Err(error);
        }
        Ok(moved)
    }
}
