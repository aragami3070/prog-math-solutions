use super::errors::Error;
use crate::{
    crawler::{Crawler, discovery},
    models::crawls::{CrawlPlan, CrawlRequest, CrawlRun, QueueLink},
    repository::{crawls, sources},
};
use sqlx::{Postgres, Transaction};

// No database or network work: invalid input/disabled collection is rejected
// before the router checks a connection out of the pool.
pub fn prepare(crawler: &Crawler, request: CrawlRequest) -> Result<CrawlPlan, Error> {
    let settings = crawler.settings().ok_or(Error::Disabled)?;
    Ok(CrawlPlan {
        source: request.source,
        user_agent: settings.user_agent,
        delay: settings.delay,
        fresh_for: settings.fresh_for,
    })
}

pub async fn execute(
    tx: &mut Transaction<'_, Postgres>,
    plan: &CrawlPlan,
) -> Result<CrawlRun, Error> {
    if !crawls::try_lock(tx).await? {
        return Err(Error::Busy);
    }
    let (paused, reason) = sources::state(tx, plan.source).await?;
    if paused {
        return Err(Error::Paused(reason.unwrap_or_default()));
    }
    crawls::recover_interrupted(tx).await?;
    if let Some(run) = crawls::resumable(tx, plan.source).await? {
        crawls::reset_failed_pages(tx, run.id).await?;
        return Ok(crawls::restart(tx, run.id).await?);
    }
    let seeds: Vec<_> = discovery::seeds(plan.source)
        .iter()
        .map(|seed| QueueLink::same(*seed))
        .collect();
    let mut run = crawls::insert(tx, plan.source, 0).await?;
    crawls::enqueue(tx, run.id, &seeds).await?;
    run.requested = seeds.len() as i32;
    Ok(run)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::sources::Source;
    use std::time::Duration;

    #[tokio::test]
    async fn prepares_source_without_url_budget() {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://unused:unused@localhost/unused")
            .unwrap();
        let crawler = Crawler::new(
            pool,
            Some("fixture-agent".into()),
            Duration::from_secs(5),
            Duration::from_secs(3600),
        );
        let plan = prepare(
            &crawler,
            CrawlRequest {
                source: Source::Internals,
            },
        )
        .unwrap();
        assert_eq!(plan.source, Source::Internals);
    }
}
