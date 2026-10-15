//! Tutorial step definitions and completion checks from pinned `shared/tutorial.js`.
//! This component does not start a Tutorial match or apply spawn/command effects.

use crate::game::{CELL, Point};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TutorialStep {
    Hedgerow,
    Orchard,
    Crossroads,
    Recruit,
    Tank,
    Trench,
    Retreat,
    Artillery,
    Bridgehead,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SpawnOrder {
    None,
    AttackMove(Point),
    Garrison(Point),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TutorialSpawn {
    pub kind: &'static str,
    pub at: Point,
    pub health_share: Option<f64>,
    pub order: SpawnOrder,
    pub auto_retreat: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StepEntry {
    pub goal: &'static str,
    pub at: Point,
    pub manpower_floor: Option<f64>,
    pub spawns: Vec<TutorialSpawn>,
}

#[derive(Clone, Copy, Debug)]
pub struct TutorialPlayer {
    pub team: i32,
    pub artillery_cooldown: f64,
}

#[derive(Clone, Copy, Debug)]
pub struct TutorialUnit<'a> {
    pub id: u32,
    pub owner: usize,
    pub kind: &'a str,
    pub hp: f64,
    pub x: f64,
    pub retreating: bool,
}

/// Current authoritative facts, not a recipient view or an alternative Game store.
pub struct CompletionView<'a> {
    pub players: &'a [TutorialPlayer],
    pub units: &'a [TutorialUnit<'a>],
    pub point_owners: &'a [Option<usize>],
    /// IDs created by the current step, retained even after retirement.
    pub spawned: &'a [u32],
}

const fn cell(x: u16, z: u16) -> Point {
    Point {
        x: (x as f64 + 0.5) * CELL,
        z: (z as f64 + 0.5) * CELL,
    }
}

fn spawn(
    kind: &'static str,
    x: u16,
    z: u16,
    health_share: Option<f64>,
    order: SpawnOrder,
) -> TutorialSpawn {
    TutorialSpawn {
        kind,
        at: cell(x, z),
        health_share,
        order,
        auto_retreat: false,
    }
}

impl TutorialStep {
    pub const ALL: [Self; 9] = [
        Self::Hedgerow,
        Self::Orchard,
        Self::Crossroads,
        Self::Recruit,
        Self::Tank,
        Self::Trench,
        Self::Retreat,
        Self::Artillery,
        Self::Bridgehead,
    ];

    /// Data for the existing spawn, wallet and command consumers to apply later.
    /// A health share uses Source's ceil(full health * share), not direct HP assignment.
    pub fn entry(self) -> StepEntry {
        let (goal, at, manpower_floor, spawns) = match self {
            Self::Hedgerow => (
                "Move your squads up to the hedgerow",
                cell(13, 22),
                None,
                vec![],
            ),
            Self::Orchard => (
                "Clear the enemy sentries out of the orchard",
                cell(24, 22),
                None,
                vec![
                    spawn("rifle", 24, 14, Some(0.6), SpawnOrder::None),
                    spawn("rifle", 24, 29, Some(0.6), SpawnOrder::None),
                ],
            ),
            Self::Crossroads => (
                "Take the crossroads: keep a squad by the flag",
                cell(34, 22),
                None,
                vec![],
            ),
            Self::Recruit => (
                "Recruit an AT gun: Tab, W, E, or click its card",
                cell(34, 22),
                Some(250.0),
                vec![],
            ),
            Self::Tank => (
                "Knock out the tank with your AT gun",
                cell(44, 22),
                None,
                vec![spawn(
                    "tank",
                    53,
                    21,
                    None,
                    SpawnOrder::AttackMove(cell(41, 22)),
                )],
            ),
            Self::Trench => (
                "Clear the farm trench. Rifles: F throws a grenade",
                cell(45, 22),
                None,
                vec![
                    spawn("rifle", 45, 17, None, SpawnOrder::None),
                    spawn("rifle", 45, 26, None, SpawnOrder::None),
                ],
            ),
            Self::Retreat => (
                "Send a squad home to refill: select it and press R",
                cell(5, 22),
                None,
                vec![],
            ),
            Self::Artillery => (
                "Shell the stone house: press C, click it, aim, click",
                cell(63, 17),
                Some(200.0),
                vec![
                    spawn("mg", 63, 21, None, SpawnOrder::Garrison(cell(63, 17))),
                    spawn("rifle", 62, 21, None, SpawnOrder::Garrison(cell(63, 17))),
                ],
            ),
            Self::Bridgehead => (
                "Cross the bridge and take the bridgehead",
                cell(62, 22),
                None,
                vec![],
            ),
        };
        StepEntry {
            goal,
            at,
            manpower_floor,
            spawns,
        }
    }

    /// Match Source's checks over living team-zero actors, not just player slot zero.
    /// Reject malformed internal facts before using an owner as an index.
    pub fn complete(self, view: &CompletionView<'_>) -> Result<bool, &'static str> {
        let mut ids = std::collections::BTreeSet::new();
        if view
            .players
            .iter()
            .any(|p| !p.artillery_cooldown.is_finite())
            || view.units.iter().any(|u| {
                u.owner >= view.players.len()
                    || !u.hp.is_finite()
                    || !u.x.is_finite()
                    || !ids.insert(u.id)
            })
            || view
                .point_owners
                .iter()
                .flatten()
                .any(|&owner| owner >= view.players.len())
        {
            return Err("invalid Tutorial completion facts");
        }
        let mine = |u: &&TutorialUnit<'_>| u.hp > 0.0 && view.players[u.owner].team == 0;
        let held = |index: usize| -> Result<bool, &'static str> {
            Ok(view
                .point_owners
                .get(index)
                .ok_or("missing Tutorial point")?
                .is_some_and(|owner| view.players[owner].team == 0))
        };
        Ok(match self {
            Self::Hedgerow => view.units.iter().filter(mine).all(|u| u.x >= 12.0 * CELL),
            Self::Orchard | Self::Tank | Self::Trench => view
                .spawned
                .iter()
                .all(|id| !view.units.iter().any(|u| u.id == *id && u.hp > 0.0)),
            Self::Crossroads => held(0)?,
            Self::Recruit => view.units.iter().filter(mine).any(|u| u.kind == "at"),
            Self::Retreat => view.units.iter().filter(mine).any(|u| u.retreating),
            Self::Artillery => view
                .players
                .iter()
                .any(|p| p.team == 0 && p.artillery_cooldown > 0.0),
            Self::Bridgehead => held(1)?,
        })
    }
}
