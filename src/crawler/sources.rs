use anyhow::{Result, bail, ensure};
use chrono::{DateTime, Utc};
use scraper::{ElementRef, Html, Selector};
use std::collections::HashSet;
use url::Url;

use crate::models::sources::{ParsedLink, ParsedPage, ParsedPost, Source};

/// Stored with every document; bump when parsing changes what is extracted.
pub const PARSER_VERSION: &str = "html-v2";

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

fn selector(css: &str) -> Selector {
    Selector::parse(css).expect("static CSS selector")
}

fn text(element: ElementRef<'_>) -> String {
    element
        .text()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

fn page_number(url: &Url) -> i32 {
    url.query_pairs()
        .find(|(key, _)| key == "page")
        .and_then(|(_, value)| value.parse().ok())
        .unwrap_or(1)
}

fn post_links(page_url: &Url, body: ElementRef<'_>) -> Vec<ParsedLink> {
    let mut seen = HashSet::new();
    body.select(&selector("a[href]"))
        .filter_map(|a| {
            let target = page_url
                .join(a.value().attr("href")?)
                .ok()
                .filter(|u| matches!(u.scheme(), "http" | "https"))?
                .to_string();
            let anchor = text(a);
            seen.insert(target.clone()).then(|| ParsedLink {
                target_url: target,
                anchor_text: (!anchor.is_empty()).then_some(anchor),
            })
        })
        .collect()
}

fn code_blocks(body: ElementRef<'_>) -> Vec<String> {
    body.select(&selector("pre")).map(text).collect()
}

pub fn parse(source: Source, url: &str, html: &str) -> Result<ParsedPage> {
    let page_url = super::discovery::normalize(source, url)?;
    let mut canonical = page_url.clone();
    canonical.set_query(None);
    let (url, native_id) = validate_url(source, canonical.as_str())?;
    let document = Html::parse_document(html);
    let title = match source {
        Source::Internals => document.select(&selector("h1")).next().map(text),
        Source::Rfc => document.select(&selector("title")).next().map(text),
    }
    .unwrap_or_default();
    ensure!(!title.is_empty(), "document title not found");
    let mut posts = Vec::new();
    match source {
        Source::Rfc => {
            let main = document
                .select(&selector("main"))
                .next()
                .ok_or_else(|| anyhow::anyhow!("RFC main element not found"))?;
            posts.push(ParsedPost {
                post_number: 1,
                author: None,
                published_at: None,
                text: text(main),
                code: code_blocks(main),
                links: post_links(&url, main),
            });
        }
        Source::Internals => {
            for post in document.select(&selector(".crawler-post")) {
                let Some(body) = post
                    .select(&selector(".post, [itemprop='articleBody']"))
                    .next()
                else {
                    continue;
                };
                let post_number = post
                    .value()
                    .attr("id")
                    .and_then(|id| id.strip_prefix("post_"))
                    .and_then(|number| number.parse::<i32>().ok())
                    .filter(|number| *number >= 1)
                    .ok_or_else(|| anyhow::anyhow!("post without a numeric post_N id"))?;
                posts.push(ParsedPost {
                    post_number,
                    author: post
                        .select(&selector(
                            "[itemprop='author'] [itemprop='name'], .creator [itemprop='name']",
                        ))
                        .next()
                        .map(text),
                    published_at: post
                        .select(&selector("time"))
                        .next()
                        .and_then(|t| t.value().attr("datetime"))
                        .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
                        .map(|t| t.with_timezone(&Utc)),
                    text: text(body),
                    code: code_blocks(body),
                    links: post_links(&url, body),
                });
            }
        }
    }
    let body_chars: usize = posts.iter().map(|post| post.text.chars().count()).sum();
    ensure!(
        body_chars >= 40,
        "main text missing or too short; page may be a challenge or unsupported layout"
    );
    // Completeness is judged from the collected page chain, not from one page.
    let has_next = document
        .select(&selector("a[rel='next'], link[rel='next']"))
        .next()
        .is_some();
    Ok(ParsedPage {
        native_id,
        canonical_url: url.to_string(),
        title,
        page_no: page_number(&page_url),
        page_url: page_url.to_string(),
        has_next,
        posts,
    })
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
    fn rfc_parser_extracts_main_and_preserves_code() {
        let doc = parse(
            Source::Rfc,
            "https://rust-lang.github.io/rfcs/3058-try-trait-v2.html",
            include_str!("../../tests/fixtures/rfc.html"),
        )
        .unwrap();
        assert_eq!((doc.page_no, doc.has_next, doc.posts.len()), (1, false, 1));
        assert!(!doc.posts[0].text.contains("Navigation"));
        assert!(doc.posts[0].code[0].contains('\n'));
    }

    #[test]
    fn internals_page_reports_chain_position_and_post_details() {
        let doc = parse(
            Source::Internals,
            "https://internals.rust-lang.org/t/example/42",
            include_str!("../../tests/fixtures/internals.html"),
        )
        .unwrap();
        assert_eq!((doc.page_no, doc.has_next), (1, true));
        let post = &doc.posts[0];
        assert_eq!(
            (post.post_number, post.author.as_deref()),
            (1, Some("example-author"))
        );
        assert!(post.published_at.is_some());
        assert!(!post.text.contains("Navigation"));
        let page_two = parse(
            Source::Internals,
            "https://internals.rust-lang.org/t/example/42?page=2",
            include_str!("../../tests/fixtures/internals.html"),
        )
        .unwrap();
        assert_eq!(page_two.page_no, 2);
    }

    #[test]
    fn challenge_is_not_a_document() {
        assert!(
            parse(
                Source::Rfc,
                "https://rust-lang.github.io/rfcs/3058-try-trait-v2.html",
                "<title>Just a moment</title><body>challenge</body>"
            )
            .is_err()
        );
    }
}
