//! Versioned match randomness, with a separate recorded stream for each purpose.
//!
//! SplitMix64 follows Sebastiano Vigna's public-domain reference implementation:
//! <https://prng.di.unimi.it/splitmix64.c>. This is simulation randomness, not a
//! credential or token generator. No platform math, clock or global state is used.

use std::collections::BTreeMap;

pub const RANDOM_VERSION: u32 = 1;
const GAMMA: u64 = 0x9e37_79b9_7f4a_7c15;

/// Stable purpose identifiers. New purposes must not reuse an existing key.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum StreamId {
    Terrain,
    Weather,
    Combat,
    OffMap,
    Ai(u32),
}

impl StreamId {
    fn key(self) -> u64 {
        match self {
            Self::Terrain => 1,
            Self::Weather => 2,
            Self::Combat => 3,
            Self::OffMap => 4,
            Self::Ai(slot) => 0x1_0000_0000 + u64::from(slot),
        }
    }
}

/// State is sufficient to resume the next draw exactly.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(GAMMA);
        mix(self.state)
    }
}

fn mix(mut value: u64) -> u64 {
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RandomSnapshot {
    pub version: u32,
    pub seed: u64,
    /// Strictly increasing stream IDs, with the current state of each used stream.
    pub streams: Vec<(StreamId, u64)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RandomError {
    UnsupportedVersion,
    UnorderedStreams,
    EmptyRange,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MatchRandom {
    seed: u64,
    streams: BTreeMap<StreamId, SplitMix64>,
    #[cfg(test)]
    #[serde(skip)]
    pub(crate) replay: Option<crate::aligned_replay::Replay>,
}

impl MatchRandom {
    pub fn new(seed: u64) -> Self {
        Self {
            seed,
            streams: BTreeMap::new(),
            #[cfg(test)]
            replay: None,
        }
    }

    #[cfg_attr(test, track_caller)]
    pub fn next_u64(&mut self, purpose: StreamId) -> u64 {
        #[cfg(test)]
        if let Some(replay) = &self.replay {
            return replay.integer(purpose, std::panic::Location::caller());
        }
        let seed = self.seed;
        self.streams
            .entry(purpose)
            .or_insert_with(|| SplitMix64::new(mix(seed ^ purpose.key().wrapping_mul(GAMMA))))
            .next_u64()
    }

    /// Uniform 53-bit fraction in [0, 1), using exactly one draw.
    #[cfg_attr(test, track_caller)]
    pub fn next_fraction(&mut self, purpose: StreamId) -> f64 {
        #[cfg(test)]
        if let Some(replay) = &self.replay {
            return replay.fraction(purpose, std::panic::Location::caller());
        }
        (self.next_u64(purpose) >> 11) as f64 * (1.0 / 9_007_199_254_740_992.0)
    }

    /// Unbiased integer in [0, upper), with rejection draws confined to this stream.
    pub fn below(&mut self, purpose: StreamId, upper: u64) -> Result<u64, RandomError> {
        if upper == 0 {
            return Err(RandomError::EmptyRange);
        }
        let threshold = upper.wrapping_neg() % upper;
        loop {
            let value = self.next_u64(purpose);
            if value >= threshold {
                return Ok(value % upper);
            }
        }
    }

    pub fn snapshot(&self) -> RandomSnapshot {
        RandomSnapshot {
            version: RANDOM_VERSION,
            seed: self.seed,
            streams: self
                .streams
                .iter()
                .map(|(&id, rng)| (id, rng.state))
                .collect(),
        }
    }

    pub fn restore(snapshot: RandomSnapshot) -> Result<Self, RandomError> {
        if snapshot.version != RANDOM_VERSION {
            return Err(RandomError::UnsupportedVersion);
        }
        if snapshot
            .streams
            .windows(2)
            .any(|pair| pair[0].0 >= pair[1].0)
        {
            return Err(RandomError::UnorderedStreams);
        }
        Ok(Self {
            #[cfg(test)]
            replay: None,
            seed: snapshot.seed,
            streams: snapshot
                .streams
                .into_iter()
                .map(|(id, state)| (id, SplitMix64 { state }))
                .collect(),
        })
    }
}
