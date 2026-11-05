use super::errors::Error;
use crate::{
    models::{
        pagination::Pagination,
        search::{SearchQuery, SearchResults, SearchScope},
    },
    search::{
        ANALYZER_VERSION, Filters, Lemmas, Options, SYNONYMS_VERSION, SearchHandle,
        query::MAX_DEPTH,
        semantic::{Request, Semantic, answer},
    },
};
use std::time::Instant;

pub async fn execute(
    handle: &SearchHandle,
    semantic: &Semantic,
    query: SearchQuery,
) -> Result<SearchResults, Error> {
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
        lemmas: query.lemmas,
    };
    let request = Request {
        text: query.q.clone(),
        filters,
        options,
        mode: query.mode,
        offset: pagination.offset() as usize,
        limit: query.page_size as usize,
    };
    let found = answer(semantic, index.clone(), request).await?;
    Ok(SearchResults {
        query: query.q,
        hits: found.hits,
        total: found.total,
        page: query.page,
        page_size: query.page_size,
        scope: SearchScope {
            mode: found.mode.as_str(),
            requested_mode: query.mode.as_str(),
            degraded: found.degraded,
            total_is_pool: found.total_is_pool,
            fuzzy: query.fuzzy,
            synonyms: query.synonyms,
            lemmas: query.lemmas,
            lemmas_version: Lemmas::shared().map(|lemmas| lemmas.version.clone()),
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
