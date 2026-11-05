//! Online evaluation (P10): which variant a session sees and how the events are analysed.
//! The experiment is fixed here, in code, before any traffic: variants, shares, the primary
//! metric and the smallest effect worth detecting. Changing any of it means a new `version`.

pub mod analysis;
pub mod simulate;

use crate::search::semantic::Mode;
use serde::Serialize;
use sha2::{Digest, Sha256};
use utoipa::ToSchema;

/// One arm of the experiment. The arm fully decides the search configuration.
#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
pub struct Variant {
    pub name: &'static str,
    pub mode: Mode,
}

/// The pre-registered plan. One change per experiment: B0 against hybrid retrieval.
#[derive(Debug, Serialize, ToSchema)]
pub struct Config {
    pub id: &'static str,
    pub version: u32,
    pub hypothesis: &'static str,
    pub primary_metric: &'static str,
    /// Smallest absolute change of the primary metric the experiment is sized to detect.
    pub minimum_effect: f64,
    pub alpha: f64,
    pub power: f64,
    /// Share of sessions in the treatment arm, in basis points (5000 = 50%).
    pub treatment_share_bp: u32,
    pub control: Variant,
    pub treatment: Variant,
}

pub const EXPERIMENT: Config = Config {
    id: "hybrid-vs-b0",
    version: 1,
    hypothesis: "Hybrid retrieval (lexical + vectors, RRF) raises the share of search sessions \
                 that end in a click without an immediate reformulation, compared with B0.",
    primary_metric: "session success rate",
    minimum_effect: 0.05,
    alpha: 0.05,
    power: 0.8,
    treatment_share_bp: 5000,
    control: Variant {
        name: "b0",
        mode: Mode::Lexical,
    },
    treatment: Variant {
        name: "hybrid",
        mode: Mode::Hybrid,
    },
};

/// A click closer to the next search than this is read as a reformulation.
pub const REFORMULATION_WINDOW_SECS: i64 = 60;

/// Session IDs are random strings made by the client; nothing identifies a person in them.
pub fn valid_session(session: &str) -> bool {
    (8..=64).contains(&session.len())
        && session
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

impl Config {
    /// Deterministic: the same session and experiment version always get the same arm, on any
    /// node and after any restart. The hash makes the arm independent of the other experiments.
    pub fn assign(&self, session: &str) -> &Variant {
        let digest = Sha256::digest(format!("{}:{}:{session}", self.id, self.version));
        let bucket = u64::from_be_bytes(digest[..8].try_into().expect("8 bytes")) % 10_000;
        if bucket < u64::from(self.treatment_share_bp) {
            &self.treatment
        } else {
            &self.control
        }
    }

    pub fn variant(&self, name: &str) -> Option<&Variant> {
        [&self.control, &self.treatment]
            .into_iter()
            .find(|variant| variant.name == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_session_always_gets_the_same_variant() {
        for i in 0..200 {
            let session = format!("session-{i:05}");
            let first = EXPERIMENT.assign(&session).name;
            for _ in 0..5 {
                assert_eq!(EXPERIMENT.assign(&session).name, first);
            }
        }
        // Pinned values: a change here changes who sees what in a running experiment.
        // (bucket 7745 and 3653 of 10000, computed independently from sha256 of
        // "hybrid-vs-b0:1:<session>")
        assert_eq!(EXPERIMENT.assign("session-00001").name, "b0");
        assert_eq!(EXPERIMENT.assign("session-00004").name, "hybrid");
    }

    #[test]
    fn the_split_follows_the_configured_share() {
        let treated = (0..20_000)
            .filter(|i| EXPERIMENT.assign(&format!("s-{i:08}")).name == "hybrid")
            .count();
        let share = treated as f64 / 20_000.0;
        assert!((share - 0.5).abs() < 0.015, "share was {share}");
    }

    #[test]
    fn session_ids_are_validated() {
        assert!(valid_session("abcdef12-xyz_90"));
        assert!(!valid_session("short"));
        assert!(!valid_session("has space in it"));
        assert!(!valid_session(&"a".repeat(65)));
    }
}
