//! Query rewriting: typo repair against the corpus vocabulary and a curated synonym table.
//!
//! Both are optional modes. The original query always stays in the search; a rewrite only
//! adds lower-weight branches, and the response says what was added so nothing is silent.

use super::{Error, SearchIndex};
use serde::Serialize;
use std::collections::HashMap;
use tantivy::schema::Field;
use utoipa::ToSchema;

/// Bump when `SYNONYMS` changes: runs of the evaluation record it.
pub const SYNONYMS_VERSION: &str = "syn-v1";
/// Weight of a corrected or expanded branch relative to the user's own words.
pub const REWRITE_BOOST: f32 = 0.5;
const MAX_VARIANTS: usize = 4;
const MIN_LEN: usize = 5;

#[derive(Debug, Default, Clone, Copy)]
pub struct Options {
    pub fuzzy: bool,
    pub synonyms: bool,
    pub lemmas: bool,
}

#[derive(Debug, Serialize, ToSchema, Clone, PartialEq, Eq)]
pub struct Correction {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Serialize, ToSchema, Clone, PartialEq, Eq)]
pub struct Expansion {
    pub from: String,
    pub to: Vec<String>,
}

/// Another form of a query word: `objects` for `object`, found through their shared lemma.
#[derive(Debug, Serialize, ToSchema, Clone, PartialEq, Eq)]
pub struct WordForms {
    pub from: String,
    pub to: Vec<String>,
}

/// What was added to the query, in terms the user can read.
#[derive(Debug, Default, Serialize, ToSchema, Clone)]
pub struct Rewrite {
    pub corrections: Vec<Correction>,
    pub expansions: Vec<Expansion>,
    pub word_forms: Vec<WordForms>,
    /// Modes asked for that could not run (the answer is then the plain lexical one).
    pub unavailable: Vec<String>,
    /// The query with corrections applied, when there were any.
    pub corrected_query: Option<String>,
    /// Token sequences to search as extra phrases; internal to query building.
    #[serde(skip)]
    pub phrases: Vec<Vec<String>>,
    /// Corrected single tokens to search as extra terms; internal to query building.
    #[serde(skip)]
    pub terms: Vec<String>,
}

impl Rewrite {
    pub fn is_empty(&self) -> bool {
        self.corrections.is_empty() && self.expansions.is_empty() && self.word_forms.is_empty()
    }
}

/// Curated, two-way groups: any member finds the others. Lower-case, tokenizer form.
/// Multi-word members are matched as phrases.
const SYNONYMS: &[&[&str]] = &[
    &["gat", "gats", "generic associated types"],
    &["nll", "non lexical lifetimes", "non-lexical lifetimes"],
    &["tait", "type alias impl trait"],
    &["rpitit", "return position impl trait in trait"],
    &["afit", "async fn in trait", "async functions in traits"],
    &[
        "hrtb",
        "higher ranked trait bounds",
        "higher-ranked trait bounds",
    ],
    &["dst", "dynamically sized types", "unsized types"],
    &["zst", "zero sized types", "zero-sized types"],
    &["mir", "mid-level intermediate representation"],
    &["ub", "undefined behavior", "undefined behaviour"],
    &["ffi", "foreign function interface"],
    &["ice", "internal compiler error"],
    &["tls", "thread local storage"],
    &["const generics", "const parameters"],
    &["dyn trait", "trait objects"],
    &["borrow checker", "borrowck"],
    &["lifetime elision", "elided lifetimes"],
    &["specialization", "specialisation"],
    &["pattern matching", "match expressions"],
    &["proc macro", "procedural macros", "proc-macro"],
];

/// Surface form -> lemma, built offline by `tools/lemmas/build.py` (spaCy, in context).
pub struct Lemmas {
    pub version: String,
    pub sha256: String,
    forms: HashMap<String, String>,
    by_lemma: HashMap<String, Vec<String>>,
}

#[derive(serde::Deserialize)]
struct LemmaFile {
    version: String,
    #[serde(default)]
    sha256: String,
    forms: HashMap<String, LemmaEntry>,
}

#[derive(serde::Deserialize)]
struct LemmaEntry {
    lemma: String,
}

impl Lemmas {
    pub fn parse(json: &[u8]) -> Result<Self, serde_json::Error> {
        let file: LemmaFile = serde_json::from_slice(json)?;
        let forms: HashMap<String, String> = file
            .forms
            .into_iter()
            .map(|(form, entry)| (form, entry.lemma))
            .collect();
        let mut by_lemma: HashMap<String, Vec<String>> = HashMap::new();
        for (form, lemma) in &forms {
            by_lemma
                .entry(lemma.clone())
                .or_default()
                .push(form.clone());
        }
        Ok(Self {
            version: file.version,
            sha256: file.sha256,
            forms,
            by_lemma,
        })
    }

    /// The table named by `LEMMA_FILE` (default `data/lemmas.json`), loaded once.
    /// `None` when the file is missing or unreadable: the lemma mode is then unavailable.
    pub fn shared() -> Option<&'static Lemmas> {
        static SHARED: std::sync::OnceLock<Option<Lemmas>> = std::sync::OnceLock::new();
        SHARED
            .get_or_init(|| {
                let path =
                    std::env::var("LEMMA_FILE").unwrap_or_else(|_| "data/lemmas.json".into());
                let bytes = std::fs::read(&path).ok()?;
                match Self::parse(&bytes) {
                    Ok(lemmas) => Some(lemmas),
                    Err(error) => {
                        tracing::warn!(%error, path, "lemma table is unusable");
                        None
                    }
                }
            })
            .as_ref()
    }

    /// Other forms of the same lemma, most common in the corpus first.
    fn related(&self, token: &str, vocabulary: &Vocabulary) -> Vec<String> {
        let lemma = self.forms.get(token).map(String::as_str).unwrap_or(token);
        let mut forms: Vec<String> = self
            .by_lemma
            .get(lemma)
            .into_iter()
            .flatten()
            .cloned()
            .chain(std::iter::once(lemma.to_owned()))
            .filter(|form| form != token)
            .collect();
        forms.sort();
        forms.dedup();
        forms.retain(|form| vocabulary.df(form) > 0);
        forms.sort_by_key(|form| std::cmp::Reverse(vocabulary.df(form)));
        forms.truncate(MAX_VARIANTS);
        forms
    }
}

/// Terms of the body field with the number of documents containing each.
pub struct Vocabulary {
    terms: Vec<(String, u32)>,
    df: HashMap<String, u32>,
}

impl Vocabulary {
    pub(super) fn read(index: &SearchIndex, field: Field) -> Result<Self, Error> {
        let searcher = index.reader.searcher();
        let mut df: HashMap<String, u32> = HashMap::new();
        for segment in searcher.segment_readers() {
            let inverted = segment.inverted_index(field)?;
            let mut stream = inverted.terms().stream()?;
            while stream.advance() {
                if let Ok(term) = std::str::from_utf8(stream.key()) {
                    *df.entry(term.to_owned()).or_default() += stream.value().doc_freq;
                }
            }
        }
        // Candidates: words, seen in at least two documents (one-off tokens are mostly noise).
        let terms = df
            .iter()
            .filter(|(term, count)| {
                **count >= 2
                    && term.chars().count() >= MIN_LEN - 1
                    && term.chars().all(char::is_alphabetic)
            })
            .map(|(term, count)| (term.clone(), *count))
            .collect();
        Ok(Self { terms, df })
    }

    fn df(&self, term: &str) -> u32 {
        self.df.get(term).copied().unwrap_or(0)
    }

    /// The most frequent vocabulary word within the edit budget of `term`, when `term` itself
    /// is rare enough to be a likely slip.
    fn correct(&self, term: &str) -> Option<String> {
        let len = term.chars().count();
        if len < MIN_LEN || !term.chars().all(char::is_alphabetic) {
            return None;
        }
        let own = self.df(term);
        let budget = if len <= 7 { 1 } else { 2 };
        let mut best: Option<(&str, u32)> = None;
        for (candidate, count) in &self.terms {
            // Slips rarely change the first letter; requiring it also keeps real words
            // the corpus lacks (`cake`, `hooks`) from being "repaired" into other words.
            if candidate == term
                || candidate.chars().next() != term.chars().next()
                || candidate.chars().count().abs_diff(len) > budget
            {
                continue;
            }
            if edit_distance(term, candidate, budget).is_some()
                && best.is_none_or(|(_, top)| *count > top)
            {
                best = Some((candidate, *count));
            }
        }
        let (word, count) = best?;
        // A word the corpus knows well is not a typo, unless the neighbour is far more common.
        (own == 0 || count >= own.saturating_mul(20)).then(|| word.to_owned())
    }
}

/// Damerau-Levenshtein (adjacent transpositions count once), bounded: `None` past `max`.
pub fn edit_distance(a: &str, b: &str, max: usize) -> Option<usize> {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.len().abs_diff(b.len()) > max {
        return None;
    }
    let mut rows = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for (i, row) in rows.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, cell) in rows[0].iter_mut().enumerate() {
        *cell = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            let mut value = (rows[i - 1][j] + 1)
                .min(rows[i][j - 1] + 1)
                .min(rows[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                value = value.min(rows[i - 2][j - 2] + 1);
            }
            rows[i][j] = value;
        }
    }
    let distance = rows[a.len()][b.len()];
    (distance <= max).then_some(distance)
}

impl SearchIndex {
    pub(super) fn rewrite(
        &self,
        tokens: &[String],
        options: Options,
        lemmas: Option<&Lemmas>,
    ) -> Result<Rewrite, Error> {
        let mut rewrite = Rewrite::default();
        let mut effective = tokens.to_vec();
        if options.fuzzy {
            let vocabulary = self.vocabulary()?;
            for (i, token) in tokens.iter().enumerate() {
                if let Some(to) = vocabulary.correct(token) {
                    effective[i] = to.clone();
                    rewrite.corrections.push(Correction {
                        from: token.clone(),
                        to,
                    });
                }
            }
            if !rewrite.corrections.is_empty() {
                if effective.len() == 1 {
                    rewrite.terms.push(effective[0].clone());
                } else {
                    for correction in &rewrite.corrections {
                        rewrite.terms.push(correction.to.clone());
                    }
                    rewrite.phrases.push(effective.clone());
                }
                rewrite.corrected_query = Some(effective.join(" "));
            }
        }
        if options.lemmas {
            match lemmas {
                None => rewrite.unavailable.push("lemmas".to_owned()),
                Some(lemmas) => {
                    let vocabulary = self.vocabulary()?;
                    for token in tokens {
                        let related = lemmas.related(token, vocabulary);
                        if !related.is_empty() {
                            rewrite.terms.extend(related.iter().cloned());
                            rewrite.word_forms.push(WordForms {
                                from: token.clone(),
                                to: related,
                            });
                        }
                    }
                }
            }
        }
        if options.synonyms {
            // Expand what the user wrote, not the repaired words: a repair is a guess.
            for group in SYNONYMS {
                for member in *group {
                    let member_tokens = self.tokens(member, self.fields.body)?;
                    if contains_run(tokens, &member_tokens) {
                        let others: Vec<&str> =
                            group.iter().copied().filter(|m| m != member).collect();
                        let mut added = Vec::new();
                        for other in others.into_iter().take(MAX_VARIANTS) {
                            let phrase = self.tokens(other, self.fields.body)?;
                            if !phrase.is_empty() {
                                rewrite.phrases.push(phrase);
                                added.push(other.to_owned());
                            }
                        }
                        rewrite.expansions.push(Expansion {
                            from: (*member).to_owned(),
                            to: added,
                        });
                        break;
                    }
                }
            }
        }
        Ok(rewrite)
    }
}

fn contains_run(tokens: &[String], run: &[String]) -> bool {
    !run.is_empty() && tokens.windows(run.len()).any(|window| window == run)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edit_distance_counts_a_swap_once() {
        assert_eq!(edit_distance("awiat", "await", 1), Some(1));
        assert_eq!(edit_distance("cheker", "checker", 1), Some(1));
        assert_eq!(edit_distance("lifetims", "lifetimes", 1), Some(1));
        assert_eq!(edit_distance("abc", "xyz", 1), None);
        assert_eq!(edit_distance("same", "same", 1), Some(0));
    }

    #[test]
    fn synonym_groups_are_lower_case_and_unique() {
        let mut seen = std::collections::HashSet::new();
        for group in SYNONYMS {
            assert!(group.len() >= 2);
            for member in *group {
                assert_eq!(*member, member.to_lowercase());
                assert!(seen.insert(*member), "{member} appears in two groups");
            }
        }
    }
}
