//! Canonical finish and timeline for represented Conquest, Assault, elimination and World phases.
use crate::{
    economy::Mode,
    game::{CELL, LOCAL_DEFENDER_OWNER, ModeKind, Point},
    match_runtime::Match,
    math,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EndReason {
    VictoryPoints,
    Headquarters,
    Bunkers,
    Structures,
    Timer,
    Draw,
    World,
    Nations,
    Tutorial,
}
impl EndReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::VictoryPoints => "vp",
            Self::Headquarters => "hq",
            Self::Bunkers => "bunkers",
            Self::Structures => "structures",
            Self::Timer => "timer",
            Self::Draw => "draw",
            Self::World => "world",
            Self::Nations => "nations",
            Self::Tutorial => "tutorial",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MatchFinish {
    pub winner: i32,
    pub reason: EndReason,
    pub end_at: Point,
    pub end_tick: i64,
}
/// Source `story.sample`: Conquest scores differ from other modes' objective/building counts.
/// Team ID indexes each array, including zero-filled gaps. No transport tag is defined here.
#[derive(Clone, Debug, PartialEq)]
pub enum MatchSample {
    Conquest {
        t: f64,
        vp: Vec<f64>,
    },
    Territories {
        t: f64,
        points: Vec<f64>,
        structures: Vec<f64>,
    },
}
impl Match {
    /// Early source sample at tick 1 and every 200 ticks. Caller owns exactly one invocation
    /// at that phase; no clock advance or duplicate suppression is invented here.
    pub fn sample_match_tick(&mut self) -> Result<bool, String> {
        self.validate_story_mode()?;
        if self.finish.is_some() {
            return Ok(false);
        }
        self.validate_match_finish_state()?;
        let tick = self.world.game.tick;
        if tick == 1 || tick % 200 == 0 {
            self.timeline.push(self.match_sample()?);
            Ok(true)
        } else {
            Ok(false)
        }
    }
    pub(crate) fn validate_story_mode(&self) -> Result<(), String> {
        match (self.options.mode, self.world.game.mode) {
            (Mode::Default | Mode::Conquest, None)
            | (Mode::Assault, Some(ModeKind::Assault))
            | (Mode::Annihilation, Some(ModeKind::Annihilation))
            | (Mode::Classic, Some(ModeKind::Classic))
            | (Mode::World, Some(ModeKind::World)) => Ok(()),
            _ => Err("finish/story supports represented Conquest, Assault and World only".into()),
        }
    }
    pub(crate) fn validate_match_finish_state(&self) -> Result<(), String> {
        self.validate_story_mode()?;
        let players = &self.start.economy.players;
        if !(1..=6).contains(&players.len())
            || self.start.players.len() != players.len()
            || self.options.players.len() != players.len()
            || self.world.game.players.len() != players.len()
            || self.story.len() != players.len()
            || self.capture_ticks.len() != self.start.points.len()
            || !(0..=9_007_199_254_740_991).contains(&self.world.game.tick)
            || self.world.game.w == 0
            || self.world.game.h == 0
            || self.start.dimensions != (self.world.game.w, self.world.game.h)
        {
            return Err("invalid Match finish state".into());
        }
        for (index, p) in players.iter().enumerate() {
            if !(0..6).contains(&p.team)
                || p.team != self.start.players[index].config.team
                || self.world.game.players[index].slot != index
                || p.team != self.world.game.players[index].team
                || p.team != self.options.players[index].team
            {
                return Err("invalid Match player identity".into());
            }
        }
        if self.world.game.mode.is_none() {
            if !self.start.win_vp.is_finite() || self.start.win_vp <= 0.0 {
                return Err("invalid Conquest goal".into());
            }
            for team in players.iter().map(|p| p.team) {
                let mut sum = 0.0;
                for p in players {
                    sum += if p.team == team { p.vp } else { 0.0 };
                }
                if !sum.is_finite() || players.iter().any(|p| !p.vp.is_finite()) {
                    return Err("nonfinite Conquest score".into());
                }
            }
        }
        for row in &self.story {
            row.validate()?;
        }
        if self
            .capture_ticks
            .iter()
            .flatten()
            .any(|tick| !(-9_007_199_254_740_991..=9_007_199_254_740_991).contains(tick))
        {
            return Err("invalid Match capture tick".into());
        }
        for point in &self.start.points {
            if point.owner.is_some_and(|owner| owner >= players.len())
                || !point.at.x.is_finite()
                || !point.at.z.is_finite()
            {
                return Err("invalid Match objective owner or position".into());
            }
        }
        if self.options.mode == Mode::Assault {
            let state = self.assault.ok_or("missing canonical Assault state")?;
            if !(0..6).contains(&state.defender_team)
                || !(-1..6).contains(&state.attacker_team)
                || state.attacker_team == state.defender_team
                || !state.time_left.is_finite()
                || players
                    .iter()
                    .any(|p| p.team != state.defender_team && p.team != state.attacker_team)
                || (state.attacker_team >= 0
                    && !players.iter().any(|p| p.team == state.attacker_team))
            {
                return Err("invalid Assault finish state".into());
            }
        } else if self.assault.is_some() {
            return Err("unexpected Assault state in Conquest".into());
        }
        if self.options.mode == Mode::World {
            let regions = self.world_economy_regions()?;
            let runtime = self.world_runtime.as_ref().ok_or("missing World runtime")?;
            if regions.len()
                != self
                    .world
                    .geography()
                    .ok_or("missing World geography")?
                    .size
                    .region_count()
                || runtime.regions.iter().any(|r| {
                    !(-1..6).contains(&r.team)
                        || !r.progress.is_finite()
                        || !(0.0..=1.0).contains(&r.progress)
                        || r.capper.is_some_and(|slot| slot >= players.len())
                        || r.cap_tick.is_some_and(|tick| {
                            !(-9_007_199_254_740_991..=9_007_199_254_740_991).contains(&tick)
                        })
                })
            {
                return Err("invalid canonical World finish regions".into());
            }
        }
        for at in [self.fallen.structure, self.fallen.bunker, self.fallen.hq]
            .into_iter()
            .flatten()
        {
            if !at.x.is_finite() || !at.z.is_finite() {
                return Err("invalid fallen position".into());
            }
        }
        let n = players.iter().map(|p| p.team).max().unwrap_or(0) as usize + 1;
        let valid = |values: &[f64]| values.len() == n && values.iter().all(|v| v.is_finite());
        for sample in &self.timeline {
            let ok = match sample {
                MatchSample::Conquest { t, vp } => {
                    self.world.game.mode.is_none() && t.is_finite() && valid(vp)
                }
                MatchSample::Territories {
                    t,
                    points,
                    structures,
                } => {
                    self.world.game.mode.is_some()
                        && t.is_finite()
                        && valid(points)
                        && valid(structures)
                }
            };
            if !ok {
                return Err("invalid Match timeline".into());
            }
        }
        if let Some(finish) = self.finish
            && (!(-1..6).contains(&finish.winner)
                || (finish.winner >= 0 && !players.iter().any(|p| p.team == finish.winner))
                || !finish.end_at.x.is_finite()
                || !finish.end_at.z.is_finite()
                || !(0..=9_007_199_254_740_991).contains(&finish.end_tick)
                || ((finish.winner == -1) != (finish.reason == EndReason::Draw)))
        {
            return Err("invalid Match report finish".into());
        }
        Ok(())
    }
    pub(crate) fn validate_sample_units(&self) -> Result<(), String> {
        let players = &self.start.economy.players;
        for unit in self.world.game.units.values() {
            if unit.kind >= self.rules.kinds.len()
                || !unit.hp.is_finite()
                || (unit.owner >= players.len()
                    && !(self.options.mode == Mode::World && unit.owner == LOCAL_DEFENDER_OWNER))
            {
                return Err("invalid Match sample unit".into());
            }
        }
        Ok(())
    }
    pub(crate) fn match_sample(&self) -> Result<MatchSample, String> {
        if self.world.game.mode.is_some() {
            self.validate_sample_units()?;
        }
        self.match_sample_with_units(
            self.world
                .game
                .units
                .values()
                .map(|u| (u.owner, u.kind, u.hp)),
        )
    }
    /// A staged World wreck may change surviving HP before its final sample is published.
    /// This is the same source sampler, using checked post-effect unit facts.
    pub(crate) fn match_sample_with_units(
        &self,
        units: impl IntoIterator<Item = (usize, usize, f64)>,
    ) -> Result<MatchSample, String> {
        let players = &self.start.economy.players;
        let n = players
            .iter()
            .map(|p| p.team)
            .max()
            .ok_or("sample needs players")? as usize
            + 1;
        let t = math::round(self.world.game.tick as f64 / 20.0);
        if self.world.game.mode.is_none() {
            let mut vp = vec![0.0; n];
            for p in players {
                vp[p.team as usize] += p.vp;
                if !vp[p.team as usize].is_finite() {
                    return Err("nonfinite Conquest sample score".into());
                }
            }
            for value in &mut vp {
                *value = value.floor();
            }
            Ok(MatchSample::Conquest { t, vp })
        } else {
            let mut points = vec![0.0; n];
            let mut structures = vec![0.0; n];
            for p in &self.start.points {
                if let Some(owner) = p.owner {
                    points[players[owner].team as usize] += 1.0;
                }
            }
            for (owner, kind, hp) in units {
                if kind >= self.rules.kinds.len()
                    || !hp.is_finite()
                    || (owner >= players.len()
                        && !(self.options.mode == Mode::World && owner == LOCAL_DEFENDER_OWNER))
                {
                    return Err("invalid Match sample unit".into());
                }
                if hp > 0.0
                    && self.rules.kinds[kind].structure
                    && (matches!(self.options.mode, Mode::World | Mode::Classic)
                        || self.rules.kinds[kind].name == "bunker")
                    && let Some(owner) = players.get(owner)
                {
                    structures[owner.team as usize] += 1.0;
                }
            }
            Ok(MatchSample::Territories {
                t,
                points,
                structures,
            })
        }
    }
    /// Prepare all fallible finish work before an actuator changes its timer or other state.
    /// The caller publishes this pair immediately under exclusive Match ownership.
    pub(crate) fn prepare_match_finish(
        &self,
        winner: i32,
        reason: EndReason,
        at: Option<Point>,
    ) -> Result<(MatchFinish, MatchSample), String> {
        self.prepare_match_finish_using(winner, reason, at, || self.match_sample())
    }
    pub(crate) fn prepare_match_finish_using(
        &self,
        winner: i32,
        reason: EndReason,
        at: Option<Point>,
        sample: impl FnOnce() -> Result<MatchSample, String>,
    ) -> Result<(MatchFinish, MatchSample), String> {
        if winner != -1 && !self.start.economy.players.iter().any(|p| p.team == winner) {
            return Err("unknown winning team".into());
        }
        let at = at.unwrap_or(Point {
            x: self.world.game.w as f64 * CELL / 2.0,
            z: self.world.game.h as f64 * CELL / 2.0,
        });
        let end_at = Point {
            x: math::round(at.x * 10.0) / 10.0,
            z: math::round(at.z * 10.0) / 10.0,
        };
        if !end_at.x.is_finite() || !end_at.z.is_finite() {
            return Err("invalid Match end position".into());
        }
        Ok((
            MatchFinish {
                winner,
                reason: if winner == -1 {
                    EndReason::Draw
                } else {
                    reason
                },
                end_at,
                end_tick: self.world.game.tick,
            },
            sample()?,
        ))
    }
    pub(crate) fn publish_match_finish(
        &mut self,
        prepared: (MatchFinish, MatchSample),
    ) -> MatchFinish {
        self.timeline.push(prepared.1);
        self.finish = Some(prepared.0);
        self.world.game.reveal = true;
        prepared.0
    }
}
