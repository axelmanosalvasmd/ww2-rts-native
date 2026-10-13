//! Socket-local snapshot admission and completion, independent of room simulation.
//! Admission happens before producing a snapshot or changing its delta history.

use std::collections::BTreeMap;

pub const SEAT_INFLIGHT_LIMIT: u64 = 256 * 1024;
pub const WATCH_BUFFER_LIMIT: u64 = 4 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackpressureError {
    Capacity,
    UnknownCompletion,
}
impl std::fmt::Display for BackpressureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Capacity => "snapshot write accounting capacity exhausted",
            Self::UnknownCompletion => "snapshot write completion is unknown or already consumed",
        })
    }
}
impl std::error::Error for BackpressureError {}

/// Opaque write identity. The transport completes each reserved write once.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct WriteTicket(u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompletionContext {
    pub current_socket: bool,
    pub seat_present: bool,
    pub playing: bool,
    pub has_game: bool,
}

/// One instance belongs to one socket, including after its seat reconnects elsewhere.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SeatBackpressure {
    inflight: u64,
    owed: bool,
    next_ticket: u64,
    pending: BTreeMap<WriteTicket, u64>,
}
impl SeatBackpressure {
    pub fn inflight(&self) -> u64 {
        self.inflight
    }
    pub fn owed(&self) -> bool {
        self.owed
    }

    /// A backed-up seat owes one fresh frame. Skips must retain all producer history.
    pub fn skip_snapshot(&mut self) -> bool {
        if self.inflight < SEAT_INFLIGHT_LIMIT {
            return false;
        }
        self.owed = true;
        true
    }

    /// Reserve a write before handing its encoded JSON to the transport.
    /// The prototype counts JavaScript string length, including its deflate queue.
    /// UTF-16 units preserve that threshold for names containing non-ASCII text.
    pub fn begin_write(&mut self, json: &str) -> Result<WriteTicket, BackpressureError> {
        let length =
            u64::try_from(json.encode_utf16().count()).map_err(|_| BackpressureError::Capacity)?;
        let inflight = self
            .inflight
            .checked_add(length)
            .ok_or(BackpressureError::Capacity)?;
        let ticket = WriteTicket(
            self.next_ticket
                .checked_add(1)
                .ok_or(BackpressureError::Capacity)?,
        );
        self.pending.insert(ticket, length);
        self.inflight = inflight;
        self.next_ticket = ticket.0;
        Ok(ticket)
    }

    /// True requests one catch-up frame with empty shots/cells and a fresh cache.
    /// A stale socket callback drains only that old socket's accounting.
    pub fn complete_write(
        &mut self,
        ticket: WriteTicket,
        context: CompletionContext,
    ) -> Result<bool, BackpressureError> {
        let length = *self
            .pending
            .get(&ticket)
            .ok_or(BackpressureError::UnknownCompletion)?;
        let inflight = self
            .inflight
            .checked_sub(length)
            .ok_or(BackpressureError::Capacity)?;
        self.pending.remove(&ticket);
        self.inflight = inflight;
        if !self.owed || inflight >= SEAT_INFLIGHT_LIMIT || !context.current_socket {
            return Ok(false);
        }
        self.owed = false;
        Ok(context.seat_present && context.playing && context.has_game)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WatcherAction {
    Disconnected,
    Send,
    Close { code: u16, reason: &'static str },
}

/// A shared watcher stream cannot skip a delta, so its slow sockets reconnect.
pub fn watcher_action(connected: bool, buffered: Option<u64>) -> WatcherAction {
    if !connected {
        WatcherAction::Disconnected
    } else if buffered.unwrap_or(0) > WATCH_BUFFER_LIMIT {
        WatcherAction::Close {
            code: 1013,
            reason: "too far behind",
        }
    } else {
        WatcherAction::Send
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn original_socket_callbacks_and_watcher_boundaries() {
        let fixture: Value =
            serde_json::from_str(include_str!("../tests/fixtures/snapshot-backpressure.json"))
                .unwrap();
        for trace in fixture["traces"].as_array().unwrap() {
            let mut state = SeatBackpressure::default();
            let mut writes = BTreeMap::new();
            for step in trace["steps"].as_array().unwrap() {
                let expected = &step["expected"];
                match step["op"].as_str().unwrap() {
                    "send" => {
                        let ticket = state.begin_write(step["json"].as_str().unwrap()).unwrap();
                        assert!(
                            writes
                                .insert(step["id"].as_u64().unwrap(), ticket)
                                .is_none()
                        );
                    }
                    "admit" => assert_eq!(
                        state.skip_snapshot(),
                        expected["skip"].as_bool().unwrap(),
                        "{}",
                        trace["name"]
                    ),
                    "complete" => {
                        let context = CompletionContext {
                            current_socket: step["currentSocket"].as_bool().unwrap(),
                            seat_present: step["seatPresent"].as_bool().unwrap(),
                            playing: step["playing"].as_bool().unwrap(),
                            has_game: step["hasGame"].as_bool().unwrap(),
                        };
                        let ticket = writes.remove(&step["id"].as_u64().unwrap()).unwrap();
                        assert_eq!(
                            state.complete_write(ticket, context).unwrap(),
                            expected["catchUp"].as_bool().unwrap(),
                            "{}",
                            trace["name"]
                        );
                    }
                    _ => panic!("unknown source operation"),
                }
                assert_eq!(state.inflight(), expected["inflight"].as_u64().unwrap());
                assert_eq!(state.owed(), expected["owed"].as_bool().unwrap());
            }
        }
        for case in fixture["watcherCases"].as_array().unwrap() {
            let action = watcher_action(
                case["connected"].as_bool().unwrap(),
                case["buffered"].as_u64(),
            );
            match case["expected"].as_str().unwrap() {
                "send" => assert_eq!(action, WatcherAction::Send),
                "disconnected" => assert_eq!(action, WatcherAction::Disconnected),
                "close" => assert_eq!(
                    action,
                    WatcherAction::Close {
                        code: 1013,
                        reason: "too far behind"
                    }
                ),
                _ => panic!("unknown watcher action"),
            }
        }
    }

    #[test]
    fn duplicate_completion_and_capacity_errors_preserve_history() {
        let mut state = SeatBackpressure::default();
        let ticket = state.begin_write("🌍é").unwrap();
        assert_eq!(state.inflight(), 3);
        let context = CompletionContext {
            current_socket: true,
            seat_present: true,
            playing: true,
            has_game: true,
        };
        assert!(!state.complete_write(ticket, context).unwrap());
        let before = state.clone();
        assert_eq!(
            state.complete_write(ticket, context),
            Err(BackpressureError::UnknownCompletion)
        );
        assert_eq!(state, before);
        state.next_ticket = u64::MAX;
        let before = state.clone();
        assert_eq!(state.begin_write("{}"), Err(BackpressureError::Capacity));
        assert_eq!(state, before);
        state.next_ticket = 0;
        state.inflight = u64::MAX;
        let before = state.clone();
        assert_eq!(state.begin_write("x"), Err(BackpressureError::Capacity));
        assert_eq!(state, before);
    }
}
