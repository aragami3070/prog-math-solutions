use std::time::Duration;

use anyhow::{Result, bail, ensure};
use reqwest::{Client, redirect::Policy};
use robotstxt::DefaultMatcher;
use sqlx::{Postgres, pool::PoolConnection};
use url::Url;

use super::discovery::normalize;
use super::{
    errors::{Blocked, Unavailable},
    persistence::Persistence,
    retry,
};
use crate::config::AGENT_PRODUCT;
use crate::models::{
    crawls::{FetchKind, FetchRecord},
    sources::Source,
};

const MAX_BYTES: usize = 2 * 1024 * 1024;

pub struct Response {
    pub status: u16,
    pub body: String,
    pub url: Url,
    pub fetch_id: i64,
    content_type: String,
    location: Option<String>,
}

pub struct Transport<'a> {
    client: Client,
    store: &'a Persistence,
    guard: &'a mut PoolConnection<Postgres>,
    source: Source,
    run_id: i64,
    delay: Duration,
    robots: String,
}

/// Why a result is worth retrying, if it is: the network failed or the server had a hiccup.
fn transient(result: &Result<Response>) -> Option<String> {
    match result {
        Ok(response) if matches!(response.status, 502..=504) => {
            Some(format!("HTTP {}", response.status))
        }
        Err(error) => error
            .downcast_ref::<reqwest::Error>()
            .filter(|e| e.is_timeout() || e.is_connect() || e.is_request() || e.is_body())
            .map(describe),
        _ => None,
    }
}

/// The error and its causes, e.g. "error sending request: connection refused".
fn describe(error: &reqwest::Error) -> String {
    let mut text = error.to_string();
    let mut source = std::error::Error::source(error);
    while let Some(cause) = source {
        text.push_str(": ");
        text.push_str(&cause.to_string());
        source = cause.source();
    }
    text
}

pub fn allowed(robots: &str, url: &str) -> bool {
    DefaultMatcher::default().one_agent_allowed_by_robots(robots, AGENT_PRODUCT, url)
}

impl<'a> Transport<'a> {
    pub fn new(
        store: &'a Persistence,
        guard: &'a mut PoolConnection<Postgres>,
        source: Source,
        run_id: i64,
        user_agent: &str,
        delay: Duration,
    ) -> Result<Self> {
        Ok(Self {
            client: Client::builder()
                .user_agent(user_agent)
                .redirect(Policy::none())
                .connect_timeout(Duration::from_secs(10))
                .timeout(Duration::from_secs(25))
                .no_proxy()
                .build()?,
            store,
            guard,
            source,
            run_id,
            delay,
            robots: String::new(),
        })
    }

    pub async fn load_robots(&mut self) -> Result<()> {
        let response = self
            .request_retrying(
                Url::parse(&format!("{}/robots.txt", self.source.origin()))?,
                FetchKind::Robots,
            )
            .await?;
        self.robots = match response.status {
            404 => String::new(),
            200 => {
                ensure!(
                    !response.body.trim_start().starts_with('<')
                        && !response.content_type.contains("text/html"),
                    "robots returned HTML, access remains disabled"
                );
                response.body
            }
            status => bail!("robots unavailable (HTTP {status}); not treating it as permission"),
        };
        // Conservative extension: use the maximum advertised crawl-delay, even
        // in another group. The matcher handles Allow/Disallow group precedence.
        for line in self.robots.lines() {
            let Some((key, value)) = line.split_once(':') else {
                continue;
            };
            if key.trim().eq_ignore_ascii_case("crawl-delay") {
                let value = value
                    .split('#')
                    .next()
                    .unwrap_or("")
                    .trim()
                    .parse::<f64>()?;
                ensure!(
                    value.is_finite() && (0.0..=300.0).contains(&value),
                    "unsupported crawl-delay; source needs manual review"
                );
                self.delay = self.delay.max(Duration::from_secs_f64(value));
            }
        }
        Ok(())
    }

    pub async fn document(&mut self, url: Url) -> Result<Response> {
        let expected = normalize(self.source, url.as_str())?;
        let mut current = url;
        for _ in 0..4 {
            let normalized = normalize(self.source, current.as_str())?;
            ensure!(normalized == expected, "redirect changed page identity");
            ensure!(
                allowed(&self.robots, current.as_str()),
                "robots disallows document URL"
            );
            let response = self
                .request_retrying(current.clone(), FetchKind::Document)
                .await?;
            if (300..400).contains(&response.status) {
                let location = response
                    .location
                    .as_deref()
                    .ok_or_else(|| anyhow::anyhow!("redirect has no Location"))?;
                current = current.join(location)?;
                continue;
            }
            ensure!(
                response.status == 200,
                "document returned HTTP {}",
                response.status
            );
            ensure!(
                response.content_type.contains("text/html"),
                "document is not HTML"
            );
            let lower = response.body.to_ascii_lowercase();
            if lower.contains("cf-chl-") || lower.contains("<title>just a moment") {
                return Err(Blocked("challenge received; no bypass attempted".into()).into());
            }
            return Ok(response);
        }
        bail!("too many redirects")
    }

    pub fn allowed(&self, url: &Url) -> bool {
        allowed(&self.robots, url.as_str())
    }

    /// Retries connection failures, timeouts and 502/503/504 with growing waits. A refusal
    /// (403/429, challenge) is never retried; it stops the source as before.
    async fn request_retrying(&mut self, url: Url, kind: FetchKind) -> Result<Response> {
        let mut attempt = 1;
        loop {
            let result = self.request(url.clone(), kind).await;
            match retry::step(transient(&result), attempt, self.delay) {
                retry::Step::Done => return result,
                retry::Step::Wait(wait) => {
                    tracing::warn!(attempt, ?wait, %url, "transient failure, retrying");
                    tokio::time::sleep(wait).await;
                    attempt += 1;
                }
                retry::Step::GiveUp(reason) => return Err(Unavailable(reason).into()),
            }
        }
    }

    async fn request(&mut self, url: Url, kind: FetchKind) -> Result<Response> {
        // The first request also waits, so serial runs and restarts cannot burst.
        tokio::time::sleep(self.delay).await;
        // Losing the connection means losing the distributed crawl lock: fail closed.
        self.store.check_lease(self.guard).await?;
        let result = self.client.get(url.clone()).send().await;
        let mut response = match result {
            Ok(response) => response,
            Err(error) => {
                self.store
                    .record_fetch(FetchRecord {
                        run_id: self.run_id,
                        url: url.as_str(),
                        kind,
                        status: None,
                        content_type: None,
                        body: None,
                        error: Some(&error.to_string()),
                    })
                    .await?;
                return Err(error.into());
            }
        };
        let status = response.status().as_u16();
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|h| h.to_str().ok())
            .unwrap_or("")
            .to_string();
        let location = response
            .headers()
            .get("location")
            .and_then(|h| h.to_str().ok())
            .map(str::to_owned);
        if status == 403 || status == 429 {
            let retry_after = response
                .headers()
                .get("retry-after")
                .and_then(|h| h.to_str().ok())
                .unwrap_or("unspecified");
            let reason = format!(
                "HTTP {status}; Retry-After: {retry_after}; operator review required before resume"
            );
            self.store
                .record_fetch(FetchRecord {
                    run_id: self.run_id,
                    url: url.as_str(),
                    kind,
                    status: Some(status),
                    content_type: Some(&content_type),
                    body: None,
                    error: Some(&reason),
                })
                .await?;
            return Err(Blocked(reason).into());
        }
        let read_result: Result<String> = async {
            let mut bytes = Vec::new();
            while let Some(chunk) = response.chunk().await? {
                ensure!(
                    bytes.len() + chunk.len() <= MAX_BYTES,
                    "response exceeds 2 MiB"
                );
                bytes.extend_from_slice(&chunk);
            }
            Ok(String::from_utf8(bytes)?)
        }
        .await;
        match read_result {
            Ok(body) => {
                let fetch_id = self
                    .store
                    .record_fetch(FetchRecord {
                        run_id: self.run_id,
                        url: url.as_str(),
                        kind,
                        status: Some(status),
                        content_type: Some(&content_type),
                        body: Some(&body),
                        error: None,
                    })
                    .await?;
                Ok(Response {
                    status,
                    content_type,
                    location,
                    body,
                    url,
                    fetch_id,
                })
            }
            Err(error) => {
                self.store
                    .record_fetch(FetchRecord {
                        run_id: self.run_id,
                        url: url.as_str(),
                        kind,
                        status: Some(status),
                        content_type: Some(&content_type),
                        body: None,
                        error: Some(&error.to_string()),
                    })
                    .await?;
                Err(error)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn robots_groups_and_wildcards() {
        let robots = "User-agent: *\nDisallow: /search\nDisallow: /*?*\nAllow: /t/\nUser-agent: rustbot\nDisallow: /\n";
        assert!(allowed(robots, "https://internals.rust-lang.org/t/name/6"));
        assert!(!allowed(robots, "https://internals.rust-lang.org/search"));
        assert!(!allowed(
            "User-agent: RustKnowledgeCollector\nDisallow: /",
            "https://internals.rust-lang.org/t/name/6"
        ));
    }

    #[tokio::test]
    async fn only_network_failures_and_gateway_errors_are_transient() {
        // Nothing listens on port 1, so the connection is refused at once.
        let error = reqwest::Client::new()
            .get("http://127.0.0.1:1/")
            .send()
            .await
            .unwrap_err();
        let reason = transient(&Err(error.into())).expect("connection failure is transient");
        assert!(reason.starts_with("error sending request"), "{reason}");

        let response = |status| {
            Ok(Response {
                status,
                body: String::new(),
                url: Url::parse("https://internals.rust-lang.org/t/1").unwrap(),
                fetch_id: 1,
                content_type: "text/html".into(),
                location: None,
            })
        };
        for status in [502, 503, 504] {
            assert_eq!(transient(&response(status)), Some(format!("HTTP {status}")));
        }
        for status in [200, 301, 404, 500] {
            assert!(transient(&response(status)).is_none(), "{status}");
        }
        assert!(transient(&Err(Blocked("HTTP 429".into()).into())).is_none());
        assert!(transient(&Err(anyhow::anyhow!("document is not HTML"))).is_none());
    }
}
