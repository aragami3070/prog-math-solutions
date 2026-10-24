use super::errors::Unavailable;
use std::time::Duration;

pub const MAX_ATTEMPTS: u32 = 3;
/// Pages in a row that failed as unreachable before the run is stopped.
pub const MAX_UNREACHABLE_STREAK: u32 = 5;
const MAX_BACKOFF: Duration = Duration::from_secs(300);

/// Wait before retry number `failures` (1-based): the crawl delay doubled each time.
/// It comes on top of the delay every request already waits.
pub fn backoff(delay: Duration, failures: u32) -> Duration {
    delay
        .saturating_mul(2u32.saturating_pow(failures))
        .min(MAX_BACKOFF)
}

#[derive(Debug, PartialEq, Eq)]
pub enum Step {
    /// Not a transient failure (or a success): hand the result back unchanged.
    Done,
    Wait(Duration),
    /// Out of attempts: the source stayed unreachable.
    GiveUp(String),
}

/// What to do after attempt number `attempt` (1-based). `transient` is the reason the
/// result is worth retrying, or `None` for a success or a permanent error.
pub fn step(transient: Option<String>, attempt: u32, delay: Duration) -> Step {
    match transient {
        None => Step::Done,
        Some(reason) if attempt >= MAX_ATTEMPTS => {
            Step::GiveUp(format!("{reason} (after {attempt} attempts)"))
        }
        Some(_) => Step::Wait(backoff(delay, attempt)),
    }
}

/// Counts consecutive pages that failed because the source was unreachable.
pub struct UnreachableStreak(u32);

impl UnreachableStreak {
    pub fn new() -> Self {
        Self(0)
    }

    /// Records one page outcome and says whether the run should stop.
    pub fn record(&mut self, error: Option<&anyhow::Error>) -> bool {
        match error {
            Some(error) if error.downcast_ref::<Unavailable>().is_some() => self.0 += 1,
            _ => self.0 = 0,
        }
        self.0 >= MAX_UNREACHABLE_STREAK
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DELAY: Duration = Duration::from_secs(5);

    #[test]
    fn backoff_doubles_and_is_capped() {
        assert_eq!(backoff(DELAY, 1), Duration::from_secs(10));
        assert_eq!(backoff(DELAY, 2), Duration::from_secs(20));
        assert_eq!(backoff(DELAY, 40), MAX_BACKOFF);
        assert_eq!(backoff(Duration::from_secs(300), 3), MAX_BACKOFF);
    }

    #[test]
    fn retries_a_transient_failure_with_growing_waits_then_gives_up() {
        let reason = || Some("connection reset".to_string());
        assert_eq!(
            step(reason(), 1, DELAY),
            Step::Wait(Duration::from_secs(10))
        );
        assert_eq!(
            step(reason(), 2, DELAY),
            Step::Wait(Duration::from_secs(20))
        );
        // No wait after the final attempt.
        assert_eq!(
            step(reason(), 3, DELAY),
            Step::GiveUp("connection reset (after 3 attempts)".into())
        );
    }

    #[test]
    fn permanent_errors_and_successes_are_returned_at_once() {
        for attempt in 1..=MAX_ATTEMPTS {
            assert_eq!(step(None, attempt, DELAY), Step::Done);
        }
    }

    #[test]
    fn streak_stops_the_run_only_after_consecutive_unreachable_pages() {
        let unreachable: anyhow::Error = Unavailable("timeout".into()).into();
        let parse_error = anyhow::anyhow!("document title not found");
        let mut streak = UnreachableStreak::new();
        for _ in 0..MAX_UNREACHABLE_STREAK - 1 {
            assert!(!streak.record(Some(&unreachable)));
        }
        // Any other outcome breaks the streak.
        assert!(!streak.record(Some(&parse_error)));
        for _ in 0..MAX_UNREACHABLE_STREAK - 1 {
            assert!(!streak.record(Some(&unreachable)));
        }
        assert!(!streak.record(None));
        for _ in 0..MAX_UNREACHABLE_STREAK - 1 {
            assert!(!streak.record(Some(&unreachable)));
        }
        assert!(streak.record(Some(&unreachable)));
    }
}
