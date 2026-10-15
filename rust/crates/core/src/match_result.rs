//! Typed result composition and personal history. Storage, transport and room admission
//! remain server responsibilities; this module never admits a runtime mode.
use crate::{
    economy::Mode,
    economy_state::ArmySize,
    match_end::{EndReason, MatchSample},
    match_runtime::Match,
    match_story::{MatchResult, PlayerStory, StoryKey},
    math,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

// Source JSON serialization displays either signed zero as zero. Canonical raw
// state keeps its bits; only this explicit result representation uses display zero.
fn number(value: f64) -> Value {
    if value == 0.0 { json!(0) } else { json!(value) }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Victory,
    Defeat,
    Draw,
}
impl Outcome {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Victory => "victory",
            Self::Defeat => "defeat",
            Self::Draw => "draw",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LastMatch {
    pub outcome: Outcome,
    pub team: i32,
}
impl LastMatch {
    pub fn fields(self) -> Value {
        json!({"outcome":self.outcome.as_str(),"team":self.team})
    }
}
/// Kept with a person, not a current seat or the report's reordered roster.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PersonHistory {
    last_match: Option<LastMatch>,
}
impl PersonHistory {
    pub fn last_match(&self) -> Option<LastMatch> {
        self.last_match
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct HordeRun {
    pub wave: u64,
    pub kills: f64,
    pub time: f64,
    pub names: Vec<String>,
}
impl HordeRun {
    pub fn fields(&self) -> Value {
        json!({"wave":self.wave,"kills":number(self.kills),"time":number(self.time),"names":self.names})
    }
    /// Source sums raw human kills before the enemy slot. Duration is the current
    /// hold-completion tick, which can differ from the report's decision tick.
    pub fn from_counters(
        wave: u64,
        tick: i64,
        story: &[PlayerStory],
        names: &[String],
    ) -> Result<Self, String> {
        if !(1..=5).contains(&names.len())
            || story.len() != names.len() + 1
            || !(0..=9_007_199_254_740_991).contains(&tick)
        {
            return Err("Horde result requires human counters followed by one enemy".into());
        }
        let mut kills = 0.0;
        for row in &story[..names.len()] {
            row.validate()?;
            kills += row.kills;
        }
        let run = Self {
            wave,
            kills,
            time: math::round(tick as f64 * 0.05),
            names: names.to_vec(),
        };
        run.validate()?;
        Ok(run)
    }
    fn validate(&self) -> Result<(), String> {
        if self.wave > 9_007_199_254_740_991
            || !self.kills.is_finite()
            || !self.time.is_finite()
            || self.time < 0.0
            || self.time.fract() != 0.0
            || !(1..=5).contains(&self.names.len())
        {
            Err("invalid Horde record".into())
        } else {
            Ok(())
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct HordeResult {
    run: HordeRun,
    record: bool,
    best: Option<HordeRun>,
}
impl HordeResult {
    pub fn run(&self) -> &HordeRun {
        &self.run
    }
    pub fn record(&self) -> bool {
        self.record
    }
    /// The previous best, including when this run replaces it.
    pub fn best(&self) -> Option<&HordeRun> {
        self.best.as_ref()
    }
    pub fn fields(&self) -> Value {
        let mut fields = self.run.fields();
        fields["record"] = json!(self.record);
        fields["best"] = self.best.as_ref().map_or(Value::Null, HordeRun::fields);
        fields
    }
}
#[derive(Clone, Debug)]
pub struct HordeRecordKey {
    pub map: String,
    pub humans: usize,
    pub army: ArmySize,
}
impl HordeRecordKey {
    pub fn storage_key(&self) -> Result<String, String> {
        if !(1..=5).contains(&self.humans) || self.map.is_empty() {
            return Err("invalid Horde record key".into());
        }
        let army = match self.army {
            ArmySize::Standard => "standard",
            ArmySize::Large => "large",
            ArmySize::Massive => "massive",
            ArmySize::Endless => "endless",
        };
        Ok(format!("{}|{}|{}", self.map, self.humans, army))
    }
}
/// Ordered in-memory record owner. The server may persist an accepted update or
/// forward it to a shard owner; file failure does not roll back the cached record.
#[derive(Clone, Debug, Default)]
pub struct HordeRecords {
    records: BTreeMap<String, HordeRun>,
}
impl HordeRecords {
    /// Load typed saved records before room use. Malformed or duplicate keys do
    /// not publish a partially restored owner.
    pub fn restore(
        records: impl IntoIterator<Item = (HordeRecordKey, HordeRun)>,
    ) -> Result<Self, String> {
        let mut restored = Self::default();
        for (key, run) in records {
            run.validate()?;
            if key.humans != run.names.len() {
                return Err("saved Horde roster disagrees with its key".into());
            }
            if restored.records.insert(key.storage_key()?, run).is_some() {
                return Err("duplicate saved Horde record key".into());
            }
        }
        Ok(restored)
    }
    pub fn entries(&self) -> impl Iterator<Item = (&str, &HordeRun)> {
        self.records.iter().map(|(key, run)| (key.as_str(), run))
    }
    pub fn best(&self, key: &HordeRecordKey) -> Result<Option<&HordeRun>, String> {
        Ok(self.records.get(&key.storage_key()?))
    }
    pub fn finish(&mut self, key: &HordeRecordKey, run: HordeRun) -> Result<HordeResult, String> {
        run.validate()?;
        let key_text = key.storage_key()?;
        if run.names.len() != key.humans {
            return Err("Horde record roster disagrees with its key".into());
        }
        let best = self.records.get(&key_text).cloned();
        let record = best.as_ref().is_none_or(|best| {
            run.wave > best.wave || (run.wave == best.wave && run.time > best.time)
        });
        if record {
            self.records.insert(key_text, run.clone());
        }
        Ok(HordeResult { run, record, best })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CompletedReport {
    result: MatchResult,
    horde: Option<HordeResult>,
    human_count: usize,
}
impl CompletedReport {
    /// Names come from the stable human roster, with canonical names retained for
    /// unattended enemy slots. No result-shaped JSON or expected frame is accepted.
    pub fn compose(
        mut result: MatchResult,
        room_names: &[String],
        sandbox: bool,
        horde: Option<HordeResult>,
    ) -> Result<Self, String> {
        let count = result.teams.len();
        let mode = Mode::from_kind(Some(result.mode));
        if !(1..=6).contains(&count)
            || result.names.len() != count
            || result.story.len() != count
            || room_names.is_empty()
            || room_names.len() > count
            || result.teams.iter().any(|team| !(0..6).contains(team))
            || (result.winner != -1 && !result.teams.contains(&result.winner))
            || ((result.winner == -1) != (result.reason == EndReason::Draw))
            || !(0..=9_007_199_254_740_991).contains(&result.end_tick)
            || !result.at.x.is_finite()
            || !result.at.z.is_finite()
            || matches!(mode, Mode::Default | Mode::Other)
        {
            return Err("invalid completed report roster or finish".into());
        }
        if matches!(mode, Mode::Horde | Mode::Tutorial) {
            if count != room_names.len() + 1 || room_names.len() > 5 {
                return Err("shared-force report requires one unattended enemy".into());
            }
        } else if room_names.len() != count {
            return Err("completed report requires every human name".into());
        }
        if (mode == Mode::Conquest) != result.win_vp.is_some()
            || result.win_vp.is_some_and(|vp| !vp.is_finite())
            || (mode == Mode::Horde && !sandbox) != horde.is_some()
        {
            return Err("completed report has incompatible score or Horde branch".into());
        }
        if let Some(horde) = &horde
            && (horde.run.names != room_names
                || horde.run.time < math::round(result.end_tick as f64 * 0.05))
        {
            return Err("Horde run roster or duration disagrees with the report".into());
        }
        let teams = *result.teams.iter().max().expect("checked roster") as usize + 1;
        for row in &result.story {
            row.validate()?;
            if StoryKey::ALL
                .into_iter()
                .any(|key| row.value(key).fract() != 0.0)
            {
                return Err("completed story must be rounded by its producer".into());
            }
        }
        for sample in &result.timeline {
            let (t, arrays): (f64, &[Vec<f64>]) = match sample {
                MatchSample::Conquest { t, vp } if mode == Mode::Conquest => {
                    (*t, std::slice::from_ref(vp))
                }
                MatchSample::Territories {
                    t,
                    points,
                    structures,
                } if mode != Mode::Conquest => {
                    if points.len() != teams || points.iter().any(|v| !v.is_finite()) {
                        return Err("invalid result point timeline".into());
                    }
                    (*t, std::slice::from_ref(structures))
                }
                _ => return Err("result timeline has the wrong mode variant".into()),
            };
            if !t.is_finite()
                || t < 0.0
                || t.fract() != 0.0
                || arrays
                    .iter()
                    .any(|a| a.len() != teams || a.iter().any(|v| !v.is_finite()))
            {
                return Err("invalid result timeline values".into());
            }
        }
        for (name, room_name) in result.names.iter_mut().zip(room_names) {
            name.clone_from(room_name);
        }
        Ok(Self {
            result,
            horde,
            human_count: room_names.len(),
        })
    }
    pub fn result(&self) -> &MatchResult {
        &self.result
    }
    pub fn horde(&self) -> Option<&HordeResult> {
        self.horde.as_ref()
    }
    /// Full internal result representation. Room admission, delivery timing and
    /// socket serialization remain outside this component.
    pub fn fields(&self) -> Value {
        let r = &self.result;
        let story = r
            .story
            .iter()
            .map(|row| {
                StoryKey::ALL
                    .into_iter()
                    .map(|key| (key.as_str().to_owned(), number(row.value(key))))
                    .collect::<serde_json::Map<String, Value>>()
            })
            .collect::<Vec<_>>();
        let timeline = r
            .timeline
            .iter()
            .map(|sample| match sample {
                MatchSample::Conquest { t, vp } => {
                    json!({"t":number(*t),"vp":vp.iter().copied().map(number).collect::<Vec<_>>()})
                }
                MatchSample::Territories {
                    t,
                    points,
                    structures,
                } => json!({"t":number(*t),
                "points":points.iter().copied().map(number).collect::<Vec<_>>(),
                "structures":structures.iter().copied().map(number).collect::<Vec<_>>()}),
            })
            .collect::<Vec<_>>();
        let mut fields = json!({"winner":r.winner,"teams":r.teams,"names":r.names,
            "reason":r.reason.as_str(),"at":{"x":number(r.at.x),"z":number(r.at.z)},
            "endTick":r.end_tick,"story":story,"timeline":timeline,
            "winVp":r.win_vp.map(number),"mode":r.mode});
        if let Some(horde) = &self.horde {
            fields["horde"] = horde.fields();
        }
        fields
    }
    /// Preflight the complete stable roster before publishing any personal outcome.
    pub fn remember(&self, people: &mut [PersonHistory]) -> Result<(), String> {
        if people.len() != self.human_count {
            return Err("personal result roster disagrees with completed report".into());
        }
        for (person, team) in people.iter_mut().zip(&self.result.teams) {
            person.last_match = Some(LastMatch {
                team: *team,
                outcome: if self.result.winner == -1 {
                    Outcome::Draw
                } else if self.result.winner == *team {
                    Outcome::Victory
                } else {
                    Outcome::Defeat
                },
            });
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq)]
pub enum RoomReport {
    HostEnded,
    Completed(Box<CompletedReport>),
}
impl RoomReport {
    pub fn fields(&self) -> Value {
        match self {
            Self::HostEnded => json!({"ended":true}),
            Self::Completed(report) => report.fields(),
        }
    }
    pub fn recipient_fields(&self, person: &PersonHistory, seated: bool) -> Value {
        let mut fields = self.fields();
        fields["you"] = self
            .personal_outcome(person, seated)
            .map_or(Value::Null, LastMatch::fields);
        fields
    }
    /// The current seat only establishes eligibility. The outcome belongs to the
    /// person, even after seats and report rows have been removed or refilled.
    pub fn personal_outcome(&self, person: &PersonHistory, seated: bool) -> Option<LastMatch> {
        match self {
            Self::Completed(_) if seated => person.last_match,
            _ => None,
        }
    }
}
impl Match {
    /// Canonical represented modes produce the typed room component directly.
    /// Horde requires its own runtime facts and is not admitted by this adapter.
    pub fn completed_report(
        &self,
        room_names: &[String],
    ) -> Result<Option<CompletedReport>, String> {
        self.match_result()?
            .map(|result| CompletedReport::compose(result, room_names, self.options.sandbox, None))
            .transpose()
    }
}

#[cfg(test)]
mod tests;
