//! Tutorial progression over the canonical Match units, objectives and wallets.
use crate::{
    economy::{Faction, Mode},
    game::{CELL, MOVE, ModeKind, Point},
    match_commands::{Command, MoveOrder},
    match_end::EndReason,
    match_runtime::{Match, MatchOptions, PlayerConfig, StartPlan},
    math,
    rules::Rules,
    tutorial_steps::{CompletionView, SpawnOrder, TutorialPlayer, TutorialStep, TutorialUnit},
    unit_publication::StagedUnit,
};
use std::collections::{BTreeSet, VecDeque};

/// Public dialogue and its map ping, returned by the tick that emits it.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TutorialLine {
    #[serde(deserialize_with = "crate::save_state::static_string")]
    pub text: crate::save_state::StaticStr,
    pub at: Point,
}

impl TutorialLine {
    pub fn shot_json(&self) -> serde_json::Value {
        serde_json::json!({"k":"say", "text":self.text, "x":self.at.x, "z":self.at.z, "pub":true})
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct TutorialRuntime {
    enemy: usize,
    index: usize,
    spawned: Vec<u32>,
    pending: Vec<TutorialLine>,
}
impl TutorialRuntime {
    pub fn enemy(&self) -> usize {
        self.enemy
    }
    pub fn step(&self) -> TutorialStep {
        TutorialStep::ALL[self.index]
    }
    pub fn goal(&self) -> &'static str {
        self.step().entry().goal
    }
    pub fn spawned(&self) -> &[u32] {
        &self.spawned
    }
}
pub(crate) const LINES: [[&str; 2]; 9] = [
    [
        "Sgt. Hollis: Morning, Lieutenant. The glider pilot missed the field by two miles, so we walk.",
        "Drag a box over the boys to select them, then right-click to move. Hedgerow first.",
    ],
    [
        "Two enemy sentry posts in the orchard. Right-click an enemy to attack it.",
        "Shell holes, hedges and woods halve the hits you take. Open fields get men killed.",
    ],
    [
        "Orchard is clear. Now the crossroads: park a squad by the flag until it turns our colour.",
        "Points pay manpower, and manpower buys men.",
    ],
    [
        "Bad news from the farm: an enemy tank is fuelling up. Rifles cannot scratch armour.",
        "Recruit an AT gun: press Tab, then W, then E. Or click its card at the bottom. HQ sent you the manpower.",
    ],
    [
        "Here it comes down the road! Set the AT gun where it can see the road and let the tank come to it.",
        "Keep the rifles out of its way. Hit it from the side or behind and it hurts twice as much.",
    ],
    [
        "They still hold the farm trench. Men in a trench shrug off bullets, not grenades.",
        "Select a rifle squad, press F and click the trench. Your MG can pin them down first.",
    ],
    [
        "Good work, but the boys are bleeding. Select a squad and press R: they run for home.",
        "Near the HQ a squad gets its men back, for manpower. A live veteran beats a dead hero.",
    ],
    [
        "Last stop, the bridge. The enemy holds the stone house beside it, and stone eats bullets.",
        "Call artillery: press C, click the house, move the mouse to aim the barrage and click again.",
    ],
    [
        "Shells away! When they land, push across and take the bridgehead.",
        "Anyone left in that house will fight for it. Finish them, then hold the flag.",
    ],
];

impl Match {
    /// Explicit core entry point. Public server admission is unchanged.
    pub fn new_tutorial(
        map: &crate::map::AuthoritativeMap,
        mut options: MatchOptions,
    ) -> Result<Self, String> {
        if options.mode != Mode::Tutorial || !(1..=6).contains(&options.players.len()) {
            return Err("Tutorial startup needs Tutorial mode and one through six players".into());
        }
        // Ordinary map validation requires two spawns. World maps remain guarded by new.
        if options.players.len() == 6 || map.spawns().len() < 2 {
            options.mode = Mode::Conquest;
            // createGame's feature whitelists still use the requested Tutorial mode.
            options.logistics = false;
            options.tech = false;
            return Self::new(map, options);
        }
        if map.points().iter().filter(|p| p.mp > 0.0).count() < 2
            || map.geography().is_some()
            || !map.triggers().is_empty()
        {
            return Err("Tutorial requires 1-5 players, two spawns and two objectives without scenario or World state".into());
        }
        let faction = [Faction::Germany, Faction::Ussr, Faction::Usa]
            .into_iter()
            .find(|f| options.players.iter().all(|p| p.faction != *f))
            .unwrap_or(Faction::Germany);
        for player in &mut options.players {
            player.team = 0;
        }
        let enemy = options.players.len();
        options.players.push(PlayerConfig {
            name: "Enemy".into(),
            team: 1,
            faction,
        });
        options.tech = false;
        options.supply = false;
        options.logistics = false;
        options.shuffle = false;
        let rules = Rules::compiled().map_err(|e| e.to_string())?;
        let start = StartPlan::prepare_tutorial(map, &options, &rules)?;
        let mut state = Self::skirmish_shell(map, options, rules, start)?;
        state.tutorial = Some(TutorialRuntime {
            enemy,
            index: 0,
            spawned: vec![],
            pending: vec![],
        });
        state.start.economy.players[enemy].mp = 0.0;
        state.complete_skirmish_start()?;
        state.begin_tutorial_step()?;
        Ok(state)
    }
    pub fn tutorial_state(&self) -> Option<&TutorialRuntime> {
        self.tutorial.as_ref()
    }

    pub(crate) fn validate_tutorial_state(&self) -> Result<(), String> {
        let state = self.tutorial.as_ref().ok_or("missing Tutorial runtime")?;
        let count = state.enemy + 1;
        if self.options.mode != Mode::Tutorial
            || self.world.game.mode != Some(ModeKind::Tutorial)
            || self.options.tech
            || self.options.supply
            || self.options.logistics
            || self.options.shuffle
            || !(2..=6).contains(&count)
            || state.index >= TutorialStep::ALL.len()
            || self.options.players.len() != count
            || self.start.players.len() != count
            || self.world.game.players.len() != count
            || self.start.economy.players.len() != count
            || self.start.points.len() < 2
            || state.spawned.iter().collect::<BTreeSet<_>>().len() != state.spawned.len()
        {
            return Err("invalid Tutorial runtime".into());
        }
        for slot in 0..count {
            let team = i32::from(slot == state.enemy);
            if self.options.players[slot].team != team
                || self.start.players[slot].config.team != team
                || self.world.game.players[slot].team != team
                || self.start.economy.players[slot].team != team
            {
                return Err("invalid Tutorial roster".into());
            }
        }
        Ok(())
    }
    fn begin_tutorial_step(&mut self) -> Result<(), String> {
        self.validate_tutorial_state()?;
        let runtime = self.tutorial.as_mut().unwrap();
        let entry = runtime.step().entry();
        let enemy = runtime.enemy;
        runtime.spawned.clear();
        runtime.pending.extend(
            LINES[runtime.index]
                .iter()
                .map(|&text| TutorialLine { text, at: entry.at }),
        );
        if let Some(floor) = entry.manpower_floor {
            for player in &mut self.start.economy.players {
                if player.team == 0 {
                    player.mp = math::max(player.mp, floor);
                }
            }
        }
        for spawn in entry.spawns {
            let kind = self
                .rules
                .kind(spawn.kind)
                .ok_or("missing Tutorial unit kind")?;
            // Scatter index zero is four metres east of home. Keep the real enemy home intact.
            let mut draft = StagedUnit::ordinary(
                &self.world.game,
                &self.rules,
                enemy,
                kind,
                self.next_unit_id,
                0,
                Point {
                    x: spawn.at.x - 4.0,
                    z: spawn.at.z,
                },
                None,
            )
            .map_err(|e| format!("Tutorial spawn: {e:?}"))?;
            let id = draft.unit.id;
            if let Some(share) = spawn.health_share {
                draft.unit.hp = (draft.unit.hp * share).ceil();
            }
            draft.control.auto_retreat = false;
            self.publish_starting_unit(draft)?;
            match spawn.order {
                SpawnOrder::None => {}
                SpawnOrder::AttackMove(at) => self.tutorial_order(id, at, true)?,
                SpawnOrder::Garrison(at) => self.tutorial_house_order(id, at)?,
            }
            self.tutorial.as_mut().unwrap().spawned.push(id);
        }
        Ok(())
    }
    fn tutorial_order(&mut self, id: u32, at: Point, attack_move: bool) -> Result<(), String> {
        let owner = self.world.game.units[&id].owner;
        let command = Command::Move {
            orders: vec![MoveOrder {
                unit: i64::from(id),
                at,
            }],
            control_ids: None,
            attack_move,
            queue: false,
            together: false,
            face: None,
        };
        if let Some(denial) = self.execute_command(owner, &command, self.finish.is_some())? {
            return Err(format!("Tutorial order refused: {denial:?}"));
        }
        Ok(())
    }
    fn tutorial_house_order(&mut self, id: u32, at: Point) -> Result<(), String> {
        let game = &self.world.game;
        let first = game
            .cell_of(at.x, at.z)
            .ok_or("Tutorial house outside map")?;
        if game.chars[first] != b'B' {
            return Err("Tutorial needs the authored stone house".into());
        }
        let occupied: BTreeSet<_> = game
            .units
            .values()
            .filter_map(|u| u.garrison)
            .chain(self.commands.units.values().filter_map(|u| u.enter))
            .collect();
        let mut seen = BTreeSet::from([first]);
        let mut queue = VecDeque::from([first]);
        let mut edges = vec![];
        while let Some(cell) = queue.pop_front() {
            let (x, z) = ((cell % game.w) as isize, (cell / game.w) as isize);
            let mut open = false;
            for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let (nx, nz) = (x + dx, z + dz);
                if nx < 0 || nz < 0 || nx >= game.w as isize || nz >= game.h as isize {
                    continue;
                }
                let next = nz as usize * game.w + nx as usize;
                if game.chars[next] == b'B' {
                    if seen.insert(next) {
                        queue.push_back(next);
                    }
                } else if game.flags[next] & MOVE == 0 {
                    open = true;
                }
            }
            if open && !occupied.contains(&cell) {
                edges.push(cell);
            }
        }
        let from = game.units[&id].at();
        crate::sort::sort_by(&mut edges, |a, b| {
            let a = tutorial_cell(game.w, a);
            let b = tutorial_cell(game.w, b);
            math::hypot(a.x - from.x, a.z - from.z) - math::hypot(b.x - from.x, b.z - from.z)
        });
        let cell = *edges.first().ok_or("Tutorial house has no free edge")?;
        self.tutorial_order(id, tutorial_cell(game.w, cell), false)?;
        self.commands.units.get_mut(&id).unwrap().enter = Some(cell);
        Ok(())
    }
    /// Consume the scripted house goal after movement, using ordinary routing to reach it.
    pub(crate) fn apply_tutorial_garrison(&mut self, id: u32) -> Result<(), String> {
        if self.tutorial.is_none() {
            return Ok(());
        }
        let Some(cell) = self.commands.units[&id].enter else {
            return Ok(());
        };
        if self.world.game.chars.get(cell) != Some(&b'B') {
            self.commands.units.get_mut(&id).unwrap().enter = None;
            return Ok(());
        }
        let at = tutorial_cell(self.world.game.w, cell);
        let unit = self
            .world
            .game
            .units
            .get_mut(&id)
            .ok_or("missing Tutorial house actor")?;
        if math::hypot(unit.x - at.x, unit.z - at.z) <= CELL * 1.6 {
            crate::unit_placement::place_unit(&mut self.grid, unit, |unit| {
                unit.x = at.x;
                unit.z = at.z;
                unit.garrison = Some(cell);
                unit.path.clear();
                unit.board = 0;
            });
            self.commands.units.get_mut(&id).unwrap().enter = None;
        }
        Ok(())
    }
    pub(crate) fn step_tutorial(&mut self) -> Result<Vec<TutorialLine>, String> {
        self.validate_tutorial_state()?;
        let runtime = self.tutorial.as_mut().unwrap();
        let lines = if self.world.game.tick >= 40 {
            std::mem::take(&mut runtime.pending)
        } else {
            vec![]
        };
        if self.finish.is_some() {
            return Ok(lines);
        }
        let players: Vec<_> = self
            .start
            .players
            .iter()
            .zip(&self.start.economy.players)
            .map(|(p, e)| TutorialPlayer {
                team: e.team,
                artillery_cooldown: p.support.get("artillery").copied().unwrap_or(0.0),
            })
            .collect();
        let units: Vec<_> = self
            .world
            .game
            .units
            .values()
            .map(|u| TutorialUnit {
                id: u.id,
                owner: u.owner,
                kind: &self.rules.kinds[u.kind].name,
                hp: u.hp,
                x: u.x,
                retreating: u.retreating,
            })
            .collect();
        let point_owners: Vec<_> = self.start.points.iter().map(|p| p.owner).collect();
        let runtime = self.tutorial.as_ref().unwrap();
        let done = runtime.step().complete(&CompletionView {
            players: &players,
            units: &units,
            point_owners: &point_owners,
            spawned: &runtime.spawned,
        })?;
        if done {
            if runtime.index + 1 == TutorialStep::ALL.len() {
                let finish = self.prepare_match_finish(
                    0,
                    EndReason::Tutorial,
                    Some(self.start.points[1].at),
                )?;
                self.publish_match_finish(finish);
            } else {
                self.tutorial.as_mut().unwrap().index += 1;
                self.begin_tutorial_step()?;
            }
        }
        Ok(lines)
    }
}
fn tutorial_cell(width: usize, cell: usize) -> Point {
    Point {
        x: (cell % width) as f64 * CELL + CELL / 2.0,
        z: (cell / width) as f64 * CELL + CELL / 2.0,
    }
}

#[cfg(test)]
mod tests;
