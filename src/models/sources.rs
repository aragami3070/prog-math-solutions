use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Internals,
    Rfc,
}

impl Source {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Internals => "internals",
            Self::Rfc => "rfc",
        }
    }

    pub fn license_source(self) -> &'static str {
        match self {
            Self::Internals => "https://internals.rust-lang.org/tos",
            Self::Rfc => "https://rust-lang.github.io/rfcs/#license",
        }
    }

    pub fn origin(self) -> &'static str {
        match self {
            Self::Internals => "https://internals.rust-lang.org",
            Self::Rfc => "https://rust-lang.github.io",
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DocumentSummary {
    pub id: i64,
    pub source: String,
    pub native_id: String,
    pub canonical_url: String,
    pub title: String,
    pub excerpt: String,
    pub text_chars: i64,
    pub completeness: Completeness,
    pub completeness_reason: String,
    pub first_seen_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DocumentPage {
    pub items: Vec<DocumentSummary>,
    pub page: u32,
    pub page_size: u32,
    pub total: i64,
    pub total_pages: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "text", rename_all = "lowercase")]
pub enum Completeness {
    Complete,
    Partial,
    Unknown,
}

impl Completeness {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::Partial => "partial",
            Self::Unknown => "unknown",
        }
    }

    /// Judges a document by the pages collected so far, as `(page_no, has_next)`.
    /// The page chain is complete when pages 1..=N are all present and the last
    /// one links to nothing further. This is true as of fetch time only.
    pub fn evaluate(pages: &[(i32, bool)]) -> (Self, String) {
        let mut pages = pages.to_vec();
        pages.sort_unstable();
        let Some(&(last, has_next)) = pages.last() else {
            return (Self::Unknown, "No pages collected".into());
        };
        if pages[0].0 != 1 {
            return (Self::Unknown, "First page has not been collected".into());
        }
        if let Some(missing) = (1..=last).find(|n| pages.iter().all(|(page, _)| page != n)) {
            return (
                Self::Partial,
                format!("Page {missing} has not been collected"),
            );
        }
        if has_next {
            return (
                Self::Partial,
                format!("Page {last} links to a next page that has not been collected"),
            );
        }
        (
            Self::Complete,
            format!("Pages 1..{last} collected; the last page has no next link"),
        )
    }
}

pub struct ParsedLink {
    pub target_url: String,
    pub anchor_text: Option<String>,
}

pub struct ParsedPost {
    pub post_number: i32,
    pub author: Option<String>,
    pub published_at: Option<DateTime<Utc>>,
    pub text: String,
    pub code: Vec<String>,
    pub links: Vec<ParsedLink>,
}

// The parser returns data, never writes SQL or makes HTTP calls.
// One value is one fetched HTML page of a document.
pub struct ParsedPage {
    pub native_id: String,
    pub canonical_url: String,
    pub title: String,
    pub page_no: i32,
    pub page_url: String,
    pub has_next: bool,
    pub posts: Vec<ParsedPost>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completeness_follows_the_page_chain() {
        let verdict = |pages: &[(i32, bool)]| Completeness::evaluate(pages).0;
        assert_eq!(verdict(&[]), Completeness::Unknown);
        assert_eq!(verdict(&[(1, false)]), Completeness::Complete);
        assert_eq!(
            verdict(&[(2, true), (1, true), (3, false)]),
            Completeness::Complete
        );
        assert_eq!(verdict(&[(1, true)]), Completeness::Partial);
        assert_eq!(verdict(&[(1, true), (2, true)]), Completeness::Partial);
        assert_eq!(verdict(&[(1, true), (3, false)]), Completeness::Partial);
        assert_eq!(verdict(&[(2, false)]), Completeness::Unknown);
        assert!(
            Completeness::evaluate(&[(1, true), (3, false)])
                .1
                .contains("Page 2")
        );
    }
}
