//! The seven source story counters and end-result bookkeeping on canonical Match state.
//! No damage-total counter, transport message or server ending hold is defined here.

use crate::{
    catalog::{Catalog, Value},
    game::Point,
    match_runtime::Match,
    math,
};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PlayerStory {
    pub kills: f64,
    pub losses: f64,
    pub built: f64,
    pub captures: f64,
    pub mp_spent: f64,
    pub support_calls: f64,
    pub planes_downed: f64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoryKey {
    Kills,
    Losses,
    Built,
    Captures,
    MpSpent,
    SupportCalls,
    PlanesDowned,
}
impl StoryKey {
    pub const ALL: [Self; 7] = [
        Self::Kills,
        Self::Losses,
        Self::Built,
        Self::Captures,
        Self::MpSpent,
        Self::SupportCalls,
        Self::PlanesDowned,
    ];
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Kills => "kills",
            Self::Losses => "losses",
            Self::Built => "built",
            Self::Captures => "captures",
            Self::MpSpent => "mpSpent",
            Self::SupportCalls => "supportCalls",
            Self::PlanesDowned => "planesDowned",
        }
    }
}
impl PlayerStory {
    pub fn value(self, key: StoryKey) -> f64 {
        match key {
            StoryKey::Kills => self.kills,
            StoryKey::Losses => self.losses,
            StoryKey::Built => self.built,
            StoryKey::Captures => self.captures,
            StoryKey::MpSpent => self.mp_spent,
            StoryKey::SupportCalls => self.support_calls,
            StoryKey::PlanesDowned => self.planes_downed,
        }
    }
    fn field(&mut self, key: StoryKey) -> &mut f64 {
        match key {
            StoryKey::Kills => &mut self.kills,
            StoryKey::Losses => &mut self.losses,
            StoryKey::Built => &mut self.built,
            StoryKey::Captures => &mut self.captures,
            StoryKey::MpSpent => &mut self.mp_spent,
            StoryKey::SupportCalls => &mut self.support_calls,
            StoryKey::PlanesDowned => &mut self.planes_downed,
        }
    }
    pub fn validate(self) -> Result<(), String> {
        if StoryKey::ALL
            .into_iter()
            .all(|key| self.value(key).is_finite())
        {
            Ok(())
        } else {
            Err("nonfinite match story counter".into())
        }
    }
    pub fn rounded(self) -> Self {
        let mut row = self;
        for key in StoryKey::ALL {
            *row.field(key) = math::round(self.value(key));
        }
        row
    }
}
#[derive(Clone, Copy, Debug)]
pub struct StoryChange {
    pub slot: Option<usize>,
    pub key: StoryKey,
    pub amount: f64,
}
/// Prepared updates hold the exclusive canonical story borrow. Wallet or unit publication can
/// commit through other Match fields, then this guard publishes without another fallible step.
/// Dropping the guard leaves every counter unchanged. It is consumed once, with no dedup rule.
pub struct PreparedStoryUpdates<'a> {
    story: &'a mut [PlayerStory],
    updates: Vec<(usize, StoryKey, f64)>,
}
impl PreparedStoryUpdates<'_> {
    pub fn commit(self) {
        for (slot, key, value) in self.updates {
            *self.story[slot].field(key) = value;
        }
    }
}
pub fn prepare_story_updates<'a>(
    story: &'a mut [PlayerStory],
    winner_decided: bool,
    changes: &[StoryChange],
) -> Result<PreparedStoryUpdates<'a>, String> {
    for row in story.iter() {
        row.validate()?;
    }
    let mut updates = Vec::new();
    for change in changes {
        if !change.amount.is_finite() {
            return Err("nonfinite story change".into());
        }
        let Some(slot) = change.slot else {
            continue;
        };
        if slot >= story.len() {
            return Err("invalid story player slot".into());
        }
        if winner_decided {
            continue;
        }
        let before = updates
            .iter()
            .rev()
            .find(|(s, k, _)| *s == slot && *k == change.key)
            .map_or(story[slot].value(change.key), |(_, _, v)| *v);
        let value = before + change.amount;
        if !value.is_finite() {
            return Err("nonfinite story result".into());
        }
        updates.push((slot, change.key, value));
    }
    Ok(PreparedStoryUpdates { story, updates })
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FallenLocations {
    pub structure: Option<Point>,
    pub bunker: Option<Point>,
    pub hq: Option<Point>,
}
/// Select the retained location at the structural ending call site. This does not
/// decide which team survived, remove an army or admit a runtime mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StructureEnding {
    LastStructure,
    LastBunker,
    LastProductionBuilding,
}
impl FallenLocations {
    pub fn end_position(self, ending: StructureEnding) -> Option<Point> {
        match ending {
            StructureEnding::LastStructure => self.structure,
            StructureEnding::LastBunker => self.bunker,
            StructureEnding::LastProductionBuilding => self.hq,
        }
    }
    /// Mode endings consume their actual retained death location, including the
    /// map-center fallback. Source retains structure but no ending selects it.
    pub fn finish(
        self,
        mode: crate::economy::Mode,
        winner: i32,
        dimensions: (usize, usize),
        tick: i64,
    ) -> Result<crate::match_end::MatchFinish, String> {
        use crate::{economy::Mode, match_end::EndReason};
        let (ending, reason) = match mode {
            Mode::Classic => (
                StructureEnding::LastProductionBuilding,
                EndReason::Headquarters,
            ),
            Mode::Annihilation => (StructureEnding::LastBunker, EndReason::Bunkers),
            Mode::Assault | Mode::Horde => (StructureEnding::LastBunker, EndReason::Structures),
            _ => return Err("mode has no retained-structure ending".into()),
        };
        crate::match_end::finish_metadata(
            winner,
            reason,
            self.end_position(ending),
            dimensions,
            tick,
        )
    }
}

#[derive(Clone, Copy, Debug)]
pub struct DeathRecord<'a> {
    pub owner: Option<usize>,
    pub kind: &'a str,
    pub at: Point,
}
/// Internal completed Match report. Server display-name overrides and transport serialization
/// are separate consumers. These fields mirror source result facts without adding wire fields.
#[derive(Clone, Debug, PartialEq)]
pub struct MatchResult {
    pub winner: i32,
    pub teams: Vec<i32>,
    pub names: Vec<String>,
    pub reason: crate::match_end::EndReason,
    pub at: Point,
    pub end_tick: i64,
    pub story: Vec<PlayerStory>,
    pub timeline: Vec<crate::match_end::MatchSample>,
    pub win_vp: Option<f64>,
    pub mode: &'static str,
}
fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::Number(v) => *v != 0.0 && !v.is_nan(),
        Value::String(v) => !v.is_empty(),
        _ => true,
    }
}
impl Match {
    pub fn record_story_tally(
        &mut self,
        slot: Option<usize>,
        key: StoryKey,
        amount: f64,
    ) -> Result<(), String> {
        self.validate_match_finish_state()?;
        prepare_story_updates(
            &mut self.story,
            self.finish.is_some(),
            &[StoryChange { slot, key, amount }],
        )?
        .commit();
        Ok(())
    }
    /// Recruitment, construction, research, reinforcement and repair use their actual MP charge;
    /// cancellations pass the negative original charge. Income and kill bounties are not spending.
    pub fn record_spending(&mut self, slot: usize, amount: f64) -> Result<(), String> {
        self.record_story_tally(Some(slot), StoryKey::MpSpent, amount)
    }
    pub fn record_built(&mut self, slot: usize) -> Result<(), String> {
        self.record_story_tally(Some(slot), StoryKey::Built, 1.0)
    }
    pub fn record_plane_downed(&mut self, slot: Option<usize>) -> Result<(), String> {
        self.record_story_tally(slot, StoryKey::PlanesDowned, 1.0)
    }
    /// Always records the accepted off-map call. MP charges count as spending; a Munitions-only
    /// charge passes None. This does not buy support or consume physical provisions.
    pub fn record_support_call(
        &mut self,
        slot: usize,
        mp_charge: Option<f64>,
    ) -> Result<(), String> {
        self.validate_match_finish_state()?;
        let mut changes = vec![StoryChange {
            slot: Some(slot),
            key: StoryKey::SupportCalls,
            amount: 1.0,
        }];
        if let Some(amount) = mp_charge {
            changes.push(StoryChange {
                slot: Some(slot),
                key: StoryKey::MpSpent,
                amount,
            });
        }
        prepare_story_updates(&mut self.story, self.finish.is_some(), &changes)?.commit();
        Ok(())
    }
    /// Death cleanup calls this after removing the unit and applying wreck/bounty effects.
    /// The snapshot needs no live-unit lookup. Last-hit attribution is resolved against canonical
    /// teams: self/allied hits do not give kills. Missing owner/killer means the source's -1 slot.
    /// Cleanup owns exactly-once calls; this method deliberately does not invent deduplication.
    pub fn record_death(
        &mut self,
        unit: DeathRecord<'_>,
        last_hit: Option<usize>,
    ) -> Result<(), String> {
        self.validate_match_finish_state()?;
        let (changes, fallen) = self.death_story_plan(unit, last_hit)?;
        prepare_story_updates(&mut self.story, self.finish.is_some(), &changes)?.commit();
        self.fallen = fallen;
        Ok(())
    }
    /// Preflight the exact ordered death snapshots before publishing unit cleanup. It copies
    /// only the at-most-six story rows, never the world or Match. With exclusive Match ownership
    /// and no intervening story, roster or finish change, the same record_death calls cannot fail.
    /// The caller still emits each source event exactly once after removal/wreck/bounty effects.
    pub fn validate_story_for_deaths(
        &self,
        deaths: &[(DeathRecord<'_>, Option<usize>)],
    ) -> Result<(), String> {
        self.validate_match_finish_state()?;
        let mut changes = Vec::new();
        for &(unit, last_hit) in deaths {
            changes.extend(self.death_story_plan(unit, last_hit)?.0);
        }
        let mut rows = self.story.clone();
        prepare_story_updates(&mut rows, self.finish.is_some(), &changes)?.commit();
        Ok(())
    }
    fn death_story_plan(
        &self,
        unit: DeathRecord<'_>,
        last_hit: Option<usize>,
    ) -> Result<(Vec<StoryChange>, FallenLocations), String> {
        if !unit.at.x.is_finite() || !unit.at.z.is_finite() {
            return Err("invalid fallen unit position".into());
        }
        let players = &self.start.economy.players;
        if unit.owner.is_some_and(|slot| slot >= players.len())
            || last_hit.is_some_and(|slot| slot >= players.len())
        {
            return Err("invalid death story player".into());
        }
        let def = Catalog::compiled()
            .map_err(|e| e.to_string())?
            .unit(unit.kind)
            .map_err(|e| e.to_string())?;
        let killer = last_hit.filter(|&by| {
            !unit
                .owner
                .is_some_and(|owner| players[by].team == players[owner].team)
        });
        let changes = vec![
            StoryChange {
                slot: unit.owner,
                key: StoryKey::Losses,
                amount: 1.0,
            },
            StoryChange {
                slot: killer,
                key: StoryKey::Kills,
                amount: 1.0,
            },
        ];
        let mut fallen = self.fallen;
        if self.finish.is_none() && def.get("structure").is_some_and(truthy) {
            fallen.structure = Some(unit.at);
            if unit.kind == "bunker" {
                fallen.bunker = Some(unit.at);
            }
            if def.get("produces").is_some_and(truthy) {
                fallen.hq = Some(unit.at);
            }
        }
        Ok((changes, fallen))
    }
    /// Report only after canonical finish. Raw story counters remain unchanged when rounded.
    /// The owning server decides when and to whom this report may be delivered.
    pub fn conquest_result(&self) -> Result<Option<MatchResult>, String> {
        self.validate_conquest_finish_state()?;
        self.match_result()
    }
    /// Internal report for explicitly represented modes. Transport and ending hold stay with
    /// the server. Other mode names/reasons do not authorize their incomplete runtime.
    pub fn match_result(&self) -> Result<Option<MatchResult>, String> {
        self.validate_match_finish_state()?;
        let Some(finish) = self.finish else {
            return Ok(None);
        };
        Ok(Some(MatchResult {
            winner: finish.winner,
            teams: self.start.economy.players.iter().map(|p| p.team).collect(),
            names: self
                .start
                .players
                .iter()
                .map(|p| p.config.name.clone())
                .collect(),
            reason: finish.reason,
            at: finish.end_at,
            end_tick: finish.end_tick,
            story: self.story.iter().map(|row| row.rounded()).collect(),
            timeline: self.timeline.clone(),
            win_vp: if self.world.game.mode.is_none() {
                Some(self.start.win_vp)
            } else {
                None
            },
            mode: match self.options.mode {
                crate::economy::Mode::World => "world",
                crate::economy::Mode::Assault => "assault",
                crate::economy::Mode::Tutorial => "tutorial",
                _ => "conquest",
            },
        }))
    }
}
