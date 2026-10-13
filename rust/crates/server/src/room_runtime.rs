//! One trusted host runtime over the canonical Match and recipient producers.
//! Public network start and complete gameplay admission are separate work.
use crate::{
    snapshot_lists::{ListScope, ListVersions, RoomListCache},
    tick_timing::{TickMeter, TickSample},
};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use ww2_core::{
    match_runtime::Match,
    match_story::MatchResult,
    match_tick::{TickEvent, TickFailure, TickReport},
    recipient_snapshot::{DeliveryState, SelectedSeat},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameKind {
    Scheduled,
    Current,
}
#[derive(Debug)]
pub struct CapturedSeat {
    pub slot: usize,
    pub batch: u64,
    pub message: Value,
    pub list_versions: ListVersions,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PauseReason {
    Host,
    Drop,
}
struct Pause {
    slot: usize,
    by: String,
    reason: PauseReason,
    until_ms: Option<u64>,
}
pub struct PausePoll {
    pub blocked: bool,
    pub send_frame: bool,
    pub message: Option<Value>,
}
pub struct EndingAction {
    pub advance: bool,
    pub complete: bool,
}

/// A caller-owned output batch retains real tick effects until a scheduled send.
/// It is not another simulation state or producer event authority.
pub struct RoomRuntime {
    state: Match,
    delivery: DeliveryState,
    lists: RoomListCache,
    meter: TickMeter,
    batch: u64,
    last_versions: Option<(u64, ListVersions)>,
    pending_events: Vec<TickEvent>,
    tick_failure: Option<TickFailure>,
    fault: Option<String>,
    pause: Option<Pause>,
    pause_sent_at: u64,
    auto_paused: BTreeSet<usize>,
    held: u16,
}
impl RoomRuntime {
    pub fn new(state: Match, now_ms: u64) -> Result<Self, String> {
        state.validate_tick_state()?;
        state
            .preflight_ordinary_snapshot()
            .map_err(|e| format!("snapshot admission: {e:?}"))?;
        state.ordinary_snapshot_raw_lists()?;
        let delivery =
            DeliveryState::new(&state).map_err(|e| format!("delivery admission: {e:?}"))?;
        let meter = TickMeter::new(now_ms).map_err(|e| e.to_string())?;
        Ok(Self {
            state,
            delivery,
            lists: RoomListCache::default(),
            meter,
            batch: 0,
            last_versions: None,
            pending_events: Vec::new(),
            tick_failure: None,
            fault: None,
            pause: None,
            pause_sent_at: now_ms,
            auto_paused: BTreeSet::new(),
            held: 0,
        })
    }
    pub fn state(&self) -> &Match {
        &self.state
    }
    /// Only a trusted host may admit recorded commands or author setup through this borrow.
    pub fn state_mut(&mut self) -> &mut Match {
        &mut self.state
    }
    pub fn fault(&self) -> Option<&str> {
        self.fault.as_deref()
    }
    /// A poisoned tick retains its committed event and retirement prefix exactly once.
    pub fn tick_failure(&self) -> Option<&TickFailure> {
        self.tick_failure.as_ref()
    }
    pub fn fault_stop(&mut self, reason: String) {
        if self.fault.is_none() {
            self.fault = Some(reason);
        }
    }
    fn running(&self) -> Result<(), String> {
        if let Some(reason) = self.fault() {
            Err(format!("native room faulted: {reason}"))
        } else {
            Ok(())
        }
    }
    pub fn set_away(&mut self, away: &[bool]) -> Result<(), String> {
        if away.len() != self.state.world.game.players.len()
            || away.len() != self.state.start.economy.players.len()
        {
            return Err("native room seat count changed".into());
        }
        for (player, away) in self.state.start.economy.players.iter_mut().zip(away) {
            player.away = *away;
        }
        Ok(())
    }
    pub fn advance_tick(&mut self, away: &[bool]) -> Result<TickReport, String> {
        self.running()?;
        self.set_away(away)?;
        match self.state.advance_tick() {
            Ok(mut report) => {
                self.pending_events.append(&mut report.events);
                Ok(report)
            }
            Err(failure) => {
                let reason = failure.reason.clone();
                self.tick_failure = Some(failure);
                self.fault_stop(reason.clone());
                Err(reason)
            }
        }
    }
    pub fn should_snapshot(&self, tick: i64) -> bool {
        tick >= 0
            && self
                .meter
                .should_snapshot(tick as u64, self.state.finish.is_some())
    }
    pub fn record_timing(&mut self, sample: TickSample, now_ms: u64) -> Result<u8, String> {
        self.meter.record(sample, now_ms).map_err(|e| e.to_string())
    }
    pub fn list_scope(&self, batch: u64) -> Result<ListScope, String> {
        self.last_versions
            .filter(|(known, _)| *known == batch)
            .map(|(_, versions)| ListScope::Shared(versions))
            .ok_or_else(|| "unknown native snapshot batch".into())
    }
    pub fn collect_frame(
        &mut self,
        selected: &[SelectedSeat],
        kind: FrameKind,
    ) -> Result<Vec<CapturedSeat>, String> {
        self.running()?;
        if selected.is_empty() {
            if kind == FrameKind::Scheduled {
                self.pending_events.clear();
            }
            return Ok(Vec::new());
        }
        let batch = self
            .batch
            .checked_add(1)
            .ok_or("native snapshot batch exhausted")?;
        let raw = self.state.ordinary_snapshot_raw_lists()?;
        let mut lists = self.lists.clone();
        let versions = lists
            .capture(batch, Some(&raw.wrecks), raw.nodes.as_ref())
            .map_err(|e| e.to_string())?;
        let events = if kind == FrameKind::Scheduled {
            self.pending_events.as_slice()
        } else {
            &[]
        };
        let frame = self
            .delivery
            .capture_selected_with_tick_events(&mut self.state, selected, events)
            .map_err(|e| format!("native snapshot: {e:?}"))?
            .ok_or("selected native frame unexpectedly absent")?;
        let mut captured = Vec::with_capacity(selected.len());
        for seat in selected {
            captured.push(CapturedSeat {
                slot: seat.slot,
                batch,
                list_versions: versions,
                message: frame
                    .recipient(seat.slot)
                    .map_err(|e| format!("native recipient: {e:?}"))?
                    .into_json(),
            });
        }
        drop(frame);
        self.lists = lists;
        self.batch = batch;
        self.last_versions = Some((batch, versions));
        if kind == FrameKind::Scheduled {
            self.pending_events.clear();
        }
        Ok(captured)
    }
    pub fn paused(&self) -> bool {
        self.pause.is_some()
    }
    pub fn pause_message(&self, now_ms: u64) -> Value {
        if let Some(pause) = &self.pause {
            json!({"t":"pause", "paused":true, "by":pause.by,
                "reason":if pause.reason == PauseReason::Drop { "drop" } else { "host" },
                "left":pause.until_ms.map_or(0, |until| until.saturating_sub(now_ms).div_ceil(1000))})
        } else {
            json!({"t":"pause", "paused":false, "by":"", "reason":"host", "left":0})
        }
    }
    pub fn pause(
        &mut self,
        slot: usize,
        by: &str,
        reason: PauseReason,
        now_ms: u64,
    ) -> Result<Option<Value>, String> {
        self.running()?;
        if slot >= self.state.world.game.players.len() {
            return Err("invalid native pause seat".into());
        }
        if self.pause.is_some() {
            return Ok(None);
        }
        let until_ms = if reason == PauseReason::Drop {
            Some(
                now_ms
                    .checked_add(30_000)
                    .ok_or("native pause deadline exhausted")?,
            )
        } else {
            None
        };
        self.pause = Some(Pause {
            slot,
            by: by.to_owned(),
            reason,
            until_ms,
        });
        self.pause_sent_at = now_ms;
        Ok(Some(self.pause_message(now_ms)))
    }
    pub fn auto_pause(
        &mut self,
        slot: usize,
        by: &str,
        now_ms: u64,
    ) -> Result<Option<Value>, String> {
        if self.pause.is_some() || self.auto_paused.contains(&slot) {
            return Ok(None);
        }
        let message = self.pause(slot, by, PauseReason::Drop, now_ms)?;
        if message.is_some() {
            self.auto_paused.insert(slot);
        }
        Ok(message)
    }
    pub fn resume(&mut self, now_ms: u64) -> Value {
        self.pause = None;
        self.pause_message(now_ms)
    }
    /// A replacement connection resumes only the drop pause owned by its stable seat.
    pub fn resume_drop(&mut self, slot: usize, now_ms: u64) -> Result<Option<Value>, String> {
        self.running()?;
        if slot >= self.state.world.game.players.len() {
            return Err("invalid native resume seat".into());
        }
        let same_drop = self
            .pause
            .as_ref()
            .is_some_and(|pause| pause.reason == PauseReason::Drop && pause.slot == slot);
        Ok(same_drop.then(|| self.resume(now_ms)))
    }

    /// Freeze the canonical report after the final hold step, before releasing the runtime.
    pub fn finished_result(&self) -> Result<MatchResult, String> {
        self.running()?;
        self.state
            .match_result()?
            .ok_or_else(|| "native result requires a finished match".into())
    }

    /// connected includes an AI takeover, matching the Source drop-resume gate.
    pub fn poll_pause(&mut self, connected: &[bool], now_ms: u64) -> Result<PausePoll, String> {
        if connected.len() != self.state.world.game.players.len() {
            return Err("invalid native pause presence".into());
        }
        let Some(pause) = &self.pause else {
            return Ok(PausePoll {
                blocked: false,
                send_frame: false,
                message: None,
            });
        };
        if pause.reason == PauseReason::Drop
            && (connected[pause.slot] || pause.until_ms.is_some_and(|until| now_ms >= until))
        {
            let message = self.resume(now_ms);
            return Ok(PausePoll {
                blocked: false,
                send_frame: false,
                message: Some(message),
            });
        }
        let send_frame = now_ms.saturating_sub(self.pause_sent_at) >= 1000;
        if send_frame {
            self.pause_sent_at = now_ms;
        }
        Ok(PausePoll {
            blocked: true,
            send_frame,
            message: send_frame.then(|| self.pause_message(now_ms)),
        })
    }
    pub fn hold_callback(&mut self) -> Result<Option<EndingAction>, String> {
        self.running()?;
        if self.state.finish.is_none() {
            return Ok(None);
        }
        if self.held >= 120 {
            return Err("native ending hold already complete".into());
        }
        self.held += 1;
        Ok(Some(EndingAction {
            advance: self.held.is_multiple_of(2),
            complete: self.held == 120,
        }))
    }
    pub fn held(&self) -> u16 {
        self.held
    }
}

#[cfg(test)]
#[path = "room_runtime_tests.rs"]
mod tests;
