use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fs, path::Path};

/// One line of `queries.jsonl`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Query {
    pub query_id: String,
    pub text: String,
    /// What the query tests: `rfc_id`, `identifier`, `prose`, `typo`, `synonym`, `word_form`,
    /// `filter`, `no_answer`.
    pub intent: String,
    /// `en` or `ru`.
    pub language: String,
    #[serde(default)]
    pub filters: QueryFilters,
    /// `dev` or `test`. Paraphrases of one need share a `group`, hence a split.
    pub split: String,
    pub group: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct QueryFilters {
    pub source: Option<String>,
    #[serde(default)]
    pub complete_only: bool,
}

/// `query_id -> document_id -> grade (0..=2)`. A pair that is absent is unjudged, not 0.
pub type Qrels = HashMap<String, HashMap<String, u8>>;

#[derive(Debug, Clone)]
pub struct RunRow {
    pub query_id: String,
    pub document_id: String,
    pub rank: usize,
    pub score: f32,
}

pub fn read_queries(path: &Path) -> anyhow::Result<Vec<Query>> {
    let mut queries = Vec::new();
    for (n, line) in fs::read_to_string(path)?.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        queries.push(
            serde_json::from_str(line)
                .map_err(|e| anyhow::anyhow!("{}:{}: {e}", path.display(), n + 1))?,
        );
    }
    Ok(queries)
}

pub fn read_qrels(path: &Path) -> anyhow::Result<Qrels> {
    let mut qrels = Qrels::new();
    for (n, line) in fs::read_to_string(path)?.lines().enumerate() {
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split('\t');
        let (Some(query), Some(document), Some(grade), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            anyhow::bail!(
                "{}:{}: expected query_id<TAB>document_id<TAB>grade",
                path.display(),
                n + 1
            );
        };
        let grade: u8 = grade.parse()?;
        anyhow::ensure!(
            grade <= 2,
            "{}:{}: grade must be 0, 1 or 2",
            path.display(),
            n + 1
        );
        qrels
            .entry(query.into())
            .or_default()
            .insert(document.into(), grade);
    }
    Ok(qrels)
}

pub fn read_run(path: &Path) -> anyhow::Result<Vec<RunRow>> {
    let mut rows = Vec::new();
    for (n, line) in fs::read_to_string(path)?.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split('\t').collect();
        let [query, document, rank, score] = parts.as_slice() else {
            anyhow::bail!(
                "{}:{}: expected query_id, document_id, rank, score",
                path.display(),
                n + 1
            );
        };
        rows.push(RunRow {
            query_id: (*query).into(),
            document_id: (*document).into(),
            rank: rank.parse()?,
            score: score.parse()?,
        });
    }
    Ok(rows)
}

/// Ranked document IDs per query, best first.
pub fn group_runs(rows: &[RunRow]) -> HashMap<String, Vec<String>> {
    let mut by_query: HashMap<String, Vec<(usize, String)>> = HashMap::new();
    for row in rows {
        by_query
            .entry(row.query_id.clone())
            .or_default()
            .push((row.rank, row.document_id.clone()));
    }
    by_query
        .into_iter()
        .map(|(query, mut docs)| {
            docs.sort();
            (query, docs.into_iter().map(|(_, doc)| doc).collect())
        })
        .collect()
}
