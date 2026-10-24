//! Lexical search over the stored corpus: a Tantivy projection of the `documents` table.
//!
//! The database stays the source of truth. An index is built whole into a fresh generation
//! directory and made current by atomically rewriting `CURRENT`, so readers never see a
//! half-built index and an old generation stays valid until the new one is ready.

mod analyzer;
mod index;
pub mod query;

pub use analyzer::{ANALYZER_NAME, ANALYZER_VERSION, analyzer};
pub use index::{Builder, Error, IndexDocument, SearchHandle, SearchIndex};
pub use query::{Filters, Hit, Results, SnippetPart};

#[cfg(test)]
mod tests;
