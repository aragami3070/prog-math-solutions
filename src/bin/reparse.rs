//! Rebuilds pages, posts and links from stored HTML. Makes no requests to source websites.
use anyhow::{Result, bail};
use rust_knowledge::{
    config::Config,
    use_cases::{
        crawls::{self, errors::Error as CrawlError},
        sources::{errors::Error, reparse},
    },
};
use sqlx::{Connection, postgres::PgPoolOptions};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    if let Err(error) = dotenvy::from_path(".env")
        && !error.not_found()
    {
        bail!("Could not load .env; check file permissions and syntax");
    }
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "rust_knowledge=info,reparse=info".into()),
        )
        .init();
    let config = Config::from_env()?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&config.database_url)
        .await?;
    // The crawl lock keeps this from racing a running collector.
    let mut guard = pool.acquire().await?;
    guard.close_on_drop();
    let mut tx = guard.begin().await?;
    match crawls::acquire_lock::execute(&mut tx).await {
        Ok(()) => {}
        Err(CrawlError::Busy) => bail!("a crawl is running; stop it before reparsing"),
        Err(error) => return Err(error.into()),
    }
    tx.commit().await?;

    let mut tx = pool.begin().await?;
    let ids = reparse::fetch_ids(&mut tx).await?;
    tx.commit().await?;
    tracing::info!(fetches = ids.len(), "reparsing stored responses");
    let (mut saved, mut skipped, mut failed) = (0u64, 0u64, 0u64);
    for (index, id) in ids.iter().enumerate() {
        let mut tx = pool.begin().await?;
        match reparse::execute(&mut tx, *id).await {
            Ok(reparse::Outcome::Saved) => {
                tx.commit().await?;
                saved += 1;
            }
            Ok(reparse::Outcome::Skipped) => skipped += 1,
            Err(error @ Error::Reparse { .. }) => {
                tracing::warn!(%error, "skipped unparseable response");
                failed += 1;
            }
            Err(error) => return Err(error.into()),
        }
        if (index + 1) % 500 == 0 {
            tracing::info!(done = index + 1, total = ids.len(), "progress");
        }
    }
    tracing::info!(saved, skipped, failed, "reparse finished");
    Ok(())
}
