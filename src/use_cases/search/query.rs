use super::errors::Error;
use crate::{
    models::{
        pagination::Pagination,
        search::{SearchQuery, SearchResults, SearchScope},
    },
    search::{
        ANALYZER_VERSION, Filters, Options, SYNONYMS_VERSION, SearchHandle, query::MAX_DEPTH,
    },
};
use std::time::Instant;

pub async fn execute(handle: &SearchHandle, query: SearchQuery) -> Result<SearchResults, Error> {
    let pagination = Pagination {
        page: query.page,
        page_size: query.page_size,
    };
    if !pagination.valid() {
        return Err(Error::InvalidPagination);
    }
    let index = handle.current().ok_or(Error::NotBuilt)?;
    let started = Instant::now();
    let filters = Filters {
        source: query.source.map(|source| source.as_str().to_owned()),
        complete_only: query.complete_only,
    };
    let options = Options {
        fuzzy: query.fuzzy,
        synonyms: query.synonyms,
    };
    let (text, offset, limit) = (
        query.q.clone(),
        pagination.offset() as usize,
        query.page_size as usize,
    );
    // Snippets tokenize whole documents: keep that off the async workers.
    let (index, found) = tokio::task::spawn_blocking(move || {
        let found = index.search(&text, &filters, options, offset, limit);
        (index, found)
    })
    .await?;
    let found = found?;
    Ok(SearchResults {
        query: query.q,
        hits: found.hits,
        total: found.total,
        page: query.page,
        page_size: query.page_size,
        scope: SearchScope {
            mode: "lexical",
            fuzzy: query.fuzzy,
            synonyms: query.synonyms,
            synonyms_version: SYNONYMS_VERSION,
            analyzer_version: ANALYZER_VERSION,
            generation: index.generation().to_owned(),
            indexed_documents: index.documents(),
            max_depth: MAX_DEPTH,
        },
        rewrite: found.rewrite,
        took_ms: started.elapsed().as_millis() as u64,
    })
}
