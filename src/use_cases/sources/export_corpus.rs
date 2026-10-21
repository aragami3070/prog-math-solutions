use super::errors::Error;
use crate::{
    models::{
        exports::{
            ExportArtifact, ExportDocument, ExportFormat, ExportLink, ExportPage, ExportPost,
            ExportQuery, ExportRow,
        },
        sources::Source,
    },
    repository::sources,
};
use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Transaction};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    io::{Seek, SeekFrom, Write},
};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

const CSV_COLUMNS: &[&str] = &[
    "schema_version",
    "document_id",
    "source",
    "native_id",
    "canonical_url",
    "title",
    "language",
    "published_at",
    "updated_at",
    "fetched_at",
    "first_seen_at",
    "last_seen_at",
    "authors",
    "source_category",
    "source_tags",
    "sections",
    "body_text",
    "links",
    "content_hash",
    "completeness",
    "completeness_reason",
    "fetch_ids",
    "parser_version",
    "license_provenance",
    "metadata",
];

/// The caller owns the repeatable-read transaction. No requests to source websites.
/// Temporary files are unlinked on creation and removed when the response is dropped.
pub async fn execute(
    tx: &mut Transaction<'_, Postgres>,
    query: &ExportQuery,
) -> Result<ExportArtifact, Error> {
    let snapshot_at = sources::export_snapshot_time(tx).await?;
    let formats = if query.format == ExportFormat::Zip {
        vec![ExportFormat::Jsonl, ExportFormat::Json, ExportFormat::Csv]
    } else {
        vec![query.format]
    };
    let mut outputs = Vec::new();
    for format in formats {
        let mut file = tokio::fs::File::from_std(tempfile::tempfile()?);
        match format {
            ExportFormat::Json => file.write_all(b"[").await?,
            ExportFormat::Csv => file.write_all(&csv_header()?).await?,
            _ => {}
        }
        outputs.push((format, file));
    }
    let mut after = String::new();
    let mut count = 0u64;
    let mut corpus_hash = Sha256::new();
    let mut source_counts = BTreeMap::<String, u64>::new();
    let mut completeness_counts = BTreeMap::<String, u64>::new();
    let mut parser_versions = BTreeSet::new();
    let mut referenced_fetches = BTreeSet::new();
    loop {
        let batch = sources::export_batch(tx, query.source, query.complete_only, &after).await?;
        if batch.is_empty() {
            break;
        }
        let ids: Vec<i64> = batch.iter().map(|row| row.id).collect();
        let mut parts = Batch::load(tx, &ids).await?;
        for row in batch {
            let id = row.id;
            let doc = canonical(row, parts.take(id));
            after = doc.document_id.clone();
            let bytes = serde_json::to_vec(&doc)?;
            corpus_hash.update(&bytes);
            corpus_hash.update(b"\n");
            for (format, file) in &mut outputs {
                match format {
                    ExportFormat::Json => {
                        if count > 0 {
                            file.write_all(b",").await?;
                        }
                        file.write_all(&bytes).await?;
                    }
                    ExportFormat::Jsonl => {
                        file.write_all(&bytes).await?;
                        file.write_all(b"\n").await?;
                    }
                    ExportFormat::Csv => {
                        file.write_all(&csv_record(&doc, query.spreadsheet_safe)?)
                            .await?
                    }
                    ExportFormat::Zip => unreachable!("ZIP is built after serializing documents"),
                }
            }
            count += 1;
            *source_counts.entry(doc.source).or_default() += 1;
            *completeness_counts.entry(doc.completeness).or_default() += 1;
            parser_versions.insert(doc.parser_version);
            referenced_fetches.extend(doc.fetch_ids);
        }
    }
    let mut files = Vec::new();
    let mut checksums = BTreeMap::new();
    for (format, mut file) in outputs {
        if format == ExportFormat::Json {
            file.write_all(b"]\n").await?;
        }
        file.flush().await?;
        let (size, sha256) = checksum(&mut file).await?;
        let name = format!("corpus.{}", format.extension());
        checksums.insert(name.clone(), json!({"bytes": size, "sha256": sha256}));
        if query.format != ExportFormat::Zip {
            return Ok(ExportArtifact {
                file,
                size,
                sha256,
                count,
                snapshot_at,
            });
        }
        files.push((name, file.into_std().await));
    }
    let manifest = serde_json::to_vec_pretty(&json!({
        "schema_version": 1,
        "snapshot_at": snapshot_at,
        "corpus_sha256": format!("{:x}", corpus_hash.finalize()),
        "corpus_hash_algorithm": "sha256 of corpus.jsonl bytes, document_id sorted with C collation",
        "filters": {"source": query.source, "complete_only": query.complete_only},
        "spreadsheet_safe": query.spreadsheet_safe,
        "documents": count,
        "documents_by_source": source_counts,
        "documents_by_completeness": completeness_counts,
        "referenced_fetches": referenced_fetches.len(),
        "parser_versions": parser_versions,
        "files": checksums,
        "raw_html_included": false,
        "limitations": [
            "Current normalized versions only, not historical snapshots of each document",
            "Unknown language/category/dates/license identifiers are not inferred",
            "Partial and unknown topics do not prove full topic coverage",
            "Missing section link attribution and anchor text remain null"
        ]
    }))?;
    let file = tokio::task::spawn_blocking(move || -> Result<std::fs::File, Error> {
        let mut zip = zip::ZipWriter::new(tempfile::tempfile()?);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        for (name, mut file) in files {
            zip.start_file(name, options)?;
            std::io::copy(&mut file, &mut zip)?;
        }
        zip.start_file("manifest.json", options)?;
        zip.write_all(&manifest)?;
        let mut file = zip.finish()?;
        file.seek(SeekFrom::Start(0))?;
        Ok(file)
    })
    .await??;
    let mut file = tokio::fs::File::from_std(file);
    let (size, sha256) = checksum(&mut file).await?;
    Ok(ExportArtifact {
        file,
        size,
        sha256,
        count,
        snapshot_at,
    })
}

async fn checksum(file: &mut tokio::fs::File) -> Result<(u64, String), Error> {
    file.seek(SeekFrom::Start(0)).await?;
    let mut hash = Sha256::new();
    let mut buffer = vec![0u8; 64 * 1024];
    let mut size = 0;
    loop {
        let read = file.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        size += read as u64;
        hash.update(&buffer[..read]);
    }
    file.seek(SeekFrom::Start(0)).await?;
    Ok((size, format!("{:x}", hash.finalize())))
}

/// Posts, links and pages of a batch of documents, loaded with one query each.
#[derive(Default)]
struct DocumentParts {
    posts: Vec<ExportPost>,
    links: Vec<ExportLink>,
    pages: Vec<ExportPage>,
}

struct Batch {
    by_document: HashMap<i64, DocumentParts>,
}

impl Batch {
    async fn load(tx: &mut Transaction<'_, Postgres>, ids: &[i64]) -> Result<Self, Error> {
        let mut by_document = HashMap::<i64, DocumentParts>::new();
        for post in sources::export_posts(tx, ids).await? {
            by_document
                .entry(post.document_id)
                .or_default()
                .posts
                .push(post);
        }
        for link in sources::export_links(tx, ids).await? {
            by_document
                .entry(link.document_id)
                .or_default()
                .links
                .push(link);
        }
        for page in sources::export_pages(tx, ids).await? {
            by_document
                .entry(page.document_id)
                .or_default()
                .pages
                .push(page);
        }
        Ok(Self { by_document })
    }

    fn take(&mut self, id: i64) -> DocumentParts {
        self.by_document.remove(&id).unwrap_or_default()
    }
}

fn timestamp(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn canonical(row: ExportRow, parts: DocumentParts) -> ExportDocument {
    let document_id = format!("{}:{}", row.source, row.native_id);
    let internals = row.source == "internals";
    let license_source = match row.source.as_str() {
        "internals" => Source::Internals.license_source(),
        _ => Source::Rfc.license_source(),
    };
    let license = json!({"source_url": license_source, "license_id": null, "status": "unresolved"});
    // Section keys keep the v1 shape: Discourse "post_N", a single RFC "section_1".
    let key = |post_number: i32| {
        if internals {
            format!("post_{post_number}")
        } else {
            format!("section_{post_number}")
        }
    };
    let mut authors = BTreeSet::new();
    let mut published_at = None;
    let mut sections = Vec::new();
    for post in parts.posts {
        if let Some(author) = &post.author {
            authors.insert(author.clone());
        }
        // Only the first post establishes a topic's publication time.
        if post.post_number == 1 {
            published_at = post.published_at;
        }
        sections.push(json!({
            "section_id": format!("{document_id}:{}", key(post.post_number)),
            "post_id": internals.then(|| key(post.post_number)),
            "author": post.author,
            "published_at": post.published_at.map(timestamp),
            "license": license,
            "heading": null,
            "text": post.text,
            "code_blocks": post.code,
            "source_url": row.canonical_url,
        }));
    }
    let links = parts
        .links
        .into_iter()
        .map(|link| {
            json!({
                "target_url": link.target_url,
                "section_id": format!("{document_id}:{}", key(link.post_number)),
                "anchor_text": link.anchor_text,
            })
        })
        .collect();
    let fetch_ids: BTreeSet<i64> = parts.pages.iter().map(|page| page.fetch_id).collect();
    let pages: Vec<Value> = parts
        .pages
        .iter()
        .map(|page| json!({"page_no": page.page_no, "url": page.url, "fetch_id": page.fetch_id}))
        .collect();
    ExportDocument {
        schema_version: 1,
        document_id,
        source: row.source,
        native_id: row.native_id,
        canonical_url: row.canonical_url,
        title: row.title,
        language: None,
        published_at,
        updated_at: None,
        fetched_at: row.fetched_at,
        first_seen_at: row.first_seen_at,
        last_seen_at: row.last_seen_at,
        authors: authors.into_iter().collect(),
        source_category: None,
        source_tags: Vec::new(),
        sections,
        body_text: row.body_text,
        links,
        content_hash: row.content_hash,
        completeness: row.completeness,
        completeness_reason: row.completeness_reason,
        fetch_ids: fetch_ids.into_iter().collect(),
        parser_version: row.parser_version,
        license_provenance: license,
        metadata: json!({"license_source": license_source, "pages": pages}),
    }
}

fn csv_header() -> Result<Vec<u8>, Error> {
    let mut writer = csv::Writer::from_writer(Vec::new());
    writer.write_record(CSV_COLUMNS)?;
    writer.flush()?;
    writer
        .into_inner()
        .map_err(|e| Error::Export(e.into_error()))
}

fn csv_record(doc: &ExportDocument, safe: bool) -> Result<Vec<u8>, Error> {
    let value = serde_json::to_value(doc)?;
    let cells = CSV_COLUMNS.iter().map(|key| match &value[*key] {
        Value::Null => String::new(),
        Value::String(s) => {
            if safe
                && (s.starts_with(['\t', '\r', '\n'])
                    || s.trim_start().starts_with(['=', '+', '-', '@']))
            {
                format!("'{s}")
            } else {
                s.clone()
            }
        }
        other => other.to_string(),
    });
    let mut writer = csv::Writer::from_writer(Vec::new());
    writer.write_record(cells)?;
    writer.flush()?;
    writer
        .into_inner()
        .map_err(|e| Error::Export(e.into_error()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn post(number: i32, text: &str) -> ExportPost {
        ExportPost {
            document_id: 1,
            post_number: number,
            author: Some("author".into()),
            published_at: Some(Utc::now()),
            text: text.into(),
            code: json!(["a\nb"]),
        }
    }

    #[test]
    fn canonical_and_csv_preserve_text_and_unknown_provenance() {
        let now = Utc::now();
        let doc = canonical(
            ExportRow {
                id: 1,
                source: "rfc".into(),
                native_id: "1".into(),
                canonical_url: "https://rust-lang.github.io/rfcs/0001-test.html".into(),
                title: "=SUM(1,2)".into(),
                body_text: "Привет, \"Rust\"\r\nsecond line\n第三行".into(),
                content_hash: "hash".into(),
                completeness: "unknown".into(),
                completeness_reason: "fixture".into(),
                parser_version: "fixture".into(),
                fetched_at: now,
                first_seen_at: now,
                last_seen_at: now,
            },
            DocumentParts {
                posts: vec![post(1, "hello")],
                links: vec![ExportLink {
                    document_id: 1,
                    post_number: 1,
                    target_url: "https://example.org".into(),
                    anchor_text: Some("example".into()),
                }],
                pages: vec![
                    ExportPage {
                        document_id: 1,
                        page_no: 1,
                        url: "https://rust-lang.github.io/rfcs/0001-test.html".into(),
                        fetch_id: 8,
                    },
                    ExportPage {
                        document_id: 1,
                        page_no: 2,
                        url: "https://rust-lang.github.io/rfcs/0001-test.html?page=2".into(),
                        fetch_id: 7,
                    },
                ],
            },
        );
        assert_eq!(doc.document_id, "rfc:1");
        assert_eq!(doc.fetch_ids, vec![7, 8]);
        assert_eq!(doc.sections[0]["section_id"], "rfc:1:section_1");
        assert_eq!(doc.sections[0]["post_id"], Value::Null);
        assert_eq!(doc.links[0]["section_id"], "rfc:1:section_1");
        assert_eq!(doc.links[0]["anchor_text"], "example");
        assert!(doc.published_at.is_some());
        assert!(doc.language.is_none());
        let serialized = serde_json::to_vec(&doc).unwrap();
        assert_eq!(
            serde_json::from_slice::<ExportDocument>(&serialized).unwrap(),
            doc
        );
        for safe in [false, true] {
            let bytes = [csv_header().unwrap(), csv_record(&doc, safe).unwrap()].concat();
            let mut reader = csv::Reader::from_reader(bytes.as_slice());
            let headers = reader.headers().unwrap().clone();
            let row = reader.records().next().unwrap().unwrap();
            assert_eq!(
                &row[headers.iter().position(|v| v == "body_text").unwrap()],
                doc.body_text
            );
            assert_eq!(&row[5], if safe { "'=SUM(1,2)" } else { "=SUM(1,2)" });
        }
    }
}
