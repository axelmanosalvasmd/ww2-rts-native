//! Tick-boundary order sequencing. Network arrival order never decides a tie.
//!
//! The caller assigns an authoritative tick, issuer and sequence before enqueueing.
//! This module performs no authentication and does not define a wire protocol.

use std::collections::BTreeMap;

pub const ORDER_QUEUE_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct OrderKey {
    pub tick: u64,
    pub issuer: u32,
    pub sequence: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StampedOrder<T> {
    pub key: OrderKey,
    pub value: T,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TickBatch<T> {
    pub tick: u64,
    pub orders: Vec<StampedOrder<T>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueueError {
    PastTick,
    DuplicateKey,
    Full,
    Exhausted,
    UnsupportedVersion,
    InvalidSnapshot,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OrderSnapshot<T> {
    pub version: u32,
    /// None after processing the final representable tick.
    pub next_tick: Option<u64>,
    pub capacity: usize,
    pub pending: Vec<StampedOrder<T>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OrderQueue<T> {
    next_tick: Option<u64>,
    capacity: usize,
    pending: BTreeMap<OrderKey, T>,
}

impl<T> OrderQueue<T> {
    pub fn new(capacity: usize) -> Self {
        Self {
            next_tick: Some(0),
            capacity,
            pending: BTreeMap::new(),
        }
    }

    pub fn next_tick(&self) -> Option<u64> {
        self.next_tick
    }

    pub fn len(&self) -> usize {
        self.pending.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    pub fn enqueue(&mut self, key: OrderKey, value: T) -> Result<(), QueueError> {
        let tick = self.next_tick.ok_or(QueueError::Exhausted)?;
        if key.tick < tick {
            return Err(QueueError::PastTick);
        }
        if self.pending.contains_key(&key) {
            return Err(QueueError::DuplicateKey);
        }
        if self.pending.len() >= self.capacity {
            return Err(QueueError::Full);
        }
        self.pending.insert(key, value);
        Ok(())
    }

    /// Consume exactly one tick, including an empty tick. Values stay ordered by
    /// issuer and then sequence. Clock time does not enter this operation.
    pub fn advance(&mut self) -> Result<TickBatch<T>, QueueError> {
        let tick = self.next_tick.ok_or(QueueError::Exhausted)?;
        let first = OrderKey {
            tick,
            issuer: 0,
            sequence: 0,
        };
        let last = OrderKey {
            tick,
            issuer: u32::MAX,
            sequence: u64::MAX,
        };
        let keys: Vec<_> = self
            .pending
            .range(first..=last)
            .map(|(&key, _)| key)
            .collect();
        let orders = keys
            .into_iter()
            .map(|key| StampedOrder {
                key,
                value: self.pending.remove(&key).expect("listed key"),
            })
            .collect();
        self.next_tick = tick.checked_add(1);
        Ok(TickBatch { tick, orders })
    }

    pub fn restore(snapshot: OrderSnapshot<T>) -> Result<Self, QueueError> {
        if snapshot.version != ORDER_QUEUE_VERSION {
            return Err(QueueError::UnsupportedVersion);
        }
        if snapshot.pending.len() > snapshot.capacity
            || snapshot
                .pending
                .windows(2)
                .any(|pair| pair[0].key >= pair[1].key)
            || snapshot
                .pending
                .iter()
                .any(|order| snapshot.next_tick.is_none_or(|tick| order.key.tick < tick))
        {
            return Err(QueueError::InvalidSnapshot);
        }
        Ok(Self {
            next_tick: snapshot.next_tick,
            capacity: snapshot.capacity,
            pending: snapshot
                .pending
                .into_iter()
                .map(|order| (order.key, order.value))
                .collect(),
        })
    }
}

impl<T: Clone> OrderQueue<T> {
    pub fn snapshot(&self) -> OrderSnapshot<T> {
        OrderSnapshot {
            version: ORDER_QUEUE_VERSION,
            next_tick: self.next_tick,
            capacity: self.capacity,
            pending: self
                .pending
                .iter()
                .map(|(&key, value)| StampedOrder {
                    key,
                    value: value.clone(),
                })
                .collect(),
        }
    }
}
