//! Offline search evaluation. Reads the database and the search index; never the sources.
//!
//!   eval rebuild                          rebuild the index from the database
//!   eval run   --variant b0               run every query, write eval/runs/<variant>.tsv + .meta.json
//!   eval pool  --runs a.tsv,b.tsv         write eval/pool.jsonl for judging (shuffled, no scores)
//!   eval qrels                            merge eval/judgments.jsonl into eval/qrels.tsv
//!   eval score --variant b0               metrics -> eval/reports/<variant>.{json,md}
//!   eval compare --a b0 --b typo          paired bootstrap of nDCG@10, by query group
//!   eval review  --variant b0 --n 20      the top 10 of N test queries for a human to judge blind
//!   eval agreement                        silver vs human judgments: agreement and Cohen's kappa
use anyhow::{Context, Result, bail, ensure};
use rust_knowledge::{
    evaluation::{
        Query, QueryMetrics, Summary, bootstrap_diff, evaluate_query, group_runs, read_qrels,
        read_queries, read_run, summarize,
    },
    search::{
        ANALYZER_VERSION, Filters, Lemmas, Options, SYNONYMS_VERSION, SearchHandle,
        semantic::{Mode, Request, Semantic, answer},
    },
    use_cases::search::rebuild,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::postgres::PgPoolOptions;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fs,
    path::{Path, PathBuf},
};

const DEPTH: usize = 20;
const EVAL: &str = "eval";

fn main() -> Result<()> {
    let _ = dotenvy::from_path(".env");
    let mut args = std::env::args().skip(1);
    let command = args.next().unwrap_or_default();
    let options = parse_options(args.collect())?;
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        match command.as_str() {
            "rebuild" => rebuild_index().await,
            "run" => run(&options).await,
            "pool" => pool(&options),
            "qrels" => qrels(),
            "score" => score(&options),
            "compare" => compare(&options),
            "review" => review(&options),
            "agreement" => agreement(),
            _ => bail!(
                "usage: eval rebuild | run | pool | qrels | score | compare (see the file header)"
            ),
        }
    })
}

fn parse_options(args: Vec<String>) -> Result<HashMap<String, String>> {
    let mut options = HashMap::new();
    let mut iter = args.into_iter();
    while let Some(flag) = iter.next() {
        let name = flag
            .strip_prefix("--")
            .with_context(|| format!("unexpected argument {flag}"))?;
        options.insert(
            name.to_owned(),
            iter.next()
                .with_context(|| format!("--{name} needs a value"))?,
        );
    }
    Ok(options)
}

fn option<'a>(options: &'a HashMap<String, String>, name: &str) -> Result<&'a str> {
    options
        .get(name)
        .map(String::as_str)
        .with_context(|| format!("--{name} is required"))
}

fn handle() -> SearchHandle {
    SearchHandle::open(std::env::var("SEARCH_INDEX_DIR").unwrap_or_else(|_| "data/index".into()))
}

async fn pool_connection() -> Result<sqlx::PgPool> {
    Ok(PgPoolOptions::new()
        .max_connections(2)
        .connect(&std::env::var("DATABASE_URL").context("DATABASE_URL is required")?)
        .await?)
}

async fn rebuild_index() -> Result<()> {
    let pool = pool_connection().await?;
    let handle = handle();
    let mut tx = pool
        .begin_with("BEGIN ISOLATION LEVEL REPEATABLE READ, READ ONLY")
        .await?;
    let result = rebuild::execute(&mut tx, &handle).await?;
    println!("{result:?}");
    Ok(())
}

/// A fingerprint of the corpus the index was built from: every document and its content hash.
async fn corpus_fingerprint(pool: &sqlx::PgPool) -> Result<(String, i64)> {
    let rows: Vec<(String, String, String)> = sqlx::query_as(
        r#"SELECT source, native_id, content_hash FROM documents
           ORDER BY source COLLATE "C", native_id COLLATE "C""#,
    )
    .fetch_all(pool)
    .await?;
    let mut hash = Sha256::new();
    for (source, native_id, content) in &rows {
        hash.update(format!("{source}:{native_id} {content}\n"));
    }
    Ok((format!("{:x}", hash.finalize()), rows.len() as i64))
}

async fn run(options: &HashMap<String, String>) -> Result<()> {
    let variant = option(options, "variant")?;
    let queries = read_queries(&PathBuf::from(EVAL).join("queries.jsonl"))?;
    let index = handle()
        .current()
        .context("no index: run `eval rebuild` first")?;
    let pool = pool_connection().await?;
    let (corpus_sha256, documents) = corpus_fingerprint(&pool).await?;
    let mode = match options.get("mode").map(String::as_str).unwrap_or("lexical") {
        "lexical" => Mode::Lexical,
        "vector" => Mode::Vector,
        "hybrid" => Mode::Hybrid,
        other => anyhow::bail!("unknown --mode {other}"),
    };
    let semantic = Semantic::from_env();
    let rewriting = Options {
        fuzzy: options.get("fuzzy").is_some_and(|v| v == "on"),
        synonyms: options.get("synonyms").is_some_and(|v| v == "on"),
        lemmas: options.get("lemmas").is_some_and(|v| v == "on"),
    };
    let mut lines = String::new();
    let mut empty = 0;
    for query in &queries {
        let filters = Filters {
            source: query.filters.source.clone(),
            complete_only: query.filters.complete_only,
        };
        let found = answer(
            &semantic,
            index.clone(),
            Request {
                text: query.text.clone(),
                filters,
                options: rewriting,
                mode,
                offset: 0,
                limit: DEPTH,
            },
        )
        .await;
        let hits = match found {
            Ok(found) => {
                // A run that quietly fell back to lexical would be recorded as the wrong variant.
                if let Some(reason) = &found.degraded {
                    anyhow::bail!("{} degraded: {reason}", query.query_id);
                }
                if !found.rewrite.is_empty() {
                    eprintln!(
                        "{} {:?}: {} -> {:?}",
                        query.query_id,
                        query.text,
                        found
                            .rewrite
                            .corrections
                            .iter()
                            .map(|c| format!("{}>{}", c.from, c.to))
                            .collect::<Vec<_>>()
                            .join(","),
                        found
                            .rewrite
                            .expansions
                            .iter()
                            .map(|e| e.from.as_str())
                            .collect::<Vec<_>>()
                    );
                }
                found.hits
            }
            // A query the engine rejects (no searchable terms) is an empty answer.
            Err(rust_knowledge::search::Error::InvalidQuery(_)) => Vec::new(),
            Err(error) => return Err(error.into()),
        };
        empty += usize::from(hits.is_empty());
        for (i, hit) in hits.iter().enumerate() {
            lines.push_str(&format!(
                "{}\t{}\t{}\t{}\n",
                query.query_id,
                hit.document_id,
                i + 1,
                hit.score
            ));
        }
    }
    let dir = PathBuf::from(EVAL).join("runs");
    fs::create_dir_all(&dir)?;
    fs::write(dir.join(format!("{variant}.tsv")), lines)?;
    let queries_sha = format!(
        "{:x}",
        Sha256::digest(fs::read(PathBuf::from(EVAL).join("queries.jsonl"))?)
    );
    let meta = json!({
        "variant": variant,
        "mode": mode.as_str(),
        "fuzzy": rewriting.fuzzy,
        "synonyms": rewriting.synonyms,
        "synonyms_version": SYNONYMS_VERSION,
        "lemmas": rewriting.lemmas,
        "lemma_table": Lemmas::shared().map(|l| json!({"version": l.version, "sha256": l.sha256})),
        "analyzer_version": ANALYZER_VERSION,
        "index_generation": index.generation(),
        "indexed_documents": index.documents(),
        "corpus_documents": documents,
        "corpus_sha256": corpus_sha256,
        "queries_sha256": queries_sha,
        "queries": queries.len(),
        "depth": DEPTH,
        "queries_without_results": empty,
    });
    fs::write(
        dir.join(format!("{variant}.meta.json")),
        serde_json::to_vec_pretty(&meta)?,
    )?;
    println!("{}", serde_json::to_string_pretty(&meta)?);
    Ok(())
}

/// Union of the top results of the given runs, per query, in a fixed shuffled order with
/// nothing that reveals which system ranked what.
fn pool(options: &HashMap<String, String>) -> Result<()> {
    let seed: u64 = options.get("seed").map_or(Ok(1), |s| s.parse())?;
    let depth: usize = options.get("depth").map_or(Ok(DEPTH), |s| s.parse())?;
    let index = handle()
        .current()
        .context("no index: run `eval rebuild` first")?;
    let queries = read_queries(&PathBuf::from(EVAL).join("queries.jsonl"))?;
    let mut candidates: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for path in option(options, "runs")?.split(',') {
        for (query, docs) in group_runs(&read_run(Path::new(path))?) {
            candidates
                .entry(query)
                .or_default()
                .extend(docs.into_iter().take(depth));
        }
    }
    let mut out = String::new();
    for query in &queries {
        let mut docs: Vec<String> = candidates
            .get(&query.query_id)
            .into_iter()
            .flatten()
            .cloned()
            .collect();
        shuffle(&mut docs, seed ^ fnv(&query.query_id));
        let mut items = Vec::new();
        for id in docs {
            let Some(doc) = index.document(&id)? else {
                continue;
            };
            let text: String = doc
                .body
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .chars()
                .take(700)
                .collect();
            items.push(json!({
                "document_id": doc.document_id, "source": doc.source, "title": doc.title,
                "url": doc.url, "completeness": doc.completeness, "text": text,
            }));
        }
        out.push_str(&serde_json::to_string(&json!({
            "query_id": query.query_id, "text": query.text, "intent": query.intent,
            "language": query.language, "filters": query.filters, "candidates": items,
        }))?);
        out.push('\n');
    }
    fs::write(PathBuf::from(EVAL).join("pool.jsonl"), out)?;
    println!("pool written for {} queries", queries.len());
    Ok(())
}

fn fnv(text: &str) -> u64 {
    text.bytes().fold(0xcbf29ce484222325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x100000001b3)
    })
}

fn shuffle<T>(items: &mut [T], seed: u64) {
    let mut state = seed.max(1);
    for i in (1..items.len()).rev() {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        items.swap(i, (state % (i as u64 + 1)) as usize);
    }
}

/// `judgments.jsonl` holds every judgment with its assessor and evidence; later lines win.
/// `qrels.tsv` is the plain query/document/grade file that scoring reads.
fn qrels() -> Result<()> {
    let text = fs::read_to_string(PathBuf::from(EVAL).join("judgments.jsonl"))?;
    let mut grades: BTreeMap<(String, String), u8> = BTreeMap::new();
    for (n, line) in text
        .lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty())
    {
        let value: Value =
            serde_json::from_str(line).with_context(|| format!("judgments.jsonl:{}", n + 1))?;
        let grade = value["grade"].as_u64().context("grade")? as u8;
        ensure!(grade <= 2, "judgments.jsonl:{}: grade must be 0..=2", n + 1);
        grades.insert(
            (
                value["query_id"].as_str().context("query_id")?.to_owned(),
                value["document_id"]
                    .as_str()
                    .context("document_id")?
                    .to_owned(),
            ),
            grade,
        );
    }
    let out: String = grades
        .iter()
        .map(|((q, d), g)| format!("{q}\t{d}\t{g}\n"))
        .collect();
    fs::write(PathBuf::from(EVAL).join("qrels.tsv"), out)?;
    println!("{} judgments", grades.len());
    Ok(())
}

struct Scored {
    per_query: BTreeMap<String, QueryMetrics>,
}

fn evaluate(variant: &str) -> Result<(Vec<Query>, Scored)> {
    let queries = read_queries(&PathBuf::from(EVAL).join("queries.jsonl"))?;
    let qrels = read_qrels(&PathBuf::from(EVAL).join("qrels.tsv"))?;
    let runs = group_runs(&read_run(
        &PathBuf::from(EVAL)
            .join("runs")
            .join(format!("{variant}.tsv")),
    )?);
    let empty = HashMap::new();
    let per_query = queries
        .iter()
        .map(|q| {
            let ranked = runs.get(&q.query_id).cloned().unwrap_or_default();
            (
                q.query_id.clone(),
                evaluate_query(&ranked, qrels.get(&q.query_id).unwrap_or(&empty)),
            )
        })
        .collect();
    Ok((queries, Scored { per_query }))
}

fn slice<'a>(queries: &'a [Query], scored: &'a Scored, keep: impl Fn(&Query) -> bool) -> Summary {
    let metrics: Vec<&QueryMetrics> = queries
        .iter()
        .filter(|q| keep(q))
        .map(|q| &scored.per_query[&q.query_id])
        .collect();
    summarize(&metrics)
}

fn score(options: &HashMap<String, String>) -> Result<()> {
    let variant = option(options, "variant")?;
    let (queries, scored) = evaluate(variant)?;
    let dir = PathBuf::from(EVAL).join("reports");
    fs::create_dir_all(&dir)?;
    let meta: Value = serde_json::from_slice(&fs::read(
        PathBuf::from(EVAL)
            .join("runs")
            .join(format!("{variant}.meta.json")),
    )?)?;
    let mut slices: BTreeMap<String, Summary> = BTreeMap::new();
    slices.insert("all".into(), slice(&queries, &scored, |_| true));
    for split in ["dev", "test"] {
        slices.insert(
            format!("split={split}"),
            slice(&queries, &scored, |q| q.split == split),
        );
    }
    for intent in queries
        .iter()
        .map(|q| q.intent.clone())
        .collect::<BTreeSet<_>>()
    {
        slices.insert(
            format!("intent={intent}"),
            slice(&queries, &scored, |q| q.intent == intent),
        );
    }
    for language in queries
        .iter()
        .map(|q| q.language.clone())
        .collect::<BTreeSet<_>>()
    {
        slices.insert(
            format!("language={language}"),
            slice(&queries, &scored, |q| q.language == language),
        );
    }
    let report = json!({ "run": meta, "slices": slices, "per_query": scored.per_query });
    fs::write(
        dir.join(format!("{variant}.json")),
        serde_json::to_vec_pretty(&report)?,
    )?;
    let fmt = |v: Option<f64>| v.map_or("—".to_owned(), |v| format!("{v:.3}"));
    let mut md = format!(
        "# Отчёт: {variant}\n\n- корпус: {} документов, sha256 `{}`\n- индекс: поколение `{}`, анализатор `{}`\n- запросов: {}, глубина оценки: top-10\n\n\
         Оценки `llm-silver`, если не указано иное: см. `eval/judgments.jsonl` (assessor, evidence). nDCG, P, MRR, recall считаются по запросам с известными релевантными документами; запросы без них — отдельно.\n\n\
         | срез | запросов | с релевантными | nDCG@10 | P@10 | strict P@10 | MRR@10 | pooled recall@10 | покрытие оценками@10 | без ответа / с выдачей |\n|---|---|---|---|---|---|---|---|---|---|\n",
        meta["corpus_documents"],
        meta["corpus_sha256"].as_str().unwrap_or(""),
        meta["index_generation"].as_str().unwrap_or(""),
        meta["analyzer_version"].as_str().unwrap_or(""),
        queries.len(),
    );
    for (name, s) in &slices {
        md.push_str(&format!(
            "| {name} | {} | {} | {} | {} | {} | {} | {} | {} | {} / {} |\n",
            s.queries,
            s.with_relevant,
            fmt(s.ndcg10),
            fmt(s.p10),
            fmt(s.strict_p10),
            fmt(s.mrr10),
            fmt(s.pooled_recall10),
            fmt(s.judgment_coverage10),
            s.no_answer_queries,
            s.no_answer_with_results,
        ));
    }
    fs::write(dir.join(format!("{variant}.md")), md.as_bytes())?;
    print!("{md}");
    Ok(())
}

fn compare(options: &HashMap<String, String>) -> Result<()> {
    let (a, b) = (option(options, "a")?, option(options, "b")?);
    let seed: u64 = options.get("seed").map_or(Ok(1), |s| s.parse())?;
    let (queries, first) = evaluate(a)?;
    let (_, second) = evaluate(b)?;
    let pairs: Vec<(String, f64, f64)> = queries
        .iter()
        .filter_map(|q| {
            Some((
                q.group.clone(),
                first.per_query[&q.query_id].ndcg10?,
                second.per_query[&q.query_id].ndcg10?,
            ))
        })
        .collect();
    match bootstrap_diff(&pairs, seed, 10_000) {
        Some((diff, low, high)) => println!(
            "nDCG@10 {b} - {a}: {diff:+.4} (95% bootstrap CI by query group, seed {seed}: {low:+.4} .. {high:+.4}), {} queries",
            pairs.len()
        ),
        None => println!("no queries with relevant documents in both runs"),
    }
    Ok(())
}

/// Test-split queries (round-robin over intents) with their top 10, shuffled, for a human
/// to grade without seeing the silver grades. The answers go to `eval/review/human.tsv`.
fn review(options: &HashMap<String, String>) -> Result<()> {
    let variant = option(options, "variant")?;
    let n: usize = options.get("n").map_or(Ok(20), |s| s.parse())?;
    let queries = read_queries(&PathBuf::from(EVAL).join("queries.jsonl"))?;
    let qrels = read_qrels(&PathBuf::from(EVAL).join("qrels.tsv"))?;
    let runs = group_runs(&read_run(
        &PathBuf::from(EVAL)
            .join("runs")
            .join(format!("{variant}.tsv")),
    )?);
    let index = handle()
        .current()
        .context("no index: run `eval rebuild` first")?;
    let mut by_intent: BTreeMap<&str, Vec<&Query>> = BTreeMap::new();
    for q in queries.iter().filter(|q| {
        q.split == "test"
            && qrels
                .get(&q.query_id)
                .is_some_and(|d| d.values().any(|g| *g > 0))
    }) {
        by_intent.entry(&q.intent).or_default().push(q);
    }
    let mut chosen = Vec::new();
    'fill: for round in 0.. {
        let mut any = false;
        for list in by_intent.values() {
            if let Some(q) = list.get(round) {
                any = true;
                chosen.push(*q);
                if chosen.len() == n {
                    break 'fill;
                }
            }
        }
        if !any {
            break;
        }
    }
    let dir = PathBuf::from(EVAL).join("review");
    fs::create_dir_all(&dir)?;
    let (mut items, mut template) = (
        String::new(),
        String::from("query_id\tdocument_id\tgrade\n"),
    );
    for q in &chosen {
        let mut docs: Vec<String> = runs
            .get(&q.query_id)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .take(10)
            .collect();
        shuffle(&mut docs, 7 ^ fnv(&q.query_id));
        let mut candidates = Vec::new();
        for id in docs {
            let Some(doc) = index.document(&id)? else {
                continue;
            };
            let text: String = doc
                .body
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .chars()
                .take(600)
                .collect();
            template.push_str(&format!("{}\t{}\t\n", q.query_id, id));
            candidates
                .push(json!({"document_id": id, "title": doc.title, "url": doc.url, "text": text}));
        }
        items.push_str(&serde_json::to_string(&json!({"query_id": q.query_id, "query": q.text, "filters": q.filters, "candidates": candidates}))?);
        items.push('\n');
    }
    fs::write(dir.join("holdout.jsonl"), items)?;
    fs::write(dir.join("human.tsv"), template)?;
    println!(
        "{} queries; grade 0/1/2 in eval/review/human.tsv (0 not relevant, 1 related, 2 answers the need)",
        chosen.len()
    );
    Ok(())
}

/// How far the silver (model) grades agree with the human ones on the pairs both judged.
fn agreement() -> Result<()> {
    let silver = read_qrels(&PathBuf::from(EVAL).join("qrels.tsv"))?;
    let mut counts = [[0usize; 3]; 3];
    for line in fs::read_to_string(PathBuf::from(EVAL).join("review/human.tsv"))?
        .lines()
        .skip(1)
    {
        let parts: Vec<&str> = line.split('\t').collect();
        let [query, document, grade] = parts.as_slice() else {
            continue;
        };
        let Ok(human) = grade.trim().parse::<usize>() else {
            continue;
        };
        ensure!(human <= 2, "grade must be 0, 1 or 2: {line}");
        if let Some(model) = silver.get(*query).and_then(|d| d.get(*document)) {
            counts[usize::from(*model)][human] += 1;
        }
    }
    let total: usize = counts.iter().flatten().sum();
    ensure!(
        total > 0,
        "no human grades yet: fill the grade column of eval/review/human.tsv"
    );
    let agree: usize = (0..3).map(|i| counts[i][i]).sum();
    let observed = agree as f64 / total as f64;
    let expected: f64 = (0..3)
        .map(|i| {
            (counts[i].iter().sum::<usize>() as f64 / total as f64)
                * (counts.iter().map(|r| r[i]).sum::<usize>() as f64 / total as f64)
        })
        .sum();
    println!(
        "judged by both: {total}; agreement {observed:.3}; Cohen's kappa {:.3}",
        (observed - expected) / (1.0 - expected)
    );
    println!("rows silver 0/1/2, columns human 0/1/2:");
    for row in counts {
        println!("{row:?}");
    }
    Ok(())
}
