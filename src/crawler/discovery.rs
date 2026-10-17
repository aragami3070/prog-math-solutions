//! Source-specific crawl scope. HTML links are candidates, never permission to fetch.
use crate::models::sources::Source;
use anyhow::{Result, bail, ensure};
use scraper::{Html, Selector};
use std::collections::HashSet;
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
                super::sources::validate_url(source, url.as_str())?;
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

pub fn is_document(source: Source, url: &Url) -> bool {
    match source {
        Source::Internals => url.path().starts_with("/t/"),
        Source::Rfc => super::sources::validate_url(source, url.as_str()).is_ok(),
    }
}

pub fn links(source: Source, base: &Url, html: &str) -> Vec<Url> {
    let doc = Html::parse_document(html);
    let selector = Selector::parse("a[href], link[rel~='next'][href], iframe[src]").unwrap();
    let mut seen = HashSet::new();
    doc.select(&selector)
        .filter_map(|node| {
            let href = node
                .value()
                .attr("href")
                .or_else(|| node.value().attr("src"))?;
            let url = normalize(source, base.join(href).ok()?.as_str()).ok()?;
            seen.insert(url.to_string()).then_some(url)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
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
        assert_eq!(
            urls.iter().map(Url::as_str).collect::<Vec<_>>(),
            vec![
                "https://internals.rust-lang.org/t/42",
                "https://internals.rust-lang.org/t/42?page=2",
                "https://internals.rust-lang.org/latest?page=1",
            ]
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
        assert_eq!(urls[0].path(), "/rfcs/toc.html");
        assert!(
            normalize(
                Source::Internals,
                "https://internals.rust-lang.org.evil/t/1"
            )
            .is_err()
        );
    }
}
