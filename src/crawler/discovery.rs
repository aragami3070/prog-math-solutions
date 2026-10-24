//! Source-specific crawl scope. HTML links are candidates, never permission to fetch.
use crate::models::{crawls::QueueLink, sources::Source};
use anyhow::{Result, bail, ensure};
use scraper::{Html, Selector};
use std::collections::HashMap;
use url::Url;

pub fn seed(source: Source) -> &'static str {
    match source {
        Source::Rfc => "https://rust-lang.github.io/rfcs/",
        Source::Internals => "https://internals.rust-lang.org/",
    }
}

fn number(value: &str) -> Result<u64> {
    ensure!(
        !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit()),
        "invalid page ID"
    );
    Ok(value.parse()?)
}

pub fn normalize(source: Source, input: &str) -> Result<Url> {
    ensure!(input.len() <= 2048, "URL too long");
    let mut url = Url::parse(input)?;
    let origin = Url::parse(source.origin())?;
    ensure!(
        url.origin() == origin.origin() && url.username().is_empty() && url.password().is_none(),
        "URL outside source"
    );
    ensure!(!url.path().contains('%'), "encoded paths unsupported");
    url.set_fragment(None);
    match source {
        Source::Rfc => {
            ensure!(url.query().is_none(), "RFC query unsupported");
            if matches!(url.path(), "/rfcs/" | "/rfcs/index.html") {
                url.set_path("/rfcs/");
            } else if url.path() != "/rfcs/toc.html" {
                validate_url(source, url.as_str())?;
            }
        }
        Source::Internals => {
            let mut page = None;
            for (key, value) in url.query_pairs() {
                match key.as_ref() {
                    "page" if page.is_none() => page = Some(number(&value)?),
                    "no_definitions" if value == "true" => {}
                    _ => bail!("unsupported query"),
                }
            }
            let parts: Vec<_> = url.path().trim_matches('/').split('/').collect();
            let path = match parts.as_slice() {
                [""] | ["latest"] => "/".to_string(),
                ["categories"] if page.is_none() => "/categories".to_string(),
                ["c", slug, id] if !slug.is_empty() && number(id)? > 0 => format!("/c/{slug}/{id}"),
                ["t", id] if number(id)? > 0 => format!("/t/{}", number(id)?),
                ["t", _, id] if number(id)? > 0 => format!("/t/{}", number(id)?),
                // Post permalinks lead to the topic; HTML continuation uses ?page=N.
                ["t", _, id, post] if number(id)? > 0 && number(post)? > 0 => {
                    format!("/t/{}", number(id)?)
                }
                _ => bail!("not a supported HTML route"),
            };
            url.set_path(&path);
            url.set_query(None);
            let first_page = if path.starts_with("/t/") { 1 } else { 0 };
            if let Some(page) = page.filter(|p| *p > first_page) {
                if path == "/" {
                    url.set_path("/latest");
                }
                url.set_query(Some(&format!("page={page}")));
            }
        }
    }
    Ok(url)
}

pub fn validate_url(source: Source, input: &str) -> Result<(Url, String)> {
    let mut url = Url::parse(input)?;
    let origin = Url::parse(source.origin())?;
    ensure!(
        url.scheme() == "https"
            && url.host_str() == origin.host_str()
            && url.port_or_known_default() == Some(443),
        "URL must belong to the selected HTTPS source"
    );
    ensure!(
        url.username().is_empty() && url.password().is_none() && url.query().is_none(),
        "credentials and query parameters are not accepted"
    );
    ensure!(!url.path().contains('%'), "encoded paths are not accepted");
    url.set_fragment(None);
    let parts: Vec<_> = url.path().trim_matches('/').split('/').collect();
    let native_id = match source {
        Source::Internals => {
            ensure!(
                parts.first() == Some(&"t") && (parts.len() == 2 || parts.len() == 3),
                "expected an Internals topic URL, without post pagination"
            );
            parts.last().unwrap().to_string()
        }
        Source::Rfc => {
            ensure!(
                parts.len() == 2 && parts[0] == "rfcs" && parts[1].ends_with(".html"),
                "expected an RFC HTML document"
            );
            let Some((id, _)) = parts[1].split_once('-') else {
                bail!("expected a numbered RFC")
            };
            id.to_string()
        }
    };
    ensure!(
        !native_id.is_empty() && native_id.chars().all(|c| c.is_ascii_digit()),
        "invalid document ID"
    );
    let id: u64 = native_id.parse()?;
    ensure!(id > 0, "document ID must be positive");
    Ok((url, id.to_string()))
}

pub fn is_document(source: Source, url: &Url) -> bool {
    match source {
        Source::Internals => url.path().starts_with("/t/"),
        Source::Rfc => validate_url(source, url.as_str()).is_ok(),
    }
}

/// The URL to request for a page: its canonical identity, plus the topic slug when the
/// link carried one, so Discourse does not answer every request with a redirect.
pub fn fetch_url(source: Source, input: &str) -> Result<Url> {
    let mut url = normalize(source, input)?;
    if source == Source::Internals {
        let parsed = Url::parse(input)?;
        let parts: Vec<_> = parsed.path().trim_matches('/').split('/').collect();
        // A numeric second segment may be a topic ID followed by a post number, not a slug.
        if let ["t", slug, id, ..] = parts.as_slice()
            && !slug.bytes().all(|b| b.is_ascii_digit())
        {
            url.set_path(&format!("/t/{slug}/{}", number(id)?));
        }
    }
    Ok(url)
}

pub fn links(source: Source, base: &Url, html: &str) -> Vec<QueueLink> {
    let doc = Html::parse_document(html);
    let selector = Selector::parse("a[href], link[rel~='next'][href], iframe[src]").unwrap();
    let mut found: Vec<QueueLink> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    for node in doc.select(&selector) {
        let Some(href) = node
            .value()
            .attr("href")
            .or_else(|| node.value().attr("src"))
        else {
            continue;
        };
        let Ok(joined) = base.join(href) else {
            continue;
        };
        let (Ok(identity), Ok(fetch)) = (
            normalize(source, joined.as_str()),
            fetch_url(source, joined.as_str()),
        ) else {
            continue;
        };
        let (identity, fetch) = (identity.to_string(), fetch.to_string());
        match index.get(&identity) {
            // The first sighting may have no slug; keep the better one.
            Some(&i) => {
                if found[i].fetch_url == found[i].url {
                    found[i].fetch_url = fetch;
                }
            }
            None => {
                index.insert(identity.clone(), found.len());
                found.push(QueueLink {
                    url: identity,
                    fetch_url: fetch,
                });
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_allowlist_rejects_non_documents() {
        for url in [
            "http://internals.rust-lang.org/t/name/6",
            "https://localhost/t/name/6",
            "https://internals.rust-lang.org/search",
            "https://internals.rust-lang.org/t/name/6/2",
            "https://internals.rust-lang.org/t/name/6?x=1",
            "https://u:p@internals.rust-lang.org/t/name/6",
        ] {
            assert!(validate_url(Source::Internals, url).is_err(), "{url}");
        }
        assert!(validate_url(Source::Rfc, "https://rust-lang.github.io/rfcs/print.html").is_err());
        assert_eq!(
            validate_url(
                Source::Rfc,
                "https://rust-lang.github.io/rfcs/0001-private-fields.html#heading"
            )
            .unwrap()
            .1,
            "1"
        );
    }
    #[test]
    fn discovers_navigation_and_deduplicates_topics() {
        let urls = links(
            Source::Internals,
            &Url::parse(seed(Source::Internals)).unwrap(),
            r#"
            <a href='/t/one/42#post_1'>a</a><a href='/t/42'>b</a>
            <a href='/t/one/42/8'>post</a><link rel='next' href='/t/one/42?page=2'>
            <a href='/latest?no_definitions=true&amp;page=1'>next</a>
            <a href='/search?q=x'>search</a><a href='/u/someone'>user</a>
            <a href='https://example.org/t/42'>external</a><a href='/t/42?api_key=secret'>bad</a>
        "#,
        );
        let pairs: Vec<_> = urls
            .iter()
            .map(|l| (l.url.as_str(), l.fetch_url.as_str()))
            .collect();
        assert_eq!(
            pairs,
            vec![
                (
                    "https://internals.rust-lang.org/t/42",
                    "https://internals.rust-lang.org/t/one/42"
                ),
                (
                    "https://internals.rust-lang.org/t/42?page=2",
                    "https://internals.rust-lang.org/t/one/42?page=2"
                ),
                (
                    "https://internals.rust-lang.org/latest?page=1",
                    "https://internals.rust-lang.org/latest?page=1"
                ),
            ]
        );
    }

    #[test]
    fn later_sighting_with_a_slug_replaces_a_bare_link() {
        let urls = links(
            Source::Internals,
            &Url::parse(seed(Source::Internals)).unwrap(),
            "<a href='/t/42'>bare</a><a href='/t/the-slug/42'>slug</a>",
        );
        assert_eq!(urls.len(), 1);
        assert_eq!(urls[0].url, "https://internals.rust-lang.org/t/42");
        assert_eq!(
            urls[0].fetch_url,
            "https://internals.rust-lang.org/t/the-slug/42"
        );
    }

    #[test]
    fn fetch_url_keeps_a_real_slug_but_not_a_topic_id_or_post_number() {
        let fetch = |input: &str| fetch_url(Source::Internals, input).unwrap().to_string();
        assert_eq!(
            fetch("https://internals.rust-lang.org/t/the-slug/42/8?page=3"),
            "https://internals.rust-lang.org/t/the-slug/42?page=3"
        );
        assert_eq!(
            fetch("https://internals.rust-lang.org/t/42"),
            "https://internals.rust-lang.org/t/42"
        );
        // `/t/2024/7` is ambiguous (slug 2024 or topic 2024 post 7): never invent a slug.
        assert_eq!(
            fetch("https://internals.rust-lang.org/t/2024/7"),
            "https://internals.rust-lang.org/t/7"
        );
        assert_eq!(
            fetch_url(
                Source::Rfc,
                "https://rust-lang.github.io/rfcs/3058-try-trait-v2.html#x"
            )
            .unwrap()
            .as_str(),
            "https://rust-lang.github.io/rfcs/3058-try-trait-v2.html"
        );
    }

    #[test]
    fn rfc_scope_includes_toc_iframe_but_not_other_books() {
        let urls = links(
            Source::Rfc,
            &Url::parse(seed(Source::Rfc)).unwrap(),
            r#"
            <iframe src='toc.html'></iframe><a href='3058-try-trait-v2.html#x'>RFC</a>
            <a href='print.html'>print</a><a href='/book/'>book</a>
        "#,
        );
        assert_eq!(urls.len(), 2);
        assert_eq!(urls[0].url, "https://rust-lang.github.io/rfcs/toc.html");
        assert_eq!(urls[0].url, urls[0].fetch_url);
        assert!(
            normalize(
                Source::Internals,
                "https://internals.rust-lang.org.evil/t/1"
            )
            .is_err()
        );
    }
}
