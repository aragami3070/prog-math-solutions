pub mod discovery;
pub mod errors;
mod http;
mod persistence;
pub mod sources;

use crate::models::{
    crawls::{CollectedPage, CrawlPlan, QueueLink, RunStatus},
    sources::Source,
};
use anyhow::Result;
use errors::Blocked;
use http::Transport;
use persistence::Persistence;
use sqlx::{PgPool, Postgres, pool::PoolConnection};
use std::{sync::Arc, time::Duration};
use tokio_util::{sync::CancellationToken, task::TaskTracker};
use url::Url;

pub struct Settings {
    pub user_agent: Arc<str>,
    pub delay: Duration,
    pub fresh_for: Duration,
}

#[derive(Clone)]
pub struct Crawler {
    pool: PgPool,
    user_agent: Option<Arc<str>>,
    delay: Duration,
    fresh_for: Duration,
    tasks: TaskTracker,
    stop: CancellationToken,
}

impl Crawler {
    pub fn new(
        pool: PgPool,
        user_agent: Option<String>,
        delay: Duration,
        fresh_for: Duration,
    ) -> Self {
        Self {
            pool,
            user_agent: user_agent.map(Into::into),
            delay,
            fresh_for,
            tasks: TaskTracker::new(),
            stop: CancellationToken::new(),
        }
    }

    pub fn settings(&self) -> Option<Settings> {
        if self.stop.is_cancelled() {
            return None;
        }
        Some(Settings {
            user_agent: self.user_agent.clone()?,
            delay: self.delay,
            fresh_for: self.fresh_for,
        })
    }

    // The router calls this only after committing the run transaction.
    pub fn spawn(&self, run_id: i64, plan: CrawlPlan, mut guard: PoolConnection<Postgres>) {
        let persistence = Persistence::new(self.pool.clone(), plan.source, run_id);
        let stop = self.stop.clone();
        self.tasks.spawn(async move {
            let result = tokio::select! {
                _ = stop.cancelled() => Err(anyhow::anyhow!("server shutdown")),
                result = collect(&persistence, &mut guard, run_id, plan) => result,
            };
            let (status, error) = match result {
                Ok(had_errors) => (
                    if had_errors {
                        RunStatus::Failed
                    } else {
                        RunStatus::Completed
                    },
                    None,
                ),
                Err(error) => {
                    let status = if stop.is_cancelled() {
                        RunStatus::Interrupted
                    } else if error.downcast_ref::<Blocked>().is_some() {
                        RunStatus::Blocked
                    } else {
                        RunStatus::Failed
                    };
                    (status, Some(error.to_string()))
                }
            };
            if let Err(error) = persistence.finish(status, error.as_deref()).await {
                tracing::error!(run_id, %error, "could not finish crawl run");
            }
            drop(guard);
        });
    }

    pub async fn shutdown(&self) {
        self.stop.cancel();
        self.tasks.close();
        self.tasks.wait().await;
    }
}

async fn collect(
    persistence: &Persistence,
    guard: &mut PoolConnection<Postgres>,
    run_id: i64,
    plan: CrawlPlan,
) -> Result<bool> {
    let mut transport = Transport::new(
        persistence,
        guard,
        plan.source,
        run_id,
        &plan.user_agent,
        plan.delay,
    )?;
    transport.load_robots().await?;
    let mut had_errors = false;
    while let Some(page) = persistence.next_page().await? {
        let mut discovered = Vec::new();
        let outcome: Result<()> = async {
            // A recently stored page is not requested again, but its links still lead the crawl on.
            let identity = discovery::normalize(plan.source, &page.url)?;
            if discovery::is_document(plan.source, &identity)
                && let Some(stored) = persistence
                    .fresh_page(identity.as_str(), plan.fresh_for)
                    .await?
            {
                discovered = discover(&transport, plan.source, &identity, &stored.body);
                persistence
                    .complete_page(CollectedPage {
                        id: page.id,
                        fetch_id: stored.fetch_id,
                        parsed: None,
                        links: discovered.clone(),
                    })
                    .await?;
                tracing::debug!(url = %identity, "reused stored page");
                return Ok(());
            }
            let url = discovery::fetch_url(plan.source, &page.fetch_url)?;
            let response = transport.document(url).await?;
            discovered = discover(&transport, plan.source, &response.url, &response.body);
            let parsed = if discovery::is_document(plan.source, &response.url) {
                Some(sources::parse(
                    plan.source,
                    response.url.as_str(),
                    &response.body,
                )?)
            } else {
                anyhow::ensure!(
                    !discovered.is_empty(),
                    "navigation page contains no supported HTML links"
                );
                None
            };
            persistence
                .complete_page(CollectedPage {
                    id: page.id,
                    fetch_id: response.fetch_id,
                    parsed,
                    links: discovered.clone(),
                })
                .await?;
            Ok(())
        }
        .await;
        if let Err(error) = outcome {
            had_errors = true;
            persistence
                .fail_page(page.id, &error.to_string(), &discovered)
                .await?;
            if error.downcast_ref::<Blocked>().is_some()
                || error.chain().any(|cause| cause.is::<sqlx::Error>())
            {
                return Err(error);
            }
        }
    }
    Ok(had_errors)
}

/// Supported links of a page that robots.txt lets the crawler request.
fn discover(transport: &Transport<'_>, source: Source, base: &Url, html: &str) -> Vec<QueueLink> {
    discovery::links(source, base, html)
        .into_iter()
        .filter(|link| Url::parse(&link.fetch_url).is_ok_and(|url| transport.allowed(&url)))
        .collect()
}
