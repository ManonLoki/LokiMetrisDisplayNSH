//! Bounded retry decisions shared by context and content-addressed file delivery.
use std::time::{Duration, Instant};

/// At most three attempts per update or explicit system recovery event.
const MAX_ATTEMPTS: u8 = 3;

#[derive(Default)]
pub(crate) struct Retry {
    attempts: u8,
    after: Option<Instant>,
}

impl Retry {
    pub(crate) fn ready(&self, now: Instant) -> bool {
        self.attempts < MAX_ATTEMPTS && self.after.is_none_or(|after| now >= after)
    }

    pub(crate) fn started(&mut self) {
        self.attempts = self.attempts.saturating_add(1);
        self.after = None;
    }

    pub(crate) fn failed(&mut self, now: Instant) {
        let seconds = 5_u64 << self.attempts.saturating_sub(1).min(3);
        self.after = (self.attempts < MAX_ATTEMPTS).then_some(now + Duration::from_secs(seconds));
    }

    pub(crate) fn deadline(&self) -> Option<Instant> {
        self.after
    }
}

/// Older files can satisfy a newer manifest's identical content hash, never its state.
pub(crate) fn reusable(
    stream: &str,
    revision: u64,
    current_stream: &str,
    current_revision: u64,
    referenced: bool,
) -> bool {
    referenced && stream == current_stream && revision > 0 && revision <= current_revision
}

#[cfg(test)]
#[path = "policy_test.rs"]
mod tests;
