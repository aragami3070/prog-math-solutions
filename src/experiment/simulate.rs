//! Synthetic traffic for checking the pipeline end to end. It is NOT evidence about the search:
//! the "effect" is whatever the caller puts in, and every report built from it says so.

use super::EXPERIMENT;
use crate::models::events::EventRow;
use chrono::{TimeZone, Utc};

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
}

const QUERIES: &[&str] = &[
    "RFC 3058",
    "async traits",
    "std::mem::swap",
    "borrow checker",
    "как работает borrow checker",
    "const generics",
    "pin projection",
    "never type stabilization",
];

/// `effect` is added to the click probability of the treatment arm.
pub fn events(sessions: usize, seed: u64, baseline: f64, effect: f64) -> Vec<EventRow> {
    let mut rng = Rng(seed.max(1).wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    let mut out = Vec::new();
    let mut clock = 0i64;
    for i in 0..sessions {
        let session = format!("sim-{seed}-{i:07}");
        let variant = EXPERIMENT.assign(&session);
        let p_click = baseline
            + if variant.name == EXPERIMENT.treatment.name {
                effect
            } else {
                0.0
            };
        clock += 30;
        let mut at = clock;
        let queries = 1 + (rng.next() * 3.0) as usize;
        for q in 0..queries {
            let query_id = format!("{session}-q{q}");
            let text = QUERIES[(rng.next() * QUERIES.len() as f64) as usize % QUERIES.len()];
            let zero = rng.next() < 0.02;
            out.push(EventRow {
                at: Utc.timestamp_opt(1_700_000_000 + at, 0).unwrap(),
                kind: "search".into(),
                session_id: session.clone(),
                variant: variant.name.into(),
                query_id: query_id.clone(),
                query: Some(if q == 0 {
                    text.to_owned()
                } else {
                    format!("{text} tutorial")
                }),
                results: Some(if zero { 0 } else { 20 }),
                latency_ms: Some(
                    20 + (rng.next() * 40.0) as i32 + if variant.name == "hybrid" { 60 } else { 0 },
                ),
                degraded: Some(false),
                document_id: None,
                rank: None,
            });
            let clicked = !zero && rng.next() < p_click;
            if clicked {
                // Position bias: most clicks go to the first results.
                let mut rank = 1;
                while rank < 10 && rng.next() < 0.45 {
                    rank += 1;
                }
                at += 6;
                out.push(EventRow {
                    at: Utc.timestamp_opt(1_700_000_000 + at, 0).unwrap(),
                    kind: "click".into(),
                    session_id: session.clone(),
                    variant: variant.name.into(),
                    query_id,
                    query: None,
                    results: None,
                    latency_ms: None,
                    degraded: None,
                    document_id: Some(format!("rfc:{}", 1 + (rng.next() * 3000.0) as u32)),
                    rank: Some(rank),
                });
                // Some satisfied users leave, some keep searching right away.
                if rng.next() > 0.12 {
                    break;
                }
            }
            at += 15;
        }
    }
    out
}
