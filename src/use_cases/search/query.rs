use super::errors::Error;
use crate::{
    experiment::{EXPERIMENT, valid_session},
    models::{
        events::ExperimentInfo,
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
    mut query: SearchQuery,
) -> Result<SearchResults, Error> {
    let pagination = Pagination {
        page: query.page,
        page_size: query.page_size,
    };
    if !pagination.valid() {
        return Err(Error::InvalidPagination);
    }
    // In the experiment the arm decides the configuration: one change per experiment.
    let experiment = match query.session.as_deref() {
        None => None,
        Some(session) if valid_session(session) => {
            let variant = EXPERIMENT.assign(session);
            query.mode = variant.mode;
            (query.fuzzy, query.fuzzy_match, query.synonyms, query.lemmas) =
                (false, false, false, false);
            Some(ExperimentInfo {
                id: EXPERIMENT.id,
                version: EXPERIMENT.version,
                variant: variant.name,
                query_id: None,
            })
        }
        Some(_) => {
            return Err(Error::InvalidQuery(
                "session must be 8..64 characters of A-Za-z0-9_-",
            ));
        }
    };
    for year in [query.year_from, query.year_to].into_iter().flatten() {
        if !(1990..=2100).contains(&year) {
            return Err(Error::InvalidQuery("years must be 1990..2100"));
        }
    }
    if query.year_from > query.year_to && query.year_to.is_some() {
        return Err(Error::InvalidQuery("year_from is after year_to"));
    }
    let index = handle.current().ok_or(Error::NotBuilt)?;
    if query
        .generation
        .as_deref()
        .is_some_and(|expected| expected != index.generation())
    {
        return Err(Error::GenerationChanged);
    }
    let started = Instant::now();
    let request_source = query.source.map(|source| source.as_str().to_owned());
    let filters = Filters {
        source: request_source.clone(),
        complete_only: query.complete_only,
        year_from: query.year_from,
        year_to: query.year_to,
    };
    let options = Options {
        fuzzy: query.fuzzy,
        fuzzy_match: query.fuzzy_match,
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
            fuzzy_match: query.fuzzy_match,
            synonyms: query.synonyms,
            lemmas: query.lemmas,
            source: request_source,
            complete_only: query.complete_only,
            year_from: query.year_from,
            year_to: query.year_to,
            lemmas_version: Lemmas::shared().map(|lemmas| lemmas.version.clone()),
            synonyms_version: SYNONYMS_VERSION,
            analyzer_version: ANALYZER_VERSION,
            generation: index.generation().to_owned(),
            indexed_documents: index.documents(),
            max_depth: MAX_DEPTH,
        },
        rewrite: found.rewrite,
        experiment,
        took_ms: started.elapsed().as_millis() as u64,
    })
}
