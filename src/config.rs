use std::{env, net::SocketAddr, path::PathBuf, time::Duration};

use anyhow::{Context, Result, ensure};

/// Product token of the crawler: the User-Agent prefix and the robots.txt group it obeys.
pub const AGENT_PRODUCT: &str = "RustKnowledgeCollector";

#[derive(Clone)]
pub struct Config {
    pub database_url: String,
    pub bind: SocketAddr,
    pub web_dist: PathBuf,
    pub crawler_user_agent: Option<String>,
    pub crawl_delay: Duration,
    /// A stored page younger than this is reused instead of requested again; zero disables reuse.
    pub fresh_for: Duration,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let seconds: u64 = env::var("CRAWL_DELAY_SECONDS")
            .unwrap_or_else(|_| "5".into())
            .parse()
            .context("invalid CRAWL_DELAY_SECONDS")?;
        ensure!(
            (5..=300).contains(&seconds),
            "crawl delay must be 5..300 seconds"
        );
        let fresh_for =
            parse_fresh_days(&env::var("CRAWL_FRESH_DAYS").unwrap_or_else(|_| "7".into()))?;
        let agent = optional("CRAWLER_USER_AGENT");
        if let Some(agent) = &agent {
            ensure!(
                agent
                    .strip_prefix(AGENT_PRODUCT)
                    .is_some_and(|rest| rest.starts_with('/'))
                    && (agent.contains("mailto:") || agent.contains("https://")),
                "CRAWLER_USER_AGENT must start with {AGENT_PRODUCT}/ and contain a real contact (mailto: or https://)"
            );
            reqwest::header::HeaderValue::from_str(agent).context("invalid crawler User-Agent")?;
        }
        Ok(Self {
            database_url: env::var("DATABASE_URL").context("DATABASE_URL is required")?,
            bind: parse_bind(&env::var("HTTP_BIND").unwrap_or_else(|_| "127.0.0.1:8080".into()))?,
            web_dist: env::var("WEB_DIST")
                .unwrap_or_else(|_| "web/dist".into())
                .into(),
            crawler_user_agent: agent,
            crawl_delay: Duration::from_secs(seconds),
            fresh_for,
        })
    }
}

fn parse_fresh_days(value: &str) -> Result<Duration> {
    let days: u64 = value.parse().context("invalid CRAWL_FRESH_DAYS")?;
    ensure!(days <= 365, "CRAWL_FRESH_DAYS must be 0..365");
    Ok(Duration::from_secs(days * 24 * 60 * 60))
}

fn parse_bind(value: &str) -> Result<SocketAddr> {
    let address: SocketAddr = value.parse().context("invalid HTTP_BIND")?;
    ensure!(
        address.ip().is_loopback(),
        "HTTP_BIND must use a loopback address for the local API"
    );
    Ok(address)
}

fn optional(key: &str) -> Option<String> {
    env::var(key).ok().filter(|value| !value.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::{parse_bind, parse_fresh_days};
    use std::time::Duration;

    #[test]
    fn local_api_only_accepts_loopback_bind_addresses() {
        for value in ["127.0.0.1:8080", "[::1]:8080"] {
            assert!(parse_bind(value).is_ok(), "{value}");
        }
        for value in ["0.0.0.0:8080", "[::]:8080", "192.168.1.2:8080", "invalid"] {
            assert!(parse_bind(value).is_err(), "{value}");
        }
    }

    #[test]
    fn page_reuse_window_is_bounded_and_can_be_disabled() {
        assert_eq!(parse_fresh_days("0").unwrap(), Duration::ZERO);
        assert_eq!(
            parse_fresh_days("7").unwrap(),
            Duration::from_secs(7 * 86_400)
        );
        for value in ["366", "-1", "1.5", "week", ""] {
            assert!(parse_fresh_days(value).is_err(), "{value:?}");
        }
    }
}
