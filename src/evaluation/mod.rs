//! Offline search evaluation: graded relevance judgments against ranked runs.
//! See `.agent/rust-knowledge/evaluation.md` for the protocol these files follow.

mod data;
mod metrics;

pub use data::{
    Qrels, Query, QueryFilters, RunRow, group_runs, read_qrels, read_queries, read_run,
};
pub use metrics::{QueryMetrics, Summary, bootstrap_diff, evaluate_query, summarize};
