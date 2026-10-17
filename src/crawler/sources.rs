use anyhow::{Result, bail, ensure};
use scraper::{ElementRef, Html, Selector};
use serde_json::json;
use url::Url;

use crate::models::sources::{ParsedDocument, Source};

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

pub fn parse(source: Source, url: &str, html: &str) -> Result<ParsedDocument> {
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
    let mut sections = Vec::new();
    let (completeness, reason) = match source {
        Source::Rfc => {
            let main = document
                .select(&selector("main"))
                .next()
                .ok_or_else(|| anyhow::anyhow!("RFC main element not found"))?;
            sections.push(json!({"text": text(main), "code": main.select(&selector("pre")).map(text).collect::<Vec<_>>() }));
            (
                "complete",
                "Single RFC HTML document; main element extracted",
            )
        }
        Source::Internals => {
            for post in document.select(&selector(".crawler-post")) {
                let Some(body) = post
                    .select(&selector(".post, [itemprop='articleBody']"))
                    .next()
                else {
                    continue;
                };
                sections.push(json!({
                    "post_id": post.value().attr("id"),
                    "author": post.select(&selector("[itemprop='author'] [itemprop='name'], .creator [itemprop='name']")).next().map(text),
                    "published_at": post.select(&selector("time")).next().and_then(|t| t.value().attr("datetime")),
                    "text": text(body),
                    "code": body.select(&selector("pre")).map(text).collect::<Vec<_>>()
                }));
            }
            // The first HTML response is not proof that all Discourse posts were fetched.
            let has_next = document
                .select(&selector("a[rel='next'], link[rel='next']"))
                .next()
                .is_some();
            if has_next {
                (
                    "partial",
                    "HTML advertises a next page; collected pages do not prove full topic coverage",
                )
            } else {
                (
                    "unknown",
                    "Full Discourse post count has not been verified; not counted as a complete topic",
                )
            }
        }
    };
    let body_text = sections
        .iter()
        .filter_map(|s| s["text"].as_str())
        .collect::<Vec<_>>()
        .join("\n\n");
    ensure!(
        body_text.chars().count() >= 40,
        "main text missing or too short; page may be a challenge or unsupported layout"
    );
    let links_selector = selector(match source {
        Source::Rfc => "main a[href]",
        Source::Internals => {
            ".crawler-post .post a[href], .crawler-post [itemprop='articleBody'] a[href]"
        }
    });
    let links: Vec<_> = document
        .select(&links_selector)
        .filter_map(|a| {
            url.join(a.value().attr("href")?)
                .ok()
                .filter(|u| matches!(u.scheme(), "http" | "https"))
                .map(|u| u.to_string())
        })
        .collect();
    Ok(ParsedDocument {
        native_id,
        canonical_url: url.to_string(),
        title,
        body_text,
        completeness,
        completeness_reason: reason.into(),
        metadata: json!({"page_url": page_url.as_str(), "sections": sections, "links": links, "license_source": match source {
            Source::Internals => "https://internals.rust-lang.org/tos",
            Source::Rfc => "https://rust-lang.github.io/rfcs/#license"
        }}),
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
        assert_eq!(doc.completeness, "complete");
        assert!(!doc.body_text.contains("Navigation"));
        assert!(
            doc.metadata["sections"][0]["code"][0]
                .as_str()
                .unwrap()
                .contains("\n")
        );
    }

    #[test]
    fn internals_does_not_claim_completeness() {
        let doc = parse(
            Source::Internals,
            "https://internals.rust-lang.org/t/example/42",
            include_str!("../../tests/fixtures/internals.html"),
        )
        .unwrap();
        assert_eq!(doc.completeness, "partial");
        assert_eq!(doc.metadata["sections"][0]["author"], "example-author");
        assert!(!doc.body_text.contains("Navigation"));
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
