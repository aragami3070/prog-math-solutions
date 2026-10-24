use crate::{
    models::{
        crawls::{CollectedPage, FetchRecord, QueueLink, QueuedPage, RunStatus, StoredPage},
        sources::Source,
    },
    use_cases::crawls,
};
use sqlx::{Connection, PgPool, Postgres, pool::PoolConnection};
use std::time::Duration;

pub struct Persistence {
    pool: PgPool,
    source: Source,
    run_id: i64,
}

impl Persistence {
    pub fn new(pool: PgPool, source: Source, run_id: i64) -> Self {
        Self {
            pool,
            source,
            run_id,
        }
    }

    pub async fn check_lease(&self, guard: &mut PoolConnection<Postgres>) -> anyhow::Result<()> {
        let mut tx = guard.begin().await?;
        crawls::check_lease::execute(&mut tx).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn record_fetch(&self, fetch: FetchRecord<'_>) -> anyhow::Result<i64> {
        let mut tx = self.pool.begin().await?;
        let id = crawls::record_fetch::execute(&mut tx, fetch).await?;
        tx.commit().await?;
        Ok(id)
    }

    pub async fn fresh_page(
        &self,
        url: &str,
        fresh_for: Duration,
    ) -> anyhow::Result<Option<StoredPage>> {
        let mut tx = self.pool.begin().await?;
        let page = crawls::fresh_page::execute(&mut tx, url, fresh_for).await?;
        tx.commit().await?;
        Ok(page)
    }

    pub async fn next_page(&self) -> anyhow::Result<Option<QueuedPage>> {
        let mut tx = self.pool.begin().await?;
        let page = crawls::next_page::execute(&mut tx, self.run_id).await?;
        tx.commit().await?;
        Ok(page)
    }

    pub async fn complete_page(&self, page: CollectedPage) -> anyhow::Result<()> {
        let mut tx = self.pool.begin().await?;
        crawls::complete_page::execute(&mut tx, self.run_id, self.source, page).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn fail_page(
        &self,
        page_id: i64,
        error: &str,
        links: &[QueueLink],
    ) -> anyhow::Result<()> {
        let mut tx = self.pool.begin().await?;
        crawls::fail_page::execute(&mut tx, self.run_id, page_id, error, links).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn finish(&self, status: RunStatus, error: Option<&str>) -> anyhow::Result<()> {
        let mut tx = self.pool.begin().await?;
        crawls::finish::execute(&mut tx, self.run_id, self.source, status, error).await?;
        tx.commit().await?;
        Ok(())
    }
}
