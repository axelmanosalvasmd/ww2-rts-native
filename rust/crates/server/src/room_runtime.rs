//! One room runtime over the canonical Match and recipient producers.
//! Gameplay admission remains with the server command handlers.
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
    slot: Option<usize>,
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

/// Host control over an existing Horde break clock.
pub(crate) fn end_horde_break(state: &mut Match, host: bool, paused: bool) -> Result<bool, String> {
    if !host || paused {
        return Ok(false);
    }
    state.end_horde_break()
}

/// A caller-owned output batch retains real tick effects until a scheduled send.
/// It is not another simulation state or producer event authority.
#[path = "room_delivered_ai.rs"]
mod ai;

pub struct RoomRuntime {
    delivered_ai: ai::RoomAi,
    #[cfg(test)]
    pub(crate) ai: Option<crate::room_ai::RoomAi>,
    #[cfg(test)]
    pub(crate) legacy_ai_queued: usize,
    state: Match,
    delivery: DeliveryState,
    watchers: ww2_core::recipient_snapshot::SpectatorDelivery,
    lists: RoomListCache,
    meter: TickMeter,
    batch: u64,
    last_versions: Option<(u64, ListVersions)>,
    pending_events: Vec<TickEvent>,
    event_generation: u64,
    tick_failure: Option<TickFailure>,
    fault: Option<String>,
    pause: Option<Pause>,
    pause_sent_at: u64,
    auto_paused: BTreeSet<usize>,
    held: u16,
    #[cfg(test)]
    snapshot_production_calls: u64,
}
impl RoomRuntime {
    #[cfg(test)]
    pub(crate) fn take_ai_trace(&mut self) -> Vec<ai::DecisionTrace> {
        self.delivered_ai.take_trace()
    }

    /// Establish full start baselines on the existing receipt owner without shot previews.
    pub(crate) fn collect_starts(
        &mut self,
        slots: &[usize],
        startup: &crate::native_startup::NativeStartup,
        names: &[String],
        match_id: u64,
    ) -> Result<Vec<(usize, Value)>, String> {
        self.running()?;
        let envelope = startup.envelope(&self.state, names, match_id)?;
        let selected = slots
            .iter()
            .map(|&slot| SelectedSeat {
                slot,
                full: true,
                omit: false,
            })
            .collect::<Vec<_>>();
        let frame = self
            .delivery
            .capture_selected_with_lists(&mut self.state, &selected, &[])
            .map_err(|e| format!("native startup delivery: {e:?}"))?
            .ok_or("native startup has no seats")?;
        let mut messages = Vec::new();
        for &slot in slots {
            let snapshot = frame
                .recipient(slot)
                .map_err(|e| format!("AI start recipient: {e:?}"))?
                .into_json();
            self.delivered_ai.observe(slot, &snapshot, true)?;
            messages.push((slot, envelope.message(slot, snapshot)));
        }
        Ok(messages)
    }
    pub(crate) fn collect_watcher_start(
        &self,
        startup: &crate::native_startup::NativeStartup,
        names: &[String],
        match_id: u64,
    ) -> Result<Value, String> {
        self.running()?;
        let envelope = startup.envelope(&self.state, names, match_id)?;
        let mut fresh = ww2_core::recipient_snapshot::SpectatorDelivery::default();
        let mut snapshot = fresh
            .capture(&self.state, &[], true)
            .map_err(|e| format!("watcher start: {e:?}"))?;
        snapshot["fog"] = Value::Null;
        let mut start = envelope.message(0, snapshot);
        start.as_object_mut().unwrap().remove("fog");
        Ok(start)
    }
    pub(crate) fn collect_watcher_frame(&mut self) -> Result<Value, String> {
        self.running()?;
        self.watchers
            .capture(&self.state, &self.pending_events, false)
            .map_err(|e| format!("watcher snapshot: {e:?}"))
    }
    pub fn new(mut state: Match, now_ms: u64) -> Result<Self, String> {
        state.validate_tick_state()?;
        if state.options.mode == ww2_core::economy::Mode::Horde {
            state.tick_state.restrict_aircraft_weapons_for_delivery();
        }
        state
            .restrict_to_ordinary_snapshots()
            .map_err(|e| format!("snapshot admission: {e:?}"))?;
        state.ordinary_snapshot_raw_lists()?;
        let delivery =
            DeliveryState::new(&state).map_err(|e| format!("delivery admission: {e:?}"))?;
        let meter = TickMeter::new(now_ms).map_err(|e| e.to_string())?;
        Ok(Self {
            state,
            delivered_ai: ai::RoomAi::default(),
            #[cfg(test)]
            ai: None,
            #[cfg(test)]
            legacy_ai_queued: 0,
            delivery,
            watchers: Default::default(),
            lists: RoomListCache::default(),
            meter,
            batch: 0,
            last_versions: None,
            pending_events: Vec::new(),
            event_generation: 0,
            tick_failure: None,
            fault: None,
            pause: None,
            pause_sent_at: now_ms,
            auto_paused: BTreeSet::new(),
            held: 0,
            #[cfg(test)]
            snapshot_production_calls: 0,
        })
    }
    #[cfg(test)]
    pub(crate) fn ai_delivered_view(
        &self,
        slot: usize,
    ) -> Option<&ww2_core::human_commander::HumanView<ww2_core::commander_ai::Observation>> {
        self.delivered_ai.delivered_view(slot)
    }
    #[cfg(test)]
    pub(crate) fn ai_level(&self, slot: usize) -> Option<ww2_core::commander_ai::Level> {
        self.delivered_ai.level(slot)
    }
    #[cfg(test)]
    pub(crate) fn ai_decoded_view(&self, slot: usize) -> Option<&ww2_core::ai_view::SeatView> {
        self.delivered_ai.decoded_view(slot)
    }
    pub(crate) fn activate_ai(
        &mut self,
        map: &Value,
        slot: usize,
        level: &str,
    ) -> Result<(), String> {
        #[cfg(test)]
        if self.ai.is_some() {
            return Err("hosted AI owner is already configured".into());
        }
        self.delivered_ai.activate(
            &self.state,
            map,
            slot,
            ww2_core::commander_ai::Level::from_key(level).unwrap_or_default(),
        )
    }
    pub(crate) fn reset_ai(&mut self, slot: usize) {
        self.delivered_ai.reset(slot);
        #[cfg(test)]
        if let Some(ai) = &mut self.ai {
            ai.reset(slot);
        }
    }
    pub(crate) fn ai_selected(&self) -> Vec<SelectedSeat> {
        if self.state.finish.is_some() {
            Vec::new()
        } else {
            self.delivered_ai.selected(self.state.world.game.tick)
        }
    }
    pub(crate) fn tick_ai(&mut self, sent: bool) -> Result<(), String> {
        self.running()?;
        self.delivered_ai.tick(&mut self.state, sent)
    }
    #[cfg(test)]
    pub(crate) fn ai_commander(
        &self,
        slot: usize,
    ) -> Option<&ww2_core::seat_commander::SeatCommander> {
        self.delivered_ai.commander(slot)
    }
    #[cfg(test)]
    pub(crate) fn ai_clone_failure(&self) -> Value {
        self.delivered_ai
            .clone_failure(self.state.start.players.len())
    }
    pub(crate) fn save_ai_memories(&self) -> Value {
        let mut memories = self
            .delivered_ai
            .save_memories(self.state.start.players.len());
        if let Some(horde) = &self.state.horde
            && let Some(rows) = memories.as_array_mut()
        {
            rows[horde.slot] = Value::Null;
            if rows.iter().all(Value::is_null) {
                return Value::Null;
            }
        }
        memories
    }
    pub(crate) fn restore_ai_memories(&mut self, saved: &Value) -> Result<(), String> {
        self.delivered_ai
            .restore_memories(saved, self.state.start.players.len())
    }
    #[cfg(test)]
    pub(crate) fn configure_ai(
        &mut self,
        seats: &[(usize, ww2_core::commander_ai::Level)],
    ) -> Result<(), String> {
        if !self
            .delivered_ai
            .selected(self.state.world.game.tick)
            .is_empty()
        {
            return Err("delivered AI owner is already configured".into());
        }
        self.ai = Some(crate::room_ai::RoomAi::new(
            &mut self.state,
            &mut self.delivery,
            seats,
        )?);
        Ok(())
    }
    #[cfg(test)]
    pub(crate) fn think(&mut self, sent: bool) -> Result<(), String> {
        self.running()?;
        if let Some(ai) = &mut self.ai {
            let before = self.state.commands.pending_commands();
            ai.tick(
                &mut self.state,
                &mut self.delivery,
                sent,
                self.event_generation,
                &self.pending_events,
            )?;
            self.legacy_ai_queued += self
                .state
                .commands
                .pending_commands()
                .saturating_sub(before);
        }
        Ok(())
    }
    pub(crate) fn save_json(&self) -> Result<Value, String> {
        self.running()?;
        if self.held >= 120 {
            return Err("completed ending cannot be saved".into());
        }
        Ok(
            json!({"state":self.state.save_json()?, "pendingEvents":Match::save_events_json(&self.pending_events)?,"held":self.held}),
        )
    }
    pub(crate) fn restore_saved_hold(&mut self, held: &Value) -> Result<(), String> {
        let held = held
            .as_u64()
            .and_then(|v| u16::try_from(v).ok())
            .filter(|v| *v < 120)
            .ok_or("invalid saved ending hold")?;
        if held > 0 && self.state.finish.is_none() {
            return Err("saved hold has no finish".into());
        }
        self.held = held;
        Ok(())
    }
    pub(crate) fn restore_saved_events(&mut self, value: Value) -> Result<(), String> {
        let events = Match::events_from_save_json(value)?;
        let mut probe = Self::new(Match::from_save_json(self.state.save_json()?)?, 0)?;
        probe.pending_events = events.clone();
        let selected = (0..self.state.world.game.players.len())
            .map(|slot| SelectedSeat {
                slot,
                full: true,
                omit: false,
            })
            .collect::<Vec<_>>();
        probe.collect_frame(&selected, FrameKind::Scheduled)?;
        self.pending_events = events;
        Ok(())
    }
    pub fn state(&self) -> &Match {
        &self.state
    }
    #[cfg(test)]
    pub(crate) fn snapshot_production_calls(&self) -> u64 {
        self.snapshot_production_calls
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
    pub(crate) fn timing_stats(&self) -> crate::tick_timing::TimingStats {
        self.meter.stats()
    }
    pub(crate) fn snapshot_cadence(&self) -> u8 {
        self.meter.snap_every()
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
        let next_event_generation = self
            .event_generation
            .checked_add(u64::from(kind == FrameKind::Scheduled))
            .ok_or("AI frame generation exhausted")?;
        if selected.is_empty() {
            if kind == FrameKind::Scheduled {
                self.pending_events.clear();
                self.event_generation = next_event_generation;
            }
            return Ok(Vec::new());
        }
        let batch = self
            .batch
            .checked_add(1)
            .ok_or("native snapshot batch exhausted")?;
        #[cfg(test)]
        {
            self.snapshot_production_calls += 1;
        }
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
            let message = frame
                .recipient(seat.slot)
                .map_err(|e| format!("native recipient: {e:?}"))?
                .into_json();
            self.delivered_ai
                .observe(seat.slot, &message, kind == FrameKind::Scheduled)?;
            captured.push(CapturedSeat {
                slot: seat.slot,
                batch,
                list_versions: versions,
                message,
            });
        }
        drop(frame);
        self.lists = lists;
        self.batch = batch;
        self.last_versions = Some((batch, versions));
        if kind == FrameKind::Scheduled {
            self.pending_events.clear();
            self.event_generation = next_event_generation;
        }
        Ok(captured)
    }
    pub(crate) fn frame_events(&self, kind: FrameKind) -> &[TickEvent] {
        if kind == FrameKind::Scheduled {
            &self.pending_events
        } else {
            &[]
        }
    }
    pub(crate) fn operative_list_scope(&mut self) -> Result<ListScope, String> {
        let raw = self.state.ordinary_snapshot_raw_lists()?;
        self.batch = self
            .batch
            .checked_add(1)
            .ok_or("native snapshot batch exhausted")?;
        let versions = self
            .lists
            .capture(self.batch, Some(&raw.wrecks), raw.nodes.as_ref())
            .map_err(|e| e.to_string())?;
        Ok(ListScope::Shared(versions))
    }

    /// Existing JSON v0 latency telemetry, derived from scheduled snapshot ticks.
    pub(crate) fn latency_stats(&self) -> Value {
        json!([
            ww2_core::math::round(self.meter.stats().snapshot_tick.p95 * 10.0) / 10.0,
            self.meter.snap_every()
        ])
    }
    pub(crate) fn open_loaded_pause(
        &mut self,
        slot: usize,
        by: &str,
        now_ms: u64,
    ) -> Result<(), String> {
        self.pause = None;
        self.auto_paused.clear();
        self.pause(slot, by, PauseReason::Host, now_ms)?;
        Ok(())
    }
    pub fn paused(&self) -> bool {
        self.pause.is_some()
    }
    pub(crate) fn next_wave(&mut self, host: bool) -> Result<bool, String> {
        self.running()?;
        let paused = self.paused();
        end_horde_break(&mut self.state, host, paused)
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
        self.pause_inner(Some(slot), by, reason, now_ms)
    }
    pub(crate) fn pause_host(&mut self, by: &str, now_ms: u64) -> Result<Option<Value>, String> {
        self.pause_inner(None, by, PauseReason::Host, now_ms)
    }
    fn pause_inner(
        &mut self,
        slot: Option<usize>,
        by: &str,
        reason: PauseReason,
        now_ms: u64,
    ) -> Result<Option<Value>, String> {
        self.running()?;
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
            .is_some_and(|pause| pause.reason == PauseReason::Drop && pause.slot == Some(slot));
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
            && (pause.slot.is_some_and(|slot| connected[slot])
                || pause.until_ms.is_some_and(|until| now_ms >= until))
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

#[cfg(test)]
#[path = "construction_production_tests.rs"]
mod construction_production_tests;

#[cfg(test)]
#[path = "scenario_recipient_tests.rs"]
mod scenario_recipient_tests;

#[cfg(test)]
#[path = "spectator_mission_tests.rs"]
mod spectator_mission_tests;

#[cfg(test)]
#[path = "core_memory_order_tests.rs"]
mod core_memory_order_tests;
