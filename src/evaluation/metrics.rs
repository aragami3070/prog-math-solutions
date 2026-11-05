use serde::Serialize;
use std::collections::HashMap;

const K: usize = 10;

/// Metrics of one query. `None` means the metric is undefined for it (no known relevant
/// document), which is not the same as 0.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct QueryMetrics {
    pub ndcg10: Option<f64>,
    pub p10: f64,
    pub strict_p10: f64,
    pub mrr10: f64,
    /// Relevant found in the top 10 out of all relevant judged for the query (the pool, not
    /// the unknown corpus).
    pub pooled_recall10: Option<f64>,
    /// Share of the top 10 that has a judgment.
    pub coverage10: f64,
    pub returned: usize,
    pub has_relevant: bool,
}

fn gain(grade: u8) -> f64 {
    f64::from((1u32 << grade) - 1)
}

fn discount(rank: usize) -> f64 {
    ((rank + 1) as f64).log2()
}

/// `ranked` is best first; `judged` holds the grades of this query. Unjudged documents
/// add no gain, and `coverage10` says how much of the top 10 that affects.
pub fn evaluate_query(ranked: &[String], judged: &HashMap<String, u8>) -> QueryMetrics {
    let top: Vec<Option<u8>> = ranked
        .iter()
        .take(K)
        .map(|doc| judged.get(doc).copied())
        .collect();
    let dcg: f64 = top
        .iter()
        .enumerate()
        .map(|(i, grade)| gain(grade.unwrap_or(0)) / discount(i + 1))
        .sum();
    let mut ideal: Vec<u8> = judged.values().copied().collect();
    ideal.sort_unstable_by(|a, b| b.cmp(a));
    let idcg: f64 = ideal
        .iter()
        .take(K)
        .enumerate()
        .map(|(i, grade)| gain(*grade) / discount(i + 1))
        .sum();
    let relevant_total = judged.values().filter(|grade| **grade >= 1).count();
    let relevant_found = top
        .iter()
        .filter(|grade| grade.is_some_and(|g| g >= 1))
        .count();
    QueryMetrics {
        ndcg10: (idcg > 0.0).then(|| dcg / idcg),
        // Fewer than 10 results do not earn the missing positions.
        p10: relevant_found as f64 / K as f64,
        strict_p10: top.iter().filter(|grade| **grade == Some(2)).count() as f64 / K as f64,
        mrr10: top
            .iter()
            .position(|grade| *grade == Some(2))
            .map_or(0.0, |i| 1.0 / (i + 1) as f64),
        pooled_recall10: (relevant_total > 0)
            .then(|| relevant_found as f64 / relevant_total as f64),
        coverage10: if top.is_empty() {
            1.0
        } else {
            top.iter().filter(|grade| grade.is_some()).count() as f64 / top.len() as f64
        },
        returned: ranked.len(),
        has_relevant: relevant_total > 0,
    }
}

/// Macro average over queries. Queries with no known relevant document are counted apart:
/// nDCG, recall and the others mean nothing for them, and a ranked answer to such a query
/// is a false positive.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Summary {
    pub queries: usize,
    pub with_relevant: usize,
    pub ndcg10: Option<f64>,
    pub p10: Option<f64>,
    pub strict_p10: Option<f64>,
    pub mrr10: Option<f64>,
    pub pooled_recall10: Option<f64>,
    pub judgment_coverage10: Option<f64>,
    pub no_answer_queries: usize,
    /// Of those, how many still got results.
    pub no_answer_with_results: usize,
}

pub fn summarize(metrics: &[&QueryMetrics]) -> Summary {
    let relevant: Vec<&&QueryMetrics> = metrics.iter().filter(|m| m.has_relevant).collect();
    let mean = |values: Vec<f64>| {
        (!values.is_empty()).then(|| values.iter().sum::<f64>() / values.len() as f64)
    };
    let no_answer: Vec<&&QueryMetrics> = metrics.iter().filter(|m| !m.has_relevant).collect();
    Summary {
        queries: metrics.len(),
        with_relevant: relevant.len(),
        ndcg10: mean(relevant.iter().filter_map(|m| m.ndcg10).collect()),
        p10: mean(relevant.iter().map(|m| m.p10).collect()),
        strict_p10: mean(relevant.iter().map(|m| m.strict_p10).collect()),
        mrr10: mean(relevant.iter().map(|m| m.mrr10).collect()),
        pooled_recall10: mean(relevant.iter().filter_map(|m| m.pooled_recall10).collect()),
        judgment_coverage10: mean(metrics.iter().map(|m| m.coverage10).collect()),
        no_answer_queries: no_answer.len(),
        no_answer_with_results: no_answer.iter().filter(|m| m.returned > 0).count(),
    }
}

/// Paired bootstrap of the mean difference `b - a`. `pairs` are `(group, a, b)` per query;
/// whole groups are resampled, because paraphrases of one need are not independent.
/// Returns `(mean difference, 95% interval low, high)`; the seed makes it reproducible.
pub fn bootstrap_diff(
    pairs: &[(String, f64, f64)],
    seed: u64,
    rounds: usize,
) -> Option<(f64, f64, f64)> {
    if pairs.is_empty() {
        return None;
    }
    let mut groups: Vec<(&str, Vec<f64>)> = Vec::new();
    for (group, a, b) in pairs {
        match groups.iter_mut().find(|(name, _)| name == group) {
            Some((_, diffs)) => diffs.push(b - a),
            None => groups.push((group, vec![b - a])),
        }
    }
    let observed = pairs.iter().map(|(_, a, b)| b - a).sum::<f64>() / pairs.len() as f64;
    let mut state = seed.max(1);
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    let mut means = Vec::with_capacity(rounds);
    for _ in 0..rounds {
        let (mut sum, mut count) = (0.0, 0usize);
        for _ in 0..groups.len() {
            let (_, diffs) = &groups[(next() % groups.len() as u64) as usize];
            sum += diffs.iter().sum::<f64>();
            count += diffs.len();
        }
        means.push(sum / count as f64);
    }
    means.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let at = |q: f64| means[((rounds as f64 - 1.0) * q).round() as usize];
    Some((observed, at(0.025), at(0.975)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn docs(ids: &[&str]) -> Vec<String> {
        ids.iter().map(|id| (*id).to_owned()).collect()
    }

    fn judged(pairs: &[(&str, u8)]) -> HashMap<String, u8> {
        pairs
            .iter()
            .map(|(doc, grade)| ((*doc).to_owned(), *grade))
            .collect()
    }

    #[test]
    fn a_perfect_ranking_scores_one() {
        let m = evaluate_query(
            &docs(&["a", "b", "c"]),
            &judged(&[("a", 2), ("b", 1), ("c", 0)]),
        );
        assert_eq!(m.ndcg10, Some(1.0));
        assert_eq!(m.mrr10, 1.0);
        assert_eq!(m.p10, 0.2);
        assert_eq!(m.strict_p10, 0.1);
        assert_eq!(m.pooled_recall10, Some(1.0));
        assert_eq!(m.coverage10, 1.0);
    }

    #[test]
    fn ndcg_uses_graded_gain_and_log_discount() {
        // Relevant at rank 2 (grade 2): DCG = 3/log2(3); ideal = 3/log2(2) = 3.
        let m = evaluate_query(&docs(&["x", "a"]), &judged(&[("a", 2)]));
        assert!((m.ndcg10.unwrap() - 1.0 / 3f64.log2()).abs() < 1e-12);
        assert_eq!(m.mrr10, 0.5);
    }

    #[test]
    fn unjudged_documents_add_nothing_and_lower_coverage() {
        let m = evaluate_query(&docs(&["u", "a"]), &judged(&[("a", 1)]));
        assert_eq!(m.coverage10, 0.5);
        assert_eq!(m.mrr10, 0.0, "MRR needs a grade 2 document");
        assert_eq!(m.p10, 0.1);
    }

    #[test]
    fn fewer_than_ten_results_do_not_earn_the_missing_positions() {
        let m = evaluate_query(&docs(&["a"]), &judged(&[("a", 2)]));
        assert_eq!(m.p10, 0.1);
        assert_eq!(m.returned, 1);
    }

    #[test]
    fn queries_without_relevant_documents_are_counted_apart() {
        let none = evaluate_query(&docs(&["a"]), &judged(&[("a", 0)]));
        assert_eq!(none.ndcg10, None);
        assert!(!none.has_relevant);
        let empty = evaluate_query(&[], &HashMap::new());
        let good = evaluate_query(&docs(&["a"]), &judged(&[("a", 2)]));
        let summary = summarize(&[&none, &empty, &good]);
        assert_eq!((summary.queries, summary.with_relevant), (3, 1));
        assert_eq!(
            (summary.no_answer_queries, summary.no_answer_with_results),
            (2, 1)
        );
        assert_eq!(summary.ndcg10, Some(1.0));
    }

    #[test]
    fn bootstrap_is_reproducible_and_brackets_the_difference() {
        let pairs: Vec<(String, f64, f64)> = (0..20)
            .map(|i| (format!("g{i}"), 0.4, 0.6 + f64::from(i % 3) * 0.01))
            .collect();
        let first = bootstrap_diff(&pairs, 7, 1000).unwrap();
        assert_eq!(first, bootstrap_diff(&pairs, 7, 1000).unwrap());
        assert!(first.1 <= first.0 && first.0 <= first.2);
        assert!(first.1 > 0.0, "a consistent improvement excludes zero");
        assert!(bootstrap_diff(&[], 7, 10).is_none());
    }
}
