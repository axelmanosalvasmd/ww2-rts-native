//! Observed wreck display facts and private recipient snapshot lists.
//! Live hulls stay in Game and display/physics metadata stay in EnvironmentRuntime.
pub use crate::match_routing::{
    OrdinaryListPublication, OrdinaryListsCommit, PreparedOrdinaryLists,
    prepare_ordinary_recipient_lists,
};
use crate::{
    environment::{EnvironmentRuntime, WreckMetadata},
    environment_debris::DebrisRow,
    game::{Game, LOCAL_DEFENDER_OWNER, ModeKind, Wreck},
    math,
    rules::Rules,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WreckRow {
    pub id: u32,
    pub kind: String,
    pub owner: i64,
    pub x: f64,
    pub z: f64,
    pub rotation: f64,
    pub motion: Option<DebrisRow>,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct RememberedWreck {
    pub query: Wreck,
    pub display: Option<WreckRow>,
}
fn rounded(n: f64) -> f64 {
    math::round(n * 10.) / 10.
}
fn number(n: f64) -> Value {
    if n == 0. { json!(0) } else { json!(n) }
}
impl WreckRow {
    /// Existing reference list shape, not a new external contract.
    pub fn to_json(&self) -> Value {
        let mut row = vec![
            json!(self.id),
            json!(self.kind),
            json!(self.owner),
            number(self.x),
            number(self.z),
            number(self.rotation),
        ];
        if let Some(m) = &self.motion {
            row.push(json!({"id":m.id,"kind":m.kind.as_str(),"x":number(m.x),"y":number(m.y),"z":number(m.z),"dir":number(m.dir),"material":m.material,"mass":number(m.mass),"v":m.v.map(number),"gravity":number(m.gravity),"drag":number(m.drag),"time":number(m.time),"born":number(m.born),"tilt":number(m.tilt),"settled":m.settled}));
        }
        Value::Array(row)
    }
}
impl RememberedWreck {
    pub(crate) fn observe(
        game: &Game,
        rules: &Rules,
        hull: &Wreck,
        metadata: Option<&WreckMetadata>,
    ) -> Result<Self, String> {
        let id = hull
            .id
            .filter(|id| *id != 0)
            .ok_or("missing wreck identity")?;
        let query = Wreck {
            id: Some(id),
            cell: None,
            x: rounded(hull.x),
            z: rounded(hull.z),
            moving: hull.moving,
        };
        if ![query.x, query.z].iter().all(|n| n.is_finite()) {
            return Err("unrepresentable rounded wreck position".into());
        }
        let display = metadata
            .map(|m| {
                let kind = rules
                    .kinds
                    .get(m.kind)
                    .ok_or("invalid wreck display kind")?;
                let owner = if m.owner < game.players.len() {
                    m.owner as i64
                } else if game.mode == Some(ModeKind::World) && m.owner == LOCAL_DEFENDER_OWNER {
                    -1
                } else {
                    return Err("invalid wreck display owner");
                };
                if !m.rotation.is_finite() || !rounded(m.rotation).is_finite() {
                    return Err("invalid wreck rotation");
                }
                let motion = m
                    .motion
                    .as_ref()
                    .filter(|body| !body.settled)
                    .map(|body| body.row());
                if motion.as_ref().is_some_and(|b| {
                    ![
                        b.x, b.y, b.z, b.dir, b.mass, b.v[0], b.v[1], b.v[2], b.gravity, b.drag,
                        b.time, b.born, b.tilt,
                    ]
                    .iter()
                    .all(|n| n.is_finite())
                }) {
                    return Err("invalid wreck motion");
                }
                if hull.moving != motion.is_some() {
                    return Err("inconsistent wreck motion owner");
                }
                Ok(WreckRow {
                    id,
                    kind: kind.name.clone(),
                    owner,
                    x: query.x,
                    z: query.z,
                    rotation: rounded(m.rotation),
                    motion,
                })
            })
            .transpose()
            .map_err(str::to_owned)?;
        Ok(Self { query, display })
    }
    pub(crate) fn hide_motion(&mut self) {
        self.query.moving = false;
        if let Some(row) = &mut self.display {
            row.motion = None;
        }
    }
}

/// Additional canonical source facts required by prepared observation publication.
/// Hull positions and identities are already protected by World's source receipt.
#[derive(Clone)]
pub(crate) struct WreckSourceFrame(BTreeMap<u32, Option<WreckMetadata>>);
impl WreckSourceFrame {
    pub(crate) fn capture(game: &Game, environment: &EnvironmentRuntime) -> Self {
        Self(
            game.wrecks
                .iter()
                .filter_map(|w| w.id)
                .map(|id| (id, environment.wreck_metadata().get(&id).cloned()))
                .collect(),
        )
    }
    pub(crate) fn validate(&self, environment: &EnvironmentRuntime) -> Result<(), String> {
        if self
            .0
            .iter()
            .any(|(id, facts)| environment.wreck_metadata().get(id) != facts.as_ref())
        {
            return Err("wreck display source changed during observation preparation".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RecipientLists {
    pub wrecks: Vec<WreckRow>,
    pub found_mines: Vec<usize>,
    /// Absent in ordinary Conquest. World rows use the canonical ResourceNode owner.
    pub nodes: Option<Vec<[f64; 4]>>,
}

impl crate::match_runtime::Match {
    /// Capture list facts after current canonical state changes. No delivery cursor is drained.
    /// Ordinary wreck memory is per seat; World wreck memory is shared by team.
    pub fn recipient_snapshot_lists(&mut self, owner: usize) -> Result<RecipientLists, String> {
        let g = &self.world.game;
        let player = g.players.get(owner).ok_or("invalid list recipient")?;
        if !(0..6).contains(&player.team) {
            return Err("invalid list recipient team".into());
        }
        if self.world.mine_seen.keys().any(|c| *c >= g.chars.len()) {
            return Err("invalid found-mine cell".into());
        }
        let team = player.team;
        let found_mines = self
            .world
            .mine_seen
            .iter()
            .filter_map(|(&c, &bits)| ((bits >> team & 1) != 0 && g.chars[c] == b'N').then_some(c))
            .collect();
        match (self.options.mode, g.mode) {
            (crate::economy::Mode::Default | crate::economy::Mode::Conquest, None) => {
                let wrecks = self.observed_wreck_rows(owner)?;
                Ok(RecipientLists {
                    wrecks,
                    found_mines,
                    nodes: None,
                })
            }
            (crate::economy::Mode::World, Some(ModeKind::World)) => {
                let runtime = self
                    .world_runtime
                    .as_ref()
                    .ok_or("missing World list owner")?;
                if runtime.nodes.iter().any(|n| {
                    n.cell >= g.chars.len()
                        || ![n.at.x, n.at.z, n.rate, rounded(n.at.x), rounded(n.at.z)]
                            .iter()
                            .all(|v| v.is_finite())
                }) {
                    return Err("invalid World node display facts".into());
                }
                let wrecks = self.world_wreck_rows(owner)?;
                let runtime = self.world_runtime.as_ref().unwrap();
                let memory = runtime
                    .memory
                    .team(team)
                    .ok_or("missing World list memory")?;
                let nodes = runtime
                    .nodes
                    .iter()
                    .filter(|n| self.world.game.reveal || memory.contains_cell(n.cell))
                    .map(|n| {
                        [
                            rounded(n.at.x),
                            rounded(n.at.z),
                            n.rate,
                            f64::from(u8::from(n.fuel)),
                        ]
                    })
                    .collect();
                Ok(RecipientLists {
                    wrecks,
                    found_mines,
                    nodes: Some(nodes),
                })
            }
            _ => Err("recipient lists require represented ordinary or World state".into()),
        }
    }
}
impl RecipientLists {
    pub fn to_json(&self) -> Value {
        let mut value = json!({"wrecks":self.wrecks.iter().map(WreckRow::to_json).collect::<Vec<_>>(),"foundMines":self.found_mines});
        if let Some(nodes) = &self.nodes {
            value["nodes"] = json!(nodes.iter().map(|row| row.map(number)).collect::<Vec<_>>());
        }
        value
    }
}

pub(crate) fn updated_wreck_memory(
    prior: &[RememberedWreck],
    game: &Game,
    accepted: Vec<RememberedWreck>,
    removal_visible: impl Fn(crate::game::Point) -> bool,
    motion_visible: impl Fn(crate::game::Point) -> bool,
) -> Vec<RememberedWreck> {
    let mut rows = prior.to_vec();
    for row in accepted {
        if let Some(old) = rows.iter_mut().find(|w| w.query.id == row.query.id) {
            *old = row;
        } else {
            rows.push(row);
        }
    }
    rows.retain(|row| {
        game.wrecks.iter().any(|w| w.id == row.query.id)
            || !removal_visible(crate::game::Point {
                x: row.query.x,
                z: row.query.z,
            })
    });
    for row in &mut rows {
        if row.query.moving
            && !motion_visible(crate::game::Point {
                x: row.query.x,
                z: row.query.z,
            })
        {
            row.hide_motion();
        }
    }
    rows
}
pub(crate) fn display_rows(rows: &[RememberedWreck]) -> Result<Vec<WreckRow>, String> {
    rows.iter()
        .map(|row| {
            row.display.clone().ok_or_else(|| {
                format!(
                    "missing observed wreck display facts: {}",
                    row.query.id.unwrap_or(0)
                )
            })
        })
        .collect()
}

#[cfg(test)]
mod tests;

/// Canonical global lists used only to version an ordinary send cache.
/// Recipient messages still use their own remembered and filtered rows.
pub struct OrdinaryRawLists {
    pub wrecks: Value,
    pub nodes: Option<Value>,
}
impl crate::match_runtime::Match {
    pub fn ordinary_snapshot_raw_lists(&self) -> Result<OrdinaryRawLists, String> {
        let game = &self.world.game;
        let horde =
            self.options.mode == crate::economy::Mode::Horde && game.mode == Some(ModeKind::Horde);
        if horde {
            self.validate_actor_mode()?;
            self.validate_horde_economy()?;
        } else if !matches!(
            self.options.mode,
            crate::economy::Mode::Default | crate::economy::Mode::Conquest
        ) || game.mode.is_some()
        {
            return Err("global ordinary lists require Conquest or canonical Horde".into());
        }
        let mut ids = std::collections::BTreeSet::new();
        let mut rows = Vec::with_capacity(game.wrecks.len());
        for hull in &game.wrecks {
            let id = hull
                .id
                .filter(|id| *id != 0 && ids.insert(*id))
                .ok_or("invalid global wreck identity")?;
            let row = RememberedWreck::observe(
                game,
                &self.rules,
                hull,
                self.environment.wreck_metadata().get(&id),
            )?
            .display
            .ok_or("missing global wreck display facts")?;
            rows.push(row.to_json());
        }
        Ok(OrdinaryRawLists {
            wrecks: Value::Array(rows),
            nodes: None,
        })
    }
}
