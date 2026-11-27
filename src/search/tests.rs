use super::*;
use tantivy::{
    DocSet, Term,
    postings::Postings,
    schema::{IndexRecordOption, Value},
};

fn doc(id: i64, source: &str, native_id: &str, title: &str, body: &str) -> IndexDocument {
    IndexDocument {
        topics: Vec::new(),
        id,
        source: source.into(),
        native_id: native_id.into(),
        url: format!("https://example.org/{source}/{native_id}"),
        title: title.into(),
        body: body.into(),
        completeness: if id % 2 == 0 { "partial" } else { "complete" }.into(),
        // Internals topics carry the year of their first post; RFCs have no date.
        year: (source == "internals").then(|| 2012 + id as i32 * 2),
    }
}

fn corpus() -> Vec<IndexDocument> {
    vec![
        doc(
            1,
            "rfc",
            "3058",
            "Try trait v2",
            "The Try trait and FromResidual make the ? operator generic.",
        ),
        doc(
            2,
            "rfc",
            "2394",
            "Async await",
            "Writing async fn bodies. The async fn syntax returns a future.",
        ),
        doc(
            3,
            "internals",
            "3058",
            "Unrelated topic with a lucky number",
            "Nothing about traits here.",
        ),
        doc(
            4,
            "internals",
            "100",
            "Negative impls",
            "Can we say T: !Send for this type? Auto traits and Send.",
        ),
        doc(
            5,
            "internals",
            "200",
            "GAT stabilization",
            "Generic associated types, GAT for short. async code needs them. fn items too.",
        ),
        doc(
            6,
            "internals",
            "300",
            "Fn then async",
            "We discussed fn first and the async keyword later.",
        ),
    ]
}

fn build(handle: &SearchHandle, docs: Vec<IndexDocument>) -> std::sync::Arc<SearchIndex> {
    let mut builder = handle.begin().unwrap();
    for doc in docs {
        builder.add(doc).unwrap();
    }
    handle.install(builder.finish().unwrap()).unwrap()
}

fn ids(index: &SearchIndex, query: &str) -> Vec<String> {
    index
        .search(query, &Filters::default(), Options::default(), 0, 10)
        .unwrap()
        .hits
        .into_iter()
        .map(|hit| hit.document_id)
        .collect()
}

fn fixture() -> (tempfile::TempDir, SearchHandle, std::sync::Arc<SearchIndex>) {
    let dir = tempfile::tempdir().unwrap();
    let handle = SearchHandle::open(dir.path());
    let index = build(&handle, corpus());
    (dir, handle, index)
}

#[test]
fn rfc_number_and_identifier_queries_find_the_document_first() {
    let (_dir, _handle, index) = fixture();
    assert_eq!(ids(&index, "rfc:3058")[0], "rfc:3058");
    assert_eq!(ids(&index, "RFC 3058")[0], "rfc:3058");
    // A bare number names both an RFC and a topic; the RFC comes first.
    assert_eq!(&ids(&index, "3058")[..2], ["rfc:3058", "internals:3058"]);
    assert_eq!(ids(&index, "FromResidual"), ["rfc:3058"]);
    assert_eq!(ids(&index, "GAT"), ["internals:200"]);
}

#[test]
fn punctuation_sensitive_queries_do_not_disappear() {
    let (_dir, _handle, index) = fixture();
    assert_eq!(ids(&index, "?")[..2].len(), 2);
    assert!(ids(&index, "?").contains(&"rfc:3058".to_string()));
    assert_eq!(ids(&index, "\"!send\"")[0], "internals:100");
    assert_eq!(ids(&index, "!Send")[0], "internals:100");
}

#[test]
fn phrases_use_positions_not_just_word_presence() {
    let (_dir, _handle, index) = fixture();
    // Document 5 has both words far apart and 6 has them in the other order; only 2 has the phrase.
    assert_eq!(ids(&index, "\"async fn\""), ["rfc:2394"]);
    // Without quotes the phrase still wins the ranking over scattered words.
    assert_eq!(ids(&index, "async fn")[0], "rfc:2394");
}

#[test]
fn a_period_keeps_dated_documents_inside_it_and_drops_undated_ones() {
    let (_dir, _handle, index) = fixture();
    let ids = |from, to| {
        let filters = Filters {
            year_from: from,
            year_to: to,
            ..Filters::default()
        };
        let mut found: Vec<String> = index
            .search("async", &filters, Options::default(), 0, 10)
            .unwrap()
            .hits
            .into_iter()
            .map(|hit| hit.document_id)
            .collect();
        found.sort();
        found
    };
    // "async" matches rfc:2394 (undated), internals:200 (2022), internals:300 (2024).
    assert_eq!(ids(None, None).len(), 3);
    assert_eq!(ids(Some(2023), None), ["internals:300"]);
    assert_eq!(ids(None, Some(2022)), ["internals:200"]);
    assert_eq!(
        ids(Some(2022), Some(2024)),
        ["internals:200", "internals:300"]
    );
    assert!(ids(Some(2030), None).is_empty());
}

#[test]
fn filters_narrow_matches_and_total_counts_all_of_them() {
    let (_dir, _handle, index) = fixture();
    let all = index
        .search("async", &Filters::default(), Options::default(), 0, 1)
        .unwrap();
    assert_eq!(all.total, 3);
    assert_eq!(all.hits.len(), 1);
    let internals = Filters {
        source: Some("internals".into()),
        ..Filters::default()
    };
    let hits = index
        .search("async", &internals, Options::default(), 0, 10)
        .unwrap();
    assert!(hits.hits.iter().all(|hit| hit.source == "internals"));
    assert_eq!(hits.total, 2);
    let complete = Filters {
        complete_only: true,
        ..Filters::default()
    };
    let hits = index
        .search("async", &complete, Options::default(), 0, 10)
        .unwrap();
    assert!(hits.hits.iter().all(|hit| hit.completeness == "complete"));
    // Pages do not repeat documents.
    let first = index
        .search("async", &Filters::default(), Options::default(), 0, 2)
        .unwrap();
    let second = index
        .search("async", &Filters::default(), Options::default(), 2, 2)
        .unwrap();
    assert!(
        first
            .hits
            .iter()
            .all(|a| second.hits.iter().all(|b| a.document_id != b.document_id))
    );
}

#[test]
fn snippets_are_plain_text_with_matched_parts() {
    let (_dir, _handle, index) = fixture();
    let hit = &index
        .search(
            "FromResidual",
            &Filters::default(),
            Options::default(),
            0,
            1,
        )
        .unwrap()
        .hits[0];
    let matched: Vec<&str> = hit
        .snippet
        .iter()
        .filter(|p| p.matched)
        .map(|p| p.text.as_str())
        .collect();
    assert_eq!(matched, ["FromResidual"]);
    let text: String = hit.snippet.iter().map(|p| p.text.as_str()).collect();
    assert!(text.contains("The Try trait"));
    // A title-only match still shows the start of the text.
    let hit = &index
        .search("rfc:3058", &Filters::default(), Options::default(), 0, 1)
        .unwrap()
        .hits[0];
    assert!(!hit.snippet.is_empty());
}

#[test]
fn bad_queries_are_rejected() {
    let (_dir, _handle, index) = fixture();
    for query in ["", "   ", "\"\"", &"a".repeat(201)] {
        assert!(
            matches!(
                index.search(query, &Filters::default(), Options::default(), 0, 10),
                Err(Error::InvalidQuery(_))
            ),
            "{query:?}"
        );
    }
    assert!(matches!(
        index.search("async", &Filters::default(), Options::default(), 995, 10),
        Err(Error::InvalidQuery(_))
    ));
}

/// The inverted index itself: a term leads to the documents containing it, with how often
/// and where in the text.
#[test]
fn postings_hold_documents_frequencies_and_positions() {
    let (_dir, _handle, index) = fixture();
    let searcher = index.reader.searcher();
    let body = index.fields.body;
    let term = Term::from_field_text(body, "async");
    let mut found = Vec::new();
    for segment in searcher.segment_readers() {
        let inverted = segment.inverted_index(body).unwrap();
        let Some(mut postings) = inverted
            .read_postings(&term, IndexRecordOption::WithFreqsAndPositions)
            .unwrap()
        else {
            continue;
        };
        let store = segment.get_store_reader(1).unwrap();
        let mut positions = Vec::new();
        while postings.doc() != tantivy::TERMINATED {
            let doc: tantivy::TantivyDocument = store.get(postings.doc()).unwrap();
            let id = doc
                .get_first(index.fields.doc_id)
                .unwrap()
                .as_str()
                .unwrap()
                .to_owned();
            postings.positions(&mut positions);
            found.push((id, postings.term_freq(), positions.clone()));
            postings.advance();
        }
    }
    found.sort();
    // "Writing async fn bodies. The async fn syntax ..." has `async` as tokens 1 and 5.
    assert_eq!(found[0], ("internals:200".to_string(), 1, vec![6]));
    assert_eq!(found[1], ("internals:300".to_string(), 1, vec![6]));
    assert_eq!(found[2], ("rfc:2394".to_string(), 2, vec![1, 5]));
    assert_eq!(found.len(), 3);
}

#[test]
fn reopening_gives_the_same_results_and_rebuild_replaces_everything() {
    let (dir, handle, index) = fixture();
    let before = ids(&index, "async fn");
    let reopened = SearchHandle::open(dir.path());
    let reopened = reopened
        .current()
        .expect("the current generation is found again");
    assert_eq!(reopened.generation(), index.generation());
    assert_eq!(ids(&reopened, "async fn"), before);

    // A rebuild with fewer documents leaves no stale ones behind.
    std::thread::sleep(std::time::Duration::from_millis(5));
    let next = build(&handle, corpus().into_iter().take(1).collect());
    assert_ne!(next.generation(), index.generation());
    assert_eq!(next.documents(), 1);
    assert!(ids(&next, "async").is_empty());
    assert_eq!(ids(&next, "rfc:3058"), ["rfc:3058"]);
    // The running readers keep working on their generation until they drop it.
    assert_eq!(ids(&index, "async fn"), before);
    assert_eq!(
        SearchHandle::open(dir.path())
            .current()
            .unwrap()
            .documents(),
        1
    );
}

#[test]
fn old_generations_are_pruned_and_failed_builds_leave_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let handle = SearchHandle::open(dir.path());
    for _ in 0..4 {
        build(&handle, corpus());
        std::thread::sleep(std::time::Duration::from_millis(3));
    }
    drop(handle.begin().unwrap()); // an abandoned build
    let generations = std::fs::read_dir(dir.path())
        .unwrap()
        .filter(|e| {
            e.as_ref()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("gen-")
        })
        .count();
    assert_eq!(generations, 2);
}

#[test]
fn an_index_from_another_analyzer_is_not_opened() {
    let (dir, handle, index) = fixture();
    let manifest = dir.path().join(index.generation()).join("manifest.json");
    let text = std::fs::read_to_string(&manifest)
        .unwrap()
        .replace(ANALYZER_VERSION, "rust-v0");
    std::fs::write(&manifest, text).unwrap();
    drop(handle);
    assert!(SearchHandle::open(dir.path()).current().is_none());
}

fn search_with(index: &SearchIndex, query: &str, options: Options) -> Results {
    index
        .search(query, &Filters::default(), options, 0, 10)
        .unwrap()
}

#[test]
fn a_typo_finds_nothing_unless_repair_is_on_and_the_repair_is_reported() {
    let (_dir, _handle, index) = fixture();
    assert!(
        search_with(&index, "asynx", Options::default())
            .hits
            .is_empty()
    );
    let found = search_with(
        &index,
        "asynx",
        Options {
            fuzzy: true,
            synonyms: false,
            lemmas: false,
            ..Options::default()
        },
    );
    assert!(!found.hits.is_empty());
    assert_eq!(
        found.rewrite.corrections,
        [Correction {
            from: "asynx".into(),
            to: "async".into()
        }]
    );
    assert_eq!(found.rewrite.corrected_query.as_deref(), Some("async"));
}

fn fuzzy_match() -> Options {
    Options {
        fuzzy_match: true,
        ..Options::default()
    }
}

#[test]
fn fuzzy_match_finds_near_words_in_the_index_without_rewriting_the_query() {
    let (_dir, _handle, index) = fixture();
    for typo in ["asynx", "asyncc", "aysnc"] {
        assert!(
            search_with(&index, typo, Options::default())
                .hits
                .is_empty()
        );
        let found = search_with(&index, typo, fuzzy_match());
        assert!(!found.hits.is_empty(), "{typo} should reach async");
        assert_eq!(found.hits[0].document_id, "rfc:2394", "{typo}");
        assert!(found.rewrite.is_empty(), "the query text stays as typed");
    }
}

#[test]
fn an_exact_word_outranks_a_near_one() {
    let (_dir, _handle, index) = fixture();
    let exact = search_with(&index, "traits", fuzzy_match());
    let plain = search_with(&index, "traits", Options::default());
    // Everything the exact search finds is still found, in the same order at the top.
    assert!(exact.total >= plain.total);
    assert_eq!(exact.hits[0].document_id, plain.hits[0].document_id);
}

#[test]
fn short_words_digits_and_phrases_stay_exact_under_fuzzy_match() {
    let (_dir, _handle, index) = fixture();
    // `fn` is under 4 letters: no neighbours.
    assert_eq!(
        search_with(&index, "fx", fuzzy_match()).total,
        search_with(&index, "fx", Options::default()).total
    );
    // `3059` is one digit from `3058` but is another number.
    assert!(search_with(&index, "3059", fuzzy_match()).hits.is_empty());
    // A quoted phrase is literal.
    assert!(
        search_with(&index, "\"asynx\"", fuzzy_match())
            .hits
            .is_empty()
    );
}

#[test]
fn a_known_word_is_not_treated_as_a_typo() {
    let (_dir, _handle, index) = fixture();
    let found = search_with(
        &index,
        "async",
        Options {
            fuzzy: true,
            synonyms: true,
            lemmas: false,
            ..Options::default()
        },
    );
    assert!(found.rewrite.corrections.is_empty());
}

#[test]
fn quoted_phrases_are_never_rewritten() {
    let (_dir, _handle, index) = fixture();
    let found = search_with(
        &index,
        "\"asynx\"",
        Options {
            fuzzy: true,
            synonyms: true,
            lemmas: false,
            ..Options::default()
        },
    );
    assert!(found.rewrite.is_empty());
    assert!(found.hits.is_empty());
}

#[test]
fn synonyms_expand_the_query_and_say_so() {
    let (_dir, _handle, index) = fixture();
    assert!(
        search_with(&index, "gats", Options::default())
            .hits
            .is_empty()
    );
    let found = search_with(
        &index,
        "gats",
        Options {
            fuzzy: false,
            synonyms: true,
            lemmas: false,
            ..Options::default()
        },
    );
    assert_eq!(found.hits[0].document_id, "internals:200");
    assert_eq!(found.rewrite.expansions[0].from, "gats");
    assert!(
        found.rewrite.expansions[0]
            .to
            .contains(&"generic associated types".to_owned())
    );
}

#[test]
fn lemmas_add_other_forms_of_a_word_and_report_them() {
    let (_dir, _handle, index) = fixture();
    let table = Lemmas::parse(
        br#"{"version":"t","forms":{"futures":{"lemma":"future","pos":"NOUN"},"bodies":{"lemma":"body","pos":"NOUN"}}}"#,
    )
    .unwrap();
    let tokens = vec!["futures".to_owned()];
    let options = Options {
        lemmas: true,
        ..Options::default()
    };
    let rewrite = index.rewrite(&tokens, options, Some(&table)).unwrap();
    assert_eq!(rewrite.word_forms[0].from, "futures");
    assert_eq!(rewrite.word_forms[0].to, ["future"]);
    // Asked for but no table: the mode says it did not run.
    let rewrite = index.rewrite(&tokens, options, None).unwrap();
    assert_eq!(rewrite.unavailable, ["lemmas"]);
    assert!(rewrite.word_forms.is_empty());
}

#[tokio::test]
async fn vector_modes_degrade_to_lexical_and_say_why() {
    use super::semantic::{Mode, Request, Semantic, answer};
    let (_dir, _handle, index) = fixture();
    // Nothing listens here: both services are "down".
    let semantic = Semantic::new("http://127.0.0.1:9", "http://127.0.0.1:9");
    for mode in [Mode::Vector, Mode::Hybrid] {
        let found = answer(
            &semantic,
            index.clone(),
            Request {
                text: "async".into(),
                filters: Filters::default(),
                options: Options::default(),
                mode,
                pool: 100,
                offset: 0,
                limit: 10,
            },
        )
        .await
        .unwrap();
        assert_eq!(found.mode, Mode::Lexical);
        assert!(found.degraded.is_some());
        assert!(!found.hits.is_empty());
        assert!(!found.total_is_pool);
    }
}

#[test]
fn assigned_topic_filters_and_supports_empty_queries() {
    let dir = tempfile::tempdir().unwrap();
    let handle = SearchHandle::open(dir.path());
    let mut builder = handle.begin().unwrap();
    let mut a = doc(1, "rfc", "1", "Async", "async text");
    a.topics = vec!["async".into()];
    builder.add(a).unwrap();
    builder
        .add(doc(2, "rfc", "2", "Async", "async text"))
        .unwrap();
    let index = builder.finish().unwrap();
    let filters = Filters {
        topic: Some("async".into()),
        ..Default::default()
    };
    for q in ["", "async"] {
        let result = index
            .search(q, &filters, Options::default(), 0, 10)
            .unwrap();
        assert_eq!(result.total, 1);
        assert_eq!(result.hits[0].id, 1);
    }
    assert_eq!(index.document("rfc:1").unwrap().unwrap().topics, ["async"]);
    assert!(
        index
            .search("", &Filters::default(), Options::default(), 0, 10)
            .is_err()
    );
}
