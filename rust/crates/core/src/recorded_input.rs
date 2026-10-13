//! Trusted gameplay identities for the existing recorded order queue.
use std::num::NonZeroU64;

/// A match-scoped identity assigned by the authenticated room adapter.
/// It is distinct from the registry's dispatch-ordered CompanionId. The safe key
/// is derived here; callers cannot supply credential text as a recorded key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompanionJournalId(NonZeroU64);
impl CompanionJournalId {
    pub fn new(id: u64) -> Option<Self> {
        NonZeroU64::new(id).map(Self)
    }
    pub const fn get(self) -> u64 {
        self.0.get()
    }
    pub fn registry_key(self) -> String {
        format!("journal-companion:{}", self.get())
    }
}

#[cfg(test)]
mod tests;
