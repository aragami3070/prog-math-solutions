use super::{ANALYZER_NAME, ANALYZER_VERSION, analyzer, rewrite::Vocabulary};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, OnceLock, RwLock},
    time::{SystemTime, UNIX_EPOCH},
};
use tantivy::{
    Index, IndexReader, IndexWriter, ReloadPolicy, TantivyDocument,
    schema::{
        Field, IndexRecordOption, STORED, STRING, Schema, TextFieldIndexing, TextOptions, Value,
    },
};

const CURRENT: &str = "CURRENT";
const MANIFEST: &str = "manifest.json";
const WRITER_HEAP: usize = 100_000_000;
/// Old generations kept next to the current one, in case a rollback is needed.
const KEPT_GENERATIONS: usize = 2;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    InvalidQuery(&'static str),
    #[error("search index error")]
    Tantivy(#[from] tantivy::TantivyError),
    #[error("search index files error")]
    Io(#[from] std::io::Error),
    #[error("search index manifest error")]
    Json(#[from] serde_json::Error),
    #[error("index was built with analyzer {found}, this build uses {ANALYZER_VERSION}")]
    AnalyzerMismatch { found: String },
}

#[derive(Clone, Copy)]
pub struct Fields {
    pub doc_id: Field,
    pub db_id: Field,
    pub source: Field,
    pub url: Field,
    pub title: Field,
    pub body: Field,
    pub completeness: Field,
}

fn schema() -> (Schema, Fields) {
    let text = TextOptions::default()
        .set_indexing_options(
            TextFieldIndexing::default()
                .set_tokenizer(ANALYZER_NAME)
                .set_index_option(IndexRecordOption::WithFreqsAndPositions),
        )
        .set_stored();
    let mut builder = Schema::builder();
    let fields = Fields {
        // `source:native_id`: exact, stable across rebuilds; segment addresses are not.
        doc_id: builder.add_text_field("doc_id", STRING | STORED),
        db_id: builder.add_i64_field("db_id", STORED),
        source: builder.add_text_field("source", STRING | STORED),
        url: builder.add_text_field("url", STORED),
        title: builder.add_text_field("title", text.clone()),
        body: builder.add_text_field("body", text),
        completeness: builder.add_text_field("completeness", STRING | STORED),
    };
    (builder.build(), fields)
}

/// One document of the corpus as it goes into the index.
pub struct IndexDocument {
    pub id: i64,
    pub source: String,
    pub native_id: String,
    pub url: String,
    pub title: String,
    pub body: String,
    pub completeness: String,
}

#[derive(Serialize, Deserialize)]
struct Manifest {
    analyzer_version: String,
    documents: u64,
    built_at: String,
}

pub struct SearchIndex {
    pub(super) index: Index,
    pub(super) reader: IndexReader,
    pub(super) fields: Fields,
    generation: String,
    documents: u64,
    vocabulary: OnceLock<Vocabulary>,
}

impl SearchIndex {
    fn open(root: &Path, generation: &str) -> Result<Self, Error> {
        let dir = root.join(generation);
        let manifest: Manifest = serde_json::from_slice(&fs::read(dir.join(MANIFEST))?)?;
        if manifest.analyzer_version != ANALYZER_VERSION {
            return Err(Error::AnalyzerMismatch {
                found: manifest.analyzer_version,
            });
        }
        Self::from_index(
            Index::open_in_dir(&dir)?,
            generation.to_owned(),
            manifest.documents,
        )
    }

    fn from_index(index: Index, generation: String, documents: u64) -> Result<Self, Error> {
        // Tokenizers are not stored in the index files; every open registers them again.
        index.tokenizers().register(ANALYZER_NAME, analyzer());
        let fields = schema().1;
        // The index never changes after the build: a new generation replaces it whole.
        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::Manual)
            .try_into()?;
        Ok(Self {
            index,
            reader,
            fields,
            generation,
            documents,
            vocabulary: OnceLock::new(),
        })
    }

    pub(super) fn vocabulary(&self) -> Result<&Vocabulary, Error> {
        if let Some(vocabulary) = self.vocabulary.get() {
            return Ok(vocabulary);
        }
        let read = Vocabulary::read(self, self.fields.body)?;
        Ok(self.vocabulary.get_or_init(|| read))
    }

    pub fn generation(&self) -> &str {
        &self.generation
    }

    pub fn documents(&self) -> u64 {
        self.documents
    }
}

/// The only writer of a new generation. Dropped without `finish`, it removes its directory.
pub struct Builder {
    dir: PathBuf,
    generation: String,
    index: Index,
    writer: Option<IndexWriter>,
    fields: Fields,
    documents: u64,
}

impl Builder {
    fn new(root: &Path) -> Result<Self, Error> {
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_millis());
        let generation = format!("gen-{millis}");
        let dir = root.join(&generation);
        fs::create_dir_all(&dir)?;
        let (schema, fields) = schema();
        let index =
            Index::create_in_dir(&dir, schema).inspect_err(|_| drop(fs::remove_dir_all(&dir)))?;
        index.tokenizers().register(ANALYZER_NAME, analyzer());
        let writer = index.writer_with_num_threads(2, WRITER_HEAP)?;
        Ok(Self {
            dir,
            generation,
            index,
            writer: Some(writer),
            fields,
            documents: 0,
        })
    }

    pub fn add(&mut self, doc: IndexDocument) -> Result<(), Error> {
        let f = self.fields;
        let mut document = TantivyDocument::default();
        document.add_text(f.doc_id, format!("{}:{}", doc.source, doc.native_id));
        document.add_i64(f.db_id, doc.id);
        document.add_text(f.source, doc.source);
        document.add_text(f.url, doc.url);
        document.add_text(f.title, doc.title);
        document.add_text(f.body, doc.body);
        document.add_text(f.completeness, doc.completeness);
        self.writer
            .as_mut()
            .expect("the writer lives until finish")
            .add_document(document)?;
        self.documents += 1;
        Ok(())
    }

    /// Commits, records the manifest and opens the result. Blocking: run off the async runtime.
    pub fn finish(mut self) -> Result<SearchIndex, Error> {
        let mut writer = self.writer.take().expect("the writer lives until finish");
        writer.commit()?;
        writer.wait_merging_threads()?;
        let manifest = Manifest {
            analyzer_version: ANALYZER_VERSION.into(),
            documents: self.documents,
            built_at: chrono::Utc::now().to_rfc3339(),
        };
        fs::write(
            self.dir.join(MANIFEST),
            serde_json::to_vec_pretty(&manifest)?,
        )?;
        let index =
            SearchIndex::from_index(self.index.clone(), self.generation.clone(), self.documents)?;
        // Kept: the directory now belongs to the installed generation.
        self.dir = PathBuf::new();
        Ok(index)
    }
}

impl Drop for Builder {
    fn drop(&mut self) {
        if !self.dir.as_os_str().is_empty() {
            drop(fs::remove_dir_all(&self.dir));
        }
    }
}

/// The index readers use, plus the rebuild lock. One handle per process.
pub struct SearchHandle {
    root: PathBuf,
    current: RwLock<Option<Arc<SearchIndex>>>,
    pub rebuilding: tokio::sync::Mutex<()>,
}

impl SearchHandle {
    /// Opens the current generation if there is one. A missing or unusable index is not
    /// fatal: the rest of the API works, and search says to rebuild.
    pub fn open(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        let current = match Self::read_current(&root) {
            Ok(index) => index.map(Arc::new),
            Err(error) => {
                tracing::warn!(%error, "search index is unusable; rebuild it via the API");
                None
            }
        };
        Self {
            root,
            current: RwLock::new(current),
            rebuilding: tokio::sync::Mutex::new(()),
        }
    }

    fn read_current(root: &Path) -> Result<Option<SearchIndex>, Error> {
        match fs::read_to_string(root.join(CURRENT)) {
            Ok(name) => SearchIndex::open(root, name.trim()).map(Some),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    pub fn current(&self) -> Option<Arc<SearchIndex>> {
        self.current.read().expect("lock is not poisoned").clone()
    }

    pub fn begin(&self) -> Result<Builder, Error> {
        fs::create_dir_all(&self.root)?;
        Builder::new(&self.root)
    }

    /// Makes the index current: the pointer file is replaced by rename, so a crash leaves
    /// either the old or the new generation, never a torn one.
    pub fn install(&self, index: SearchIndex) -> Result<Arc<SearchIndex>, Error> {
        let pointer = self.root.join(format!("{CURRENT}.tmp"));
        fs::write(&pointer, index.generation())?;
        fs::rename(&pointer, self.root.join(CURRENT))?;
        let index = Arc::new(index);
        *self.current.write().expect("lock is not poisoned") = Some(index.clone());
        self.prune(index.generation());
        Ok(index)
    }

    fn prune(&self, current: &str) {
        let Ok(entries) = fs::read_dir(&self.root) else {
            return;
        };
        let mut generations: Vec<String> = entries
            .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
            .filter(|name| name.starts_with("gen-") && name != current)
            .collect();
        generations.sort();
        let stale = generations.len().saturating_sub(KEPT_GENERATIONS - 1);
        for name in &generations[..stale] {
            drop(fs::remove_dir_all(self.root.join(name)));
        }
    }
}

impl SearchIndex {
    pub(super) fn stored_text(&self, doc: &TantivyDocument, field: Field) -> String {
        doc.get_first(field)
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_owned()
    }
}
