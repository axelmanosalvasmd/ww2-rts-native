//! Tutorial constructor and late tick consumer over canonical Match owners.
//! Public room attachment remains restricted by the server's existing R1 admission.
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
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq)]
pub struct TutorialLine {
    pub text: &'static str,
    pub at: Point,
}
#[derive(Clone, Debug)]
pub struct TutorialRuntime {
    enemy: usize,
    step: usize,
    spawned: Vec<u32>,
    pending_lines: Vec<TutorialLine>,
}
impl TutorialRuntime {
    pub fn step(&self) -> TutorialStep {
        TutorialStep::ALL[self.step]
    }
    pub fn goal(&self) -> &'static str {
        self.step().entry().goal
    }
    pub fn spawned(&self) -> &[u32] {
        &self.spawned
    }
    pub fn enemy(&self) -> usize {
        self.enemy
    }
}

const LINES: [[&str; 2]; 9] = [
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
    /// Humans share team zero and the first authored spawn. The unattended enemy
    /// owns a real player slot, its own systems and the second authored spawn.
    /// This explicitly implements Tutorial startup; Match::new keeps its old gate.
    pub fn new_tutorial(
        map: &crate::map::AuthoritativeMap,
        mut options: MatchOptions,
    ) -> Result<Self, String> {
        let (w, h) = map.dimensions();
        if options.mode != Mode::Tutorial
            || !(1..=5).contains(&options.players.len())
            || map.spawns().len() < 2
            || w < 64
            || h < 30
            || map.geography().is_some()
            || !map.triggers().is_empty()
            || map.points().iter().filter(|p| p.mp > 0.0).count() < 2
        {
            return Err(
                "Tutorial requires 1-5 humans, two spawns and its authored objectives".into(),
            );
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
        options.shuffle = false;
        options.tech = false;
        options.supply = false;
        options.logistics = false;
        let rules = Rules::compiled().map_err(|e| e.to_string())?;
        let start = StartPlan::prepare_tutorial(map, &options, &rules)?;
        let mut state = Self::skirmish_shell(map, options, rules, start)?;
        state.tutorial = Some(TutorialRuntime {
            enemy,
            step: 0,
            spawned: Vec::new(),
            pending_lines: Vec::new(),
        });
        state.start.economy.players[enemy].mp = 0.0;
        state.complete_skirmish_start()?;
        state.begin_tutorial_step()?;
        state.validate_tutorial_state()?;
        Ok(state)
    }

    pub fn tutorial_state(&self) -> Option<&TutorialRuntime> {
        self.tutorial.as_ref()
    }

    pub(crate) fn validate_tutorial_state(&self) -> Result<(), String> {
        let runtime = self
            .tutorial
            .as_ref()
            .ok_or("missing canonical Tutorial state")?;
        if self.options.mode != Mode::Tutorial
            || self.world.game.mode != Some(ModeKind::Tutorial)
            || self.options.tech
            || self.options.supply
            || self.options.logistics
            || runtime.step >= TutorialStep::ALL.len()
            || runtime.enemy + 1 != self.start.players.len()
            || self.start.players.len() != self.world.game.players.len()
            || self.start.players.len() != self.start.economy.players.len()
            || self.start.players.len() != self.options.players.len()
            || self.start.points.len() < 2
            || runtime
                .spawned
                .iter()
                .copied()
                .collect::<BTreeSet<_>>()
                .len()
                != runtime.spawned.len()
        {
            return Err("invalid canonical Tutorial state".into());
        }
        for (slot, player) in self.start.players.iter().enumerate() {
            let team = if slot == runtime.enemy { 1 } else { 0 };
            if player.config.team != team
                || self.world.game.players[slot].team != team
                || self.start.economy.players[slot].team != team
                || self.options.players[slot].team != team
            {
                return Err("invalid Tutorial actor team".into());
            }
        }
        Ok(())
    }

    fn begin_tutorial_step(&mut self) -> Result<(), String> {
        self.validate_tutorial_state()?;
        let runtime = self.tutorial.as_ref().expect("validated Tutorial");
        let step = runtime.step;
        let owner = runtime.enemy;
        let entry = TutorialStep::ALL[step].entry();
        if let Some(floor) = entry.manpower_floor {
            for player in &mut self.start.economy.players {
                if player.team == 0 {
                    player.mp = math::max(player.mp, floor);
                }
            }
        }
        let runtime = self.tutorial.as_mut().expect("validated Tutorial");
        runtime.spawned.clear();
        runtime.pending_lines.extend(
            LINES[step]
                .iter()
                .map(|&text| TutorialLine { text, at: entry.at }),
        );
        for spawn in entry.spawns {
            let kind = self
                .rules
                .kind(spawn.kind)
                .ok_or("unknown Tutorial spawn kind")?;
            let id = self.next_unit_id;
            // Source temporarily shifts the enemy home so scatter index zero
            // resolves this authored cell, then restores the real home unchanged.
            let home = Point {
                x: spawn.at.x - 4.0,
                z: spawn.at.z,
            };
            let mut draft = StagedUnit::ordinary(
                &self.world.game,
                &self.rules,
                owner,
                kind,
                id,
                0,
                home,
                None,
            )
            .map_err(|e| format!("Tutorial spawn: {e:?}"))?;
            if let Some(share) = spawn.health_share {
                draft.unit.hp = (draft.unit.hp * share).ceil();
            }
            draft.control.auto_retreat = false;
            self.publish_starting_unit(draft)?;
            match spawn.order {
                SpawnOrder::None => {}
                SpawnOrder::AttackMove(at) => self.tutorial_move(owner, id, at, true)?,
                SpawnOrder::Garrison(at) => self.order_tutorial_house(owner, id, at)?,
            }
            self.tutorial
                .as_mut()
                .expect("Tutorial owner")
                .spawned
                .push(id);
        }
        Ok(())
    }

    fn tutorial_move(
        &mut self,
        owner: usize,
        id: u32,
        at: Point,
        attack_move: bool,
    ) -> Result<(), String> {
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
            return Err(format!("Tutorial scripted order refused: {denial:?}"));
        }
        Ok(())
    }

    fn order_tutorial_house(&mut self, owner: usize, id: u32, at: Point) -> Result<(), String> {
        let game = &self.world.game;
        let first = game
            .cell_of(at.x, at.z)
            .ok_or("Tutorial house outside map")?;
        if game.chars[first] != b'B' {
            return Err("Tutorial garrison needs an actual house".into());
        }
        let taken: BTreeSet<_> = game
            .units
            .values()
            .filter_map(|u| u.garrison)
            .chain(self.commands.units.values().filter_map(|u| u.enter))
            .collect();
        let from = game
            .units
            .get(&id)
            .ok_or("missing Tutorial garrison actor")?
            .at();
        let mut seen = BTreeSet::from([first]);
        let mut queue = vec![first];
        let mut edge = Vec::new();
        let mut index = 0;
        while index < queue.len() {
            let cell = queue[index];
            index += 1;
            let (x, y) = (cell % game.w, cell / game.w);
            let mut open = false;
            for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let (nx, ny) = (x as isize + dx, y as isize + dy);
                if nx < 0 || ny < 0 || nx >= game.w as isize || ny >= game.h as isize {
                    continue;
                }
                let next = ny as usize * game.w + nx as usize;
                if game.chars[next] == b'B' {
                    if seen.insert(next) {
                        queue.push(next);
                    }
                } else if game.flags[next] & MOVE == 0 {
                    open = true;
                }
            }
            if open && !taken.contains(&cell) {
                edge.push(cell);
            }
        }
        let center = |cell: usize| Point {
            x: (cell % game.w) as f64 * CELL + CELL / 2.0,
            z: (cell / game.w) as f64 * CELL + CELL / 2.0,
        };
        edge.sort_by(|a, b| {
            let (a, b) = (center(*a), center(*b));
            math::hypot(a.x - from.x, a.z - from.z)
                .total_cmp(&math::hypot(b.x - from.x, b.z - from.z))
        });
        let cell = *edge.first().ok_or("Tutorial house is full")?;
        let goal = center(cell);
        self.tutorial_move(owner, id, goal, false)?;
        self.commands
            .units
            .get_mut(&id)
            .ok_or("missing Tutorial house control")?
            .enter = Some(cell);
        Ok(())
    }

    pub(crate) fn apply_tutorial_house_entry(&mut self, id: u32) -> Result<(), String> {
        if self.options.mode != Mode::Tutorial {
            return Ok(());
        }
        let cell = self
            .commands
            .units
            .get(&id)
            .ok_or("missing Tutorial control")?
            .enter;
        let Some(cell) = cell else {
            return Ok(());
        };
        let game = &self.world.game;
        if game.chars.get(cell) != Some(&b'B') {
            self.commands
                .units
                .get_mut(&id)
                .expect("checked control")
                .enter = None;
            return Ok(());
        }
        let at = Point {
            x: (cell % game.w) as f64 * CELL + CELL / 2.0,
            z: (cell / game.w) as f64 * CELL + CELL / 2.0,
        };
        let unit = game.units.get(&id).ok_or("missing Tutorial house actor")?;
        if math::hypot(unit.x - at.x, unit.z - at.z) <= CELL * 1.6 {
            let unit = self.world.game.units.get_mut(&id).expect("checked actor");
            unit.x = at.x;
            unit.z = at.z;
            unit.garrison = Some(cell);
            unit.path.clear();
            unit.board = 0;
            self.commands
                .units
                .get_mut(&id)
                .expect("checked control")
                .enter = None;
            self.grid.update(id, unit);
        }
        Ok(())
    }

    /// Late Source phase after actual point capture/income and death callbacks.
    /// It never overwrites clock, unit poses, owners, HP or completion facts.
    pub(crate) fn step_tutorial(&mut self) -> Result<Vec<TutorialLine>, String> {
        self.validate_tutorial_state()?;
        let runtime = self.tutorial.as_mut().expect("validated Tutorial");
        let lines = if self.world.game.tick >= 40 {
            std::mem::take(&mut runtime.pending_lines)
        } else {
            Vec::new()
        };
        if self.finish.is_some() {
            return Ok(lines);
        }
        let players = self
            .start
            .players
            .iter()
            .zip(&self.start.economy.players)
            .map(|(start, player)| TutorialPlayer {
                team: player.team,
                artillery_cooldown: start.support.get("artillery").copied().unwrap_or(0.0),
            })
            .collect::<Vec<_>>();
        let units = self
            .world
            .game
            .units
            .values()
            .map(|unit| TutorialUnit {
                id: unit.id,
                owner: unit.owner,
                kind: self.rules.kinds[unit.kind].name.as_str(),
                hp: unit.hp,
                x: unit.x,
                retreating: unit.retreating,
            })
            .collect::<Vec<_>>();
        let owners = self
            .start
            .points
            .iter()
            .map(|p| p.owner)
            .collect::<Vec<_>>();
        let runtime = self.tutorial.as_ref().expect("Tutorial owner");
        if !runtime.step().complete(&CompletionView {
            players: &players,
            units: &units,
            point_owners: &owners,
            spawned: &runtime.spawned,
        })? {
            return Ok(lines);
        }
        if runtime.step + 1 == TutorialStep::ALL.len() {
            let prepared =
                self.prepare_match_finish(0, EndReason::Tutorial, Some(self.start.points[1].at))?;
            self.publish_match_finish(prepared);
        } else {
            self.tutorial.as_mut().expect("Tutorial owner").step += 1;
            self.begin_tutorial_step()?;
        }
        Ok(lines)
    }
}

#[cfg(test)]
#[path = "match_tutorial/tests.rs"]
mod tests;
