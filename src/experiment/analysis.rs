//! Analysis of the recorded events. The unit is the session, never the single query: queries of
//! one session are dependent, and the arm is assigned per session. Everything is deterministic
//! (fixed bootstrap seed), so a report can be reproduced from the same events.

use super::{Config, REFORMULATION_WINDOW_SECS};
use crate::models::events::EventRow;
use serde::Serialize;
use std::collections::HashMap;

const BOOTSTRAP_ROUNDS: usize = 2000;
const SEED: u64 = 20_241_105;
/// A slice (query type) is reported only when both arms have at least this many sessions.
pub const MIN_SLICE_SESSIONS: usize = 100;
/// SRM: a split this unlikely means the assignment or the logging is broken.
const SRM_ALPHA: f64 = 0.001;

#[derive(Debug, Clone, Default)]
struct Query {
    at: i64,
    text: String,
    results: i64,
    latency_ms: Option<i64>,
    degraded: bool,
    clicks: Vec<(i64, u32)>,
    reformulated: bool,
}

impl Query {
    fn last_activity(&self) -> i64 {
        self.clicks
            .iter()
            .map(|c| c.0)
            .max()
            .unwrap_or(self.at)
            .max(self.at)
    }

    fn clicked(&self) -> bool {
        !self.clicks.is_empty()
    }

    fn first_click_rank(&self) -> Option<u32> {
        self.clicks.iter().min_by_key(|c| c.0).map(|c| c.1)
    }

    /// A click and no immediate reformulation.
    fn success(&self) -> bool {
        self.clicked() && !self.reformulated
    }
}

#[derive(Debug, Clone)]
struct Session {
    treatment: bool,
    kind: &'static str,
    queries: Vec<Query>,
}

impl Session {
    fn share(&self, f: impl Fn(&Query) -> bool) -> f64 {
        self.queries.iter().filter(|q| f(q)).count() as f64 / self.queries.len() as f64
    }

    fn success(&self) -> f64 {
        f64::from(self.queries.iter().any(Query::success))
    }
}

/// What kind of information need the first query of a session expresses.
pub fn query_kind(text: &str) -> &'static str {
    let lower = text.trim().to_lowercase();
    let digits = lower
        .strip_prefix("rfc")
        .unwrap_or(&lower)
        .trim_start_matches([':', ' ']);
    if lower.chars().any(|c| ('а'..='я').contains(&c) || c == 'ё') {
        "russian"
    } else if !digits.is_empty() && digits.len() <= 4 && digits.bytes().all(|b| b.is_ascii_digit())
    {
        "rfc_id"
    } else if lower.contains("::") || lower.contains('_') || lower.contains('(') {
        "identifier"
    } else {
        "english_prose"
    }
}

fn normalize(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn sessions(config: &Config, events: &[EventRow]) -> (Vec<Session>, usize) {
    let mut by_session: HashMap<&str, Vec<&EventRow>> = HashMap::new();
    let mut ignored = 0;
    for event in events {
        if config.variant(&event.variant).is_some() {
            by_session.entry(&event.session_id).or_default().push(event);
        } else {
            ignored += 1;
        }
    }
    let mut names: Vec<&str> = by_session.keys().copied().collect();
    names.sort_unstable();
    let mut out = Vec::new();
    for name in names {
        let mut rows = by_session.remove(name).unwrap_or_default();
        rows.sort_by_key(|e| e.at);
        let treatment = rows[0].variant == config.treatment.name;
        let mut queries: Vec<(String, Query)> = Vec::new();
        for row in rows {
            let at = row.at.timestamp();
            if row.kind == "search" {
                queries.push((
                    row.query_id.clone(),
                    Query {
                        at,
                        text: row.query.clone().unwrap_or_default(),
                        results: i64::from(row.results.unwrap_or(0)),
                        latency_ms: row.latency_ms.map(i64::from),
                        degraded: row.degraded.unwrap_or(false),
                        ..Query::default()
                    },
                ));
            } else if let Some((_, query)) = queries.iter_mut().find(|(id, _)| *id == row.query_id)
            {
                query.clicks.push((at, row.rank.unwrap_or(1).max(1) as u32));
            }
        }
        let mut queries: Vec<Query> = queries.into_iter().map(|(_, q)| q).collect();
        for i in 0..queries.len().saturating_sub(1) {
            let next = &queries[i + 1];
            let changed = normalize(&next.text) != normalize(&queries[i].text);
            let soon = next.at - queries[i].last_activity() <= REFORMULATION_WINDOW_SECS;
            queries[i].reformulated = changed && soon;
        }
        if queries.is_empty() {
            continue;
        }
        out.push(Session {
            treatment,
            kind: query_kind(&queries[0].text),
            queries,
        });
    }
    (out, ignored)
}

// ---- statistics ----------------------------------------------------------------------------

fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        f64::NAN
    } else {
        values.iter().sum::<f64>() / values.len() as f64
    }
}

fn variance(values: &[f64]) -> f64 {
    let m = mean(values);
    values.iter().map(|v| (v - m).powi(2)).sum::<f64>() / (values.len() as f64 - 1.0)
}

/// Complementary error function (Abramowitz & Stegun 7.1.26, error < 1.5e-7).
fn erfc(x: f64) -> f64 {
    let z = x.abs();
    let t = 1.0 / (1.0 + 0.327_591_1 * z);
    let poly = t
        * (0.254_829_592
            + t * (-0.284_496_736
                + t * (1.421_413_741 + t * (-1.453_152_027 + t * 1.061_405_429))));
    let value = poly * (-z * z).exp();
    if x >= 0.0 { value } else { 2.0 - value }
}

fn normal_cdf(x: f64) -> f64 {
    0.5 * erfc(-x / std::f64::consts::SQRT_2)
}

/// Two-sided p of a Welch z statistic for `treatment - control`.
fn welch_p(control: &[f64], treatment: &[f64]) -> Option<f64> {
    if control.len() < 2 || treatment.len() < 2 {
        return None;
    }
    let se = (variance(control) / control.len() as f64
        + variance(treatment) / treatment.len() as f64)
        .sqrt();
    if se == 0.0 {
        return Some(if mean(control) == mean(treatment) {
            1.0
        } else {
            0.0
        });
    }
    let z = (mean(treatment) - mean(control)) / se;
    Some((2.0 * (1.0 - normal_cdf(z.abs()))).clamp(0.0, 1.0))
}

fn bootstrap_ci(control: &[f64], treatment: &[f64]) -> Option<(f64, f64)> {
    if control.is_empty() || treatment.is_empty() {
        return None;
    }
    let mut state = SEED;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    let resampled = |values: &[f64], next: &mut dyn FnMut() -> u64| {
        let sum: f64 = (0..values.len())
            .map(|_| values[(next() % values.len() as u64) as usize])
            .sum();
        sum / values.len() as f64
    };
    let mut diffs: Vec<f64> = (0..BOOTSTRAP_ROUNDS)
        .map(|_| resampled(treatment, &mut next) - resampled(control, &mut next))
        .collect();
    diffs.sort_by(|a, b| a.partial_cmp(b).expect("finite"));
    let at = |q: f64| diffs[((BOOTSTRAP_ROUNDS as f64 - 1.0) * q).round() as usize];
    Some((at(0.025), at(0.975)))
}

/// Chi-square goodness of fit of the observed split against the planned share (1 df).
pub fn srm(control: usize, treatment: usize, treatment_share: f64) -> (f64, f64) {
    let total = (control + treatment) as f64;
    let (ec, et) = (total * (1.0 - treatment_share), total * treatment_share);
    let chi2 = (control as f64 - ec).powi(2) / ec + (treatment as f64 - et).powi(2) / et;
    (chi2, erfc((chi2 / 2.0).sqrt()))
}

fn normal_quantile(p: f64) -> f64 {
    let (mut lo, mut hi) = (-10.0, 10.0);
    for _ in 0..80 {
        let mid = (lo + hi) / 2.0;
        if normal_cdf(mid) < p {
            lo = mid
        } else {
            hi = mid
        }
    }
    (lo + hi) / 2.0
}

/// Sessions per arm needed to see an absolute change `delta` of a rate near `baseline`.
pub fn required_per_arm(baseline: f64, delta: f64, alpha: f64, power: f64) -> usize {
    let p1 = baseline.clamp(0.01, 0.99);
    let p2 = (p1 + delta).clamp(0.01, 0.99);
    let z = normal_quantile(1.0 - alpha / 2.0) + normal_quantile(power);
    let variance = p1 * (1.0 - p1) + p2 * (1.0 - p2);
    (z * z * variance / delta.powi(2)).ceil() as usize
}

// ---- report --------------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct Metric {
    pub name: &'static str,
    /// `primary`, `secondary` or `guardrail`.
    pub role: &'static str,
    /// Higher is better, lower is better, or neither (a guardrail to watch).
    pub better: &'static str,
    pub control: f64,
    pub treatment: f64,
    pub difference: f64,
    pub ci_low: Option<f64>,
    pub ci_high: Option<f64>,
    pub p_value: Option<f64>,
    /// Bonferroni over the secondary metrics; the primary is not adjusted.
    pub p_adjusted: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct Slice {
    pub kind: &'static str,
    pub control_sessions: usize,
    pub treatment_sessions: usize,
    /// `null` when a slice has too few sessions in an arm to say anything.
    pub success: Option<Metric>,
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub experiment: &'static str,
    pub version: u32,
    pub source: String,
    pub control: &'static str,
    pub treatment: &'static str,
    pub control_sessions: usize,
    pub treatment_sessions: usize,
    pub ignored_events: usize,
    pub srm_chi2: f64,
    pub srm_p: f64,
    pub srm_ok: bool,
    pub required_per_arm: usize,
    pub metrics: Vec<Metric>,
    pub control_p95_latency_ms: Option<i64>,
    pub treatment_p95_latency_ms: Option<i64>,
    pub slices: Vec<Slice>,
    pub verdict: String,
}

type Def = (
    &'static str,
    &'static str,
    &'static str,
    Box<dyn Fn(&Session) -> f64>,
);

fn metric(
    name: &'static str,
    role: &'static str,
    better: &'static str,
    control: &[f64],
    treatment: &[f64],
) -> Metric {
    let ci = bootstrap_ci(control, treatment);
    Metric {
        name,
        role,
        better,
        control: mean(control),
        treatment: mean(treatment),
        difference: mean(treatment) - mean(control),
        ci_low: ci.map(|c| c.0),
        ci_high: ci.map(|c| c.1),
        p_value: welch_p(control, treatment),
        p_adjusted: None,
    }
}

fn p95(mut values: Vec<i64>) -> Option<i64> {
    if values.is_empty() {
        return None;
    }
    values.sort_unstable();
    Some(values[(values.len() as f64 * 0.95).ceil() as usize - 1])
}

pub fn analyze(config: &Config, events: &[EventRow], source: &str) -> Report {
    let (all, ignored) = sessions(config, events);
    let (treated, control): (Vec<&Session>, Vec<&Session>) = all.iter().partition(|s| s.treatment);
    let share = f64::from(config.treatment_share_bp) / 10_000.0;
    let (chi2, srm_p) = if all.is_empty() {
        (0.0, 1.0)
    } else {
        srm(control.len(), treated.len(), share)
    };
    let column = |group: &[&Session], f: &dyn Fn(&Session) -> f64| -> Vec<f64> {
        group.iter().map(|s| f(s)).collect()
    };
    let mrr = |s: &Session| {
        mean(
            &s.queries
                .iter()
                .map(|q| q.first_click_rank().map_or(0.0, |r| 1.0 / f64::from(r)))
                .collect::<Vec<_>>(),
        )
    };
    let defs: Vec<Def> = vec![
        (
            "session success rate",
            "primary",
            "higher",
            Box::new(Session::success),
        ),
        (
            "MRR of the first click",
            "secondary",
            "higher",
            Box::new(mrr),
        ),
        (
            "CTR@3",
            "secondary",
            "higher",
            Box::new(|s| s.share(|q| q.clicks.iter().any(|c| c.1 <= 3))),
        ),
        (
            "CTR@10",
            "secondary",
            "higher",
            Box::new(|s| s.share(|q| q.clicks.iter().any(|c| c.1 <= 10))),
        ),
        (
            "abandonment (queries without a click)",
            "secondary",
            "lower",
            Box::new(|s| s.share(|q| !q.clicked())),
        ),
        (
            "reformulation rate",
            "secondary",
            "lower",
            Box::new(|s| s.share(|q| q.reformulated)),
        ),
        (
            "zero-result rate",
            "guardrail",
            "lower",
            Box::new(|s| s.share(|q| q.results == 0)),
        ),
        (
            "degraded answers",
            "guardrail",
            "lower",
            Box::new(|s| s.share(|q| q.degraded)),
        ),
    ];
    let mut metrics: Vec<Metric> = defs
        .iter()
        .map(|(name, role, better, f)| {
            metric(
                name,
                role,
                better,
                &column(&control, f),
                &column(&treated, f),
            )
        })
        .collect();
    let secondary = metrics.iter().filter(|m| m.role == "secondary").count() as f64;
    for m in metrics.iter_mut().filter(|m| m.role == "secondary") {
        m.p_adjusted = m.p_value.map(|p| (p * secondary).min(1.0));
    }

    let latencies = |group: &[&Session]| {
        p95(group
            .iter()
            .flat_map(|s| s.queries.iter().filter_map(|q| q.latency_ms))
            .collect())
    };
    let slices = ["rfc_id", "identifier", "english_prose", "russian"]
        .into_iter()
        .map(|kind| {
            let c: Vec<f64> = control
                .iter()
                .filter(|s| s.kind == kind)
                .map(|s| s.success())
                .collect();
            let t: Vec<f64> = treated
                .iter()
                .filter(|s| s.kind == kind)
                .map(|s| s.success())
                .collect();
            let enough = c.len() >= MIN_SLICE_SESSIONS && t.len() >= MIN_SLICE_SESSIONS;
            Slice {
                kind,
                control_sessions: c.len(),
                treatment_sessions: t.len(),
                success: enough
                    .then(|| metric("session success rate", "secondary", "higher", &c, &t)),
            }
        })
        .collect();

    let baseline = metrics[0].control;
    let baseline = if baseline.is_nan() { 0.5 } else { baseline };
    let required = required_per_arm(baseline, config.minimum_effect, config.alpha, config.power);
    let srm_ok = srm_p >= SRM_ALPHA;
    let primary = &metrics[0];
    let verdict = verdict(
        config,
        primary,
        srm_ok,
        srm_p,
        control.len().min(treated.len()),
        required,
    );
    Report {
        experiment: config.id,
        version: config.version,
        source: source.to_owned(),
        control: config.control.name,
        treatment: config.treatment.name,
        control_sessions: control.len(),
        treatment_sessions: treated.len(),
        ignored_events: ignored,
        srm_chi2: chi2,
        srm_p,
        srm_ok,
        required_per_arm: required,
        control_p95_latency_ms: latencies(&control),
        treatment_p95_latency_ms: latencies(&treated),
        metrics,
        slices,
        verdict,
    }
}

fn verdict(
    config: &Config,
    primary: &Metric,
    srm_ok: bool,
    srm_p: f64,
    smaller_arm: usize,
    required: usize,
) -> String {
    if smaller_arm < 2 {
        return "No conclusion: fewer than two sessions in an arm.".into();
    }
    if !srm_ok {
        return format!(
            "Do not interpret: the traffic split deviates from the plan (SRM p = {srm_p:.2e}); assignment or logging is broken."
        );
    }
    let (low, high) = (
        primary.ci_low.unwrap_or(f64::NAN),
        primary.ci_high.unwrap_or(f64::NAN),
    );
    let excludes_zero = low > 0.0 || high < 0.0;
    let enough = smaller_arm >= required;
    let size = format!(
        "{smaller_arm} sessions in the smaller arm, {required} needed to detect {:.0} points",
        config.minimum_effect * 100.0
    );
    match (enough, excludes_zero) {
        (false, false) => format!(
            "Insufficient sample ({size}); the interval includes zero. No conclusion either way."
        ),
        (false, true) => format!(
            "Preliminary only ({size}): the interval excludes zero (difference {:+.3}, [{low:+.3}, {high:+.3}]) but the sample is below the plan; repeat with more traffic before relying on it.",
            primary.difference
        ),
        (true, false) => format!(
            "No difference detected ({size}): the interval [{low:+.3}, {high:+.3}] includes zero, so an effect of {:.0} points or more is unlikely.",
            config.minimum_effect * 100.0
        ),
        (true, true) if primary.difference > 0.0 => format!(
            "{} is better on the primary metric ({size}): {:+.3} [{low:+.3}, {high:+.3}].",
            config.treatment.name, primary.difference
        ),
        (true, true) => format!(
            "{} is worse on the primary metric ({size}): {:+.3} [{low:+.3}, {high:+.3}].",
            config.treatment.name, primary.difference
        ),
    }
}

fn cell(value: Option<f64>) -> String {
    value.map_or("—".into(), |v| format!("{v:+.3}"))
}

pub fn markdown(report: &Report, offline: Option<&str>) -> String {
    let mut out = format!(
        "# Online evaluation: {} v{}\n\nData: {}\n\n- control `{}`: {} sessions; treatment `{}`: {} sessions; events of unknown variants ignored: {}\n- SRM chi² = {:.3}, p = {:.4} ({})\n- sessions per arm needed for the planned effect: {}\n- p95 latency: control {} ms, treatment {} ms\n\n## Verdict\n\n{}\n\n",
        report.experiment,
        report.version,
        report.source,
        report.control,
        report.control_sessions,
        report.treatment,
        report.treatment_sessions,
        report.ignored_events,
        report.srm_chi2,
        report.srm_p,
        if report.srm_ok {
            "split matches the plan"
        } else {
            "SRM: split broken"
        },
        report.required_per_arm,
        report
            .control_p95_latency_ms
            .map_or("—".into(), |v| v.to_string()),
        report
            .treatment_p95_latency_ms
            .map_or("—".into(), |v| v.to_string()),
        report.verdict,
    );
    out.push_str("## Metrics (unit: session; 95% bootstrap interval of treatment − control)\n\n| metric | role | control | treatment | difference | interval | p | p adj. |\n|---|---|---|---|---|---|---|---|\n");
    for m in &report.metrics {
        out.push_str(&format!(
            "| {} | {} | {:.3} | {:.3} | {:+.3} | [{}, {}] | {} | {} |\n",
            m.name,
            m.role,
            m.control,
            m.treatment,
            m.difference,
            cell(m.ci_low),
            cell(m.ci_high),
            m.p_value.map_or("—".into(), |p| format!("{p:.3}")),
            m.p_adjusted.map_or("—".into(), |p| format!("{p:.3}")),
        ));
    }
    out.push_str("\nSecondary p values are Bonferroni-adjusted; the primary metric is not. Guardrails are watched, not tested.\n\n## Slices (first query of the session; reported with at least ");
    out.push_str(&format!("{MIN_SLICE_SESSIONS} sessions per arm)\n\n| slice | control | treatment | success difference | interval |\n|---|---|---|---|---|\n"));
    for s in &report.slices {
        match &s.success {
            Some(m) => out.push_str(&format!(
                "| {} | {} | {} | {:+.3} | [{}, {}] |\n",
                s.kind,
                s.control_sessions,
                s.treatment_sessions,
                m.difference,
                cell(m.ci_low),
                cell(m.ci_high)
            )),
            None => out.push_str(&format!(
                "| {} | {} | {} | not enough sessions | — |\n",
                s.kind, s.control_sessions, s.treatment_sessions
            )),
        }
    }
    if let Some(offline) = offline {
        let primary = &report.metrics[0];
        out.push_str(&format!("\n## Agreement with the offline result\n\nOffline: {offline}. Online primary metric: {:+.3}. ", primary.difference));
        out.push_str("Clicks are biased by position and snippet; a higher click rate is not the same as higher relevance, and a disagreement is reported as found.\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::experiment::EXPERIMENT;
    use chrono::{TimeZone, Utc};

    fn event(
        session: &str,
        variant: &str,
        kind: &str,
        query_id: &str,
        text: &str,
        secs: i64,
        rank: i32,
    ) -> EventRow {
        EventRow {
            at: Utc.timestamp_opt(1_700_000_000 + secs, 0).unwrap(),
            kind: kind.into(),
            session_id: session.into(),
            variant: variant.into(),
            query_id: query_id.into(),
            query: (kind == "search").then(|| text.into()),
            results: (kind == "search").then_some(10),
            latency_ms: (kind == "search").then_some(40),
            degraded: (kind == "search").then_some(false),
            document_id: (kind == "click").then(|| "rfc:1".into()),
            rank: (kind == "click").then_some(rank),
        }
    }

    #[test]
    fn a_click_followed_by_a_new_query_is_not_a_success() {
        let events = vec![
            event("s1", "b0", "search", "q1", "borrow", 0, 0),
            event("s1", "b0", "click", "q1", "", 5, 2),
            event("s1", "b0", "search", "q2", "borrow checker", 20, 0),
        ];
        let (sessions, _) = sessions(&EXPERIMENT, &events);
        assert!(sessions[0].queries[0].reformulated);
        assert_eq!(sessions[0].success(), 0.0);
    }

    #[test]
    fn a_click_with_nothing_after_it_is_a_success_and_a_repeat_is_not_a_reformulation() {
        let events = vec![
            event("s1", "b0", "search", "q1", "Borrow", 0, 0),
            event("s1", "b0", "search", "q2", " borrow ", 3, 0),
            event("s1", "b0", "click", "q2", "", 8, 1),
        ];
        let (sessions, _) = sessions(&EXPERIMENT, &events);
        assert!(!sessions[0].queries[0].reformulated);
        assert_eq!(sessions[0].success(), 1.0);
    }

    #[test]
    fn srm_flags_a_broken_split_and_accepts_a_fair_one() {
        assert!(srm(5000, 5040, 0.5).1 > 0.5);
        assert!(srm(4000, 6000, 0.5).1 < 1e-10);
    }

    #[test]
    fn the_normal_tail_matches_known_values() {
        assert!((normal_cdf(1.96) - 0.975).abs() < 1e-4);
        assert!((normal_quantile(0.975) - 1.96).abs() < 1e-3);
    }

    #[test]
    fn sample_size_matches_the_textbook_figure() {
        // 0.50 -> 0.55 at alpha 0.05 and power 0.8 needs about 1565 per arm.
        let n = required_per_arm(0.5, 0.05, 0.05, 0.8);
        assert!((1500..1650).contains(&n), "n = {n}");
    }

    #[test]
    fn query_kinds() {
        assert_eq!(query_kind("RFC 3058"), "rfc_id");
        assert_eq!(query_kind("3058"), "rfc_id");
        assert_eq!(query_kind("std::mem::swap"), "identifier");
        assert_eq!(query_kind("как работает borrow checker"), "russian");
        assert_eq!(query_kind("async traits"), "english_prose");
    }
}
