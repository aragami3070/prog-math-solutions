//! Offline search evaluation. Reads the database and the search index; never the sources.
//!
//!   eval                                   everything: rebuild, every variant, the table
//!   eval rebuild                          rebuild the index from the database
//!   eval run   --variant b0               run every query, write eval/runs/<variant>.tsv + .meta.json
//!   eval pool  --runs a.tsv,b.tsv         write eval/pool.jsonl for judging (shuffled, no scores)
//!   eval qrels                            merge eval/judgments.jsonl into eval/qrels.tsv
//!   eval score --variant b0               metrics -> eval/reports/<variant>.{json,md}
//!   eval compare --a b0 --b typo          paired bootstrap of nDCG@10, by query group
//!   eval review  --variant b0 --n 20      the top 10 of N test queries for a human to judge blind
//!   eval agreement                        silver vs human judgments: agreement and Cohen's kappa
//!   eval online-simulate --sessions 4000 --effect 0.06   synthetic events -> eval/online/synthetic.jsonl
//!   eval online-report --events f.jsonl --synthetic yes  | --db yes     A/B report -> eval/online/<name>.{json,md}
use anyhow::{Context, Result, bail, ensure};
use rust_knowledge::{
    models::search::RebuildResult,
    evaluation::{
        Query, QueryMetrics, Summary, bootstrap_diff, evaluate_query, group_runs, read_qrels,
        read_queries, read_run, summarize,
    },
    experiment::{EXPERIMENT, analysis, simulate},
    search::{
        ANALYZER_VERSION, Filters, Lemmas, Options, SYNONYMS_VERSION, SearchHandle,
        semantic::{Mode, POOL, Request, Semantic, answer},
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
            // Without a command the whole evaluation runs: rebuild, every variant, the table.
            "" | "all" => all().await,
            "rebuild" => rebuild_index().await,
            "run" => run(&options).await,
            "pool" => pool(&options),
            "qrels" => qrels(),
            "score" => score(&options),
            "compare" => compare(&options),
            "review" => review(&options),
            "agreement" => agreement(),
            "online-simulate" => online_simulate(&options),
            "online-report" => online_report(&options).await,
            _ => bail!(
                "usage: eval [all | rebuild | run | pool | qrels | score | compare | review | agreement | online-*] (see the file header)"
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

/// The same, but the result is returned instead of printed: the full run reports it itself.
async fn rebuild_index_quiet() -> Result<RebuildResult> {
    let pool = pool_connection().await?;
    let handle = handle();
    let mut tx = pool
        .begin_with("BEGIN ISOLATION LEVEL REPEATABLE READ, READ ONLY")
        .await?;
    Ok(rebuild::execute(&mut tx, &handle).await?)
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
    let mode = match options.get("mode").map(String::as_str).unwrap_or("lexical") {
        "lexical" => Mode::Lexical,
        "vector" => Mode::Vector,
        "hybrid" => Mode::Hybrid,
        other => anyhow::bail!("unknown --mode {other}"),
    };
    let rewriting = Options {
        fuzzy: options.get("fuzzy").is_some_and(|v| v == "on"),
        fuzzy_match: options.get("fuzzy_match").is_some_and(|v| v == "on"),
        synonyms: options.get("synonyms").is_some_and(|v| v == "on"),
        lemmas: options.get("lemmas").is_some_and(|v| v == "on"),
    };
    let meta = run_variant(variant, mode, rewriting, true).await?;
    println!("{}", serde_json::to_string_pretty(&meta)?);
    Ok(())
}

/// Runs every query of one variant against the current index and writes the run and its
/// passport. The variant decides the mode and the rewriting modes; nothing else differs
/// between the runs, so the comparison between them is fair.
async fn run_variant(
    variant: &str,
    mode: Mode,
    rewriting: Options,
    explain: bool,
) -> Result<Value> {
    let queries = read_queries(&PathBuf::from(EVAL).join("queries.jsonl"))?;
    let index = handle()
        .current()
        .context("no index: run `eval rebuild` first")?;
    let pool = pool_connection().await?;
    let (corpus_sha256, documents) = corpus_fingerprint(&pool).await?;
    let semantic = Semantic::from_env();
    let mut lines = String::new();
    let mut empty = 0;
    for query in &queries {
        let filters = Filters {
            source: query.filters.source.clone(),
            complete_only: query.filters.complete_only,
            ..Filters::default()
        };
        let found = answer(
            &semantic,
            index.clone(),
            Request {
                text: query.text.clone(),
                filters,
                options: rewriting,
                mode,
                pool: POOL,
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
                if explain && !found.rewrite.is_empty() {
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
        "fuzzy_match": rewriting.fuzzy_match,
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
    Ok(meta)
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
    print!("{}", score_variant(variant)?);
    Ok(())
}

/// The metrics of one variant, written to `eval/reports/<variant>.{json,md}`.
fn score_variant(variant: &str) -> Result<String> {
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
    // The judgments the numbers are computed from: without this a report cannot be compared with
    // another one, because a new label can change the metrics of every variant.
    let qrels_path = PathBuf::from(EVAL).join("qrels.tsv");
    let qrels_bytes = fs::read(&qrels_path)?;
    let qrels = json!({
        "judgments": qrels_bytes.iter().filter(|b| **b == b'\n').count(),
        "sha256": format!("{:x}", Sha256::digest(&qrels_bytes)),
    });
    let report = json!({ "run": meta, "qrels": qrels, "slices": slices, "per_query": scored.per_query });
    fs::write(
        dir.join(format!("{variant}.json")),
        serde_json::to_vec_pretty(&report)?,
    )?;
    let fmt = |v: Option<f64>| v.map_or("—".to_owned(), |v| format!("{v:.3}"));
    let mut md = format!(
        "# Отчёт: {variant}\n\n- корпус: {} документов, sha256 `{}`\n- индекс: поколение `{}`, анализатор `{}`\n- запросов: {}, глубина оценки: top-10\n- оценки: qrels, {} строк, sha256 `{}`\n\n\
         Оценки `llm-silver`, если не указано иное: см. `eval/judgments.jsonl` (assessor, evidence). nDCG, P, MRR, recall считаются по запросам с известными релевантными документами; запросы без них — отдельно.\n\n\
         | срез | запросов | с релевантными | nDCG@10 | P@10 | strict P@10 | MRR@10 | pooled recall@10 | покрытие оценками@10 | без ответа / с выдачей |\n|---|---|---|---|---|---|---|---|---|---|\n",
        meta["corpus_documents"],
        meta["corpus_sha256"].as_str().unwrap_or(""),
        meta["index_generation"].as_str().unwrap_or(""),
        meta["analyzer_version"].as_str().unwrap_or(""),
        queries.len(),
        qrels["judgments"],
        qrels["sha256"].as_str().unwrap_or(""),
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
    Ok(md)
}

/// One variant of the evaluation: the mode it searches in and the rewriting modes it turns
/// on. The variants differ by one thing at a time, so each of them can be compared with the
/// baseline and with the others.
struct Variant {
    name: &'static str,
    mode: Mode,
    rewriting: &'static [&'static str],
}

const VARIANTS: &[Variant] = &[
    Variant { name: "b0", mode: Mode::Lexical, rewriting: &[] },
    Variant { name: "b1-typo", mode: Mode::Lexical, rewriting: &["fuzzy"] },
    Variant { name: "b1-fuzzymatch", mode: Mode::Lexical, rewriting: &["fuzzy_match"] },
    Variant { name: "b2-syn", mode: Mode::Lexical, rewriting: &["synonyms"] },
    Variant { name: "b3-typo-syn", mode: Mode::Lexical, rewriting: &["fuzzy", "synonyms"] },
    Variant { name: "b4-lemma", mode: Mode::Lexical, rewriting: &["lemmas"] },
    Variant {
        name: "b5-all",
        mode: Mode::Lexical,
        rewriting: &["fuzzy", "fuzzy_match", "synonyms", "lemmas"],
    },
    Variant {
        name: "v5-vector",
        mode: Mode::Vector,
        rewriting: &["fuzzy", "fuzzy_match", "synonyms", "lemmas"],
    },
    Variant { name: "v5-hybrid", mode: Mode::Hybrid, rewriting: &[] },
    Variant {
        name: "v5-hybrid-all",
        mode: Mode::Hybrid,
        rewriting: &["fuzzy", "fuzzy_match", "synonyms", "lemmas"],
    },
];

/// Runs everything: rebuilds the index, runs every variant, scores it and prints the
/// comparison table. This is what `eval` without a command does.
async fn all() -> Result<()> {
    println!("Пересборка индекса из базы...");
    let rebuilt = rebuild_index_quiet().await?;
    println!(
        "  поколение {}, документов {}, {} мс\n",
        rebuilt.generation, rebuilt.documents, rebuilt.took_ms
    );
    let mut results: Vec<(String, Mode, Vec<Query>, Scored, Summary)> = Vec::new();
    for variant in VARIANTS {
        let rewriting = Options {
            fuzzy: variant.rewriting.contains(&"fuzzy"),
            fuzzy_match: variant.rewriting.contains(&"fuzzy_match"),
            synonyms: variant.rewriting.contains(&"synonyms"),
            lemmas: variant.rewriting.contains(&"lemmas"),
        };
        print!("  {} ... ", variant.name);
        match run_variant(variant.name, variant.mode, rewriting, false).await {
            Ok(_) => {
                score_variant(variant.name)?;
                let (queries, scored) = evaluate(variant.name)?;
                let summary = slice(&queries, &scored, |_| true);
                println!("готово");
                results.push((
                    variant.name.to_owned(),
                    variant.mode,
                    queries,
                    scored,
                    summary,
                ));
            }
            Err(error) => println!("пропущен: {error:#}"),
        }
    }
    println!();
    print_comparison(&results);
    Ok(())
}

/// The summary table: the variants, their metrics and the difference against the baseline.
fn print_comparison(results: &[(String, Mode, Vec<Query>, Scored, Summary)]) {
    let Some((baseline_name, _, queries, baseline, _)) = results.first() else {
        println!("Ни один вариант не посчитан.");
        return;
    };
    println!("Что посчитано:");
    if let Some(meta) = passport() {
        println!(
            "  корпус: {} документов, sha256 {}",
            meta["corpus_documents"].as_i64().unwrap_or_default(),
            &meta["corpus_sha256"].as_str().unwrap_or("?")[..16]
        );
        println!(
            "  индекс: поколение {}, анализатор {}",
            meta["index_generation"].as_str().unwrap_or("?"),
            meta["analyzer_version"].as_str().unwrap_or("?")
        );
    }
    if let Some((count, sha)) = judgments_fingerprint() {
        println!("  оценки: qrels, {count} строк, sha256 {}", &sha[..16]);
    }
    let baseline_summary = &results[0].4;
    println!(
        "  запросов: {}, из них с известными релевантными: {}; без ответа: {}, из них с выдачей: {}",
        queries.len(),
        baseline_summary.with_relevant,
        baseline_summary.no_answer_queries,
        baseline_summary.no_answer_with_results
    );

    let fmt = |v: Option<f64>| v.map_or("—".to_owned(), |v| format!("{v:.3}"));
    let mut rows: Vec<Vec<String>> = Vec::new();
    for (name, mode, _, scored, summary) in results {
        let delta = if name == baseline_name {
            "—".to_owned()
        } else {
            match delta_against(baseline, scored, queries) {
                Some((diff, low, high)) => {
                    format!("{diff:+.3} ({low:+.3} … {high:+.3})")
                }
                None => "—".to_owned(),
            }
        };
        rows.push(vec![
            name.clone(),
            match mode {
                Mode::Lexical => "лексика".to_owned(),
                Mode::Vector => "вектор".to_owned(),
                Mode::Hybrid => "гибрид".to_owned(),
            },
            VARIANTS
                .iter()
                .find(|v| v.name == name)
                .map(|v| {
                    if v.rewriting.is_empty() {
                        "—".to_owned()
                    } else {
                        v.rewriting.join("+")
                    }
                })
                .unwrap_or_default(),
            fmt(summary.ndcg10),
            fmt(slice(queries, scored, |q| q.split == "dev").ndcg10),
            fmt(slice(queries, scored, |q| q.split == "test").ndcg10),
            delta,
            fmt(summary.p10),
            fmt(summary.mrr10),
            fmt(summary.pooled_recall10),
        ]);
    }
    println!("\nСводка (nDCG@10 и P@10 — macro по запросам; Δ — парный bootstrap по группам запросов, 95% ДИ):");
    print_table(
        &[
            "вариант".to_owned(),
            "режим".to_owned(),
            "переписывание".to_owned(),
            "nDCG@10".to_owned(),
            "dev".to_owned(),
            "test".to_owned(),
            "Δ против b0".to_owned(),
            "P@10".to_owned(),
            "MRR".to_owned(),
            "recall".to_owned(),
        ],
        &rows,
    );

    let intents: Vec<String> = queries
        .iter()
        .map(|q| q.intent.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .filter(|intent| {
            slice(queries, &results[0].3, |q| &q.intent == intent).with_relevant > 0
        })
        .collect();
    let mut by_intent: Vec<Vec<String>> = Vec::new();
    for (name, _, _, scored, _) in results {
        let mut row = vec![name.clone()];
        for intent in &intents {
            let summary = slice(queries, scored, |q| &q.intent == intent);
            row.push(fmt(summary.ndcg10));
        }
        by_intent.push(row);
    }
    let mut headers = vec!["вариант".to_owned()];
    headers.extend(intents.iter().cloned());
    println!("\nПо срезам (nDCG@10):");
    print_table(&headers, &by_intent);
    println!("\nПодробные отчёты: eval/reports/<вариант>.md и .json");
}

/// The difference between two rankings of the same queries, by paired bootstrap over the
/// query groups (paraphrases of one need are not independent).
fn delta_against(first: &Scored, second: &Scored, queries: &[Query]) -> Option<(f64, f64, f64)> {
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
    bootstrap_diff(&pairs, 1, 10_000)
}

/// The passport of a run: what the numbers were computed from.
fn passport() -> Option<Value> {
    let dir = PathBuf::from(EVAL).join("runs");
    let mut names: Vec<String> = fs::read_dir(&dir)
        .ok()?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .filter(|name| name.ends_with(".meta.json"))
        .collect();
    names.sort();
    let name = names.first()?;
    fs::read(dir.join(name)).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok())
}

/// The judgments the numbers are computed from.
fn judgments_fingerprint() -> Option<(usize, String)> {
    let bytes = fs::read(PathBuf::from(EVAL).join("qrels.tsv")).ok()?;
    let count = bytes.iter().filter(|b| **b == b'\n').count();
    Some((count, format!("{:x}", Sha256::digest(&bytes))))
}

/// A table with the columns aligned to their content.
fn print_table(headers: &[String], rows: &[Vec<String>]) {
    let mut widths: Vec<usize> = headers.iter().map(|h| h.chars().count()).collect();
    for row in rows {
        for (index, cell) in row.iter().enumerate() {
            if index < widths.len() {
                widths[index] = widths[index].max(cell.chars().count());
            }
        }
    }
    let line = |cells: &[String]| {
        cells
            .iter()
            .enumerate()
            .map(|(index, cell)| {
                let width = widths.get(index).copied().unwrap_or(0);
                format!("{cell:<width$}")
            })
            .collect::<Vec<_>>()
            .join("  ")
    };
    println!("{}", line(headers));
    println!(
        "{}",
        widths
            .iter()
            .map(|width| "-".repeat(*width))
            .collect::<Vec<_>>()
            .join("  ")
    );
    for row in rows {
        println!("{}", line(row));
    }
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

fn online_simulate(options: &HashMap<String, String>) -> Result<()> {
    let number = |name: &str, default: f64| -> Result<f64> {
        options.get(name).map_or(Ok(default), |v| {
            v.parse()
                .with_context(|| format!("--{name} must be a number"))
        })
    };
    let events = simulate::events(
        number("sessions", 4000.0)? as usize,
        number("seed", 1.0)? as u64,
        number("baseline", 0.55)?,
        number("effect", 0.0)?,
    );
    let out = PathBuf::from(
        options
            .get("out")
            .map_or("eval/online/synthetic.jsonl", String::as_str),
    );
    fs::create_dir_all(out.parent().unwrap_or(Path::new(".")))?;
    let lines: Result<Vec<String>, _> = events.iter().map(serde_json::to_string).collect();
    fs::write(&out, lines?.join("\n") + "\n")?;
    println!("{} events -> {}", events.len(), out.display());
    Ok(())
}

async fn online_report(options: &HashMap<String, String>) -> Result<()> {
    let (events, source) = if let Some(path) = options.get("events") {
        let text = fs::read_to_string(path).with_context(|| format!("reading {path}"))?;
        let events = text
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(serde_json::from_str)
            .collect::<Result<Vec<_>, _>>()?;
        let source = if options.get("synthetic").is_some_and(|v| v == "yes") {
            format!(
                "SYNTHETIC traffic from {path}: checks the pipeline, says nothing about the search"
            )
        } else {
            format!("recorded events from {path}")
        };
        (events, source)
    } else {
        ensure!(
            options.get("db").is_some_and(|v| v == "yes"),
            "give --events <file> or --db yes"
        );
        let pool = pool_connection().await?;
        let events = rust_knowledge::repository::events::read(&pool, EXPERIMENT.id, None).await?;
        (
            events,
            "events recorded by this server (search_events)".to_owned(),
        )
    };
    let report = analysis::analyze(&EXPERIMENT, &events, &source);
    let name = options.get("name").map_or("report", String::as_str);
    let dir = PathBuf::from(EVAL).join("online");
    fs::create_dir_all(&dir)?;
    fs::write(
        dir.join(format!("{name}.json")),
        serde_json::to_vec_pretty(&report)?,
    )?;
    let markdown = analysis::markdown(&report, options.get("offline").map(String::as_str));
    fs::write(dir.join(format!("{name}.md")), &markdown)?;
    println!("{markdown}");
    Ok(())
}
