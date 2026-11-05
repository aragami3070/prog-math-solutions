use super::errors::Error;
use crate::{models::events::ClickEvent, repository};
use sqlx::{Postgres, Transaction};

fn is_uuid(text: &str) -> bool {
    text.len() == 36
        && text.bytes().enumerate().all(|(i, b)| match i {
            8 | 13 | 18 | 23 => b == b'-',
            _ => b.is_ascii_hexdigit(),
        })
}

fn is_document(text: &str) -> bool {
    text.split_once(':').is_some_and(|(source, id)| {
        matches!(source, "rfc" | "internals")
            && (1..=12).contains(&id.len())
            && id.bytes().all(|b| b.is_ascii_digit())
    })
}

pub async fn execute(tx: &mut Transaction<'_, Postgres>, click: ClickEvent) -> Result<(), Error> {
    if !is_uuid(&click.query_id) {
        return Err(Error::InvalidQueryId);
    }
    if !is_document(&click.document_id) {
        return Err(Error::InvalidDocument);
    }
    if !(1..=1000).contains(&click.rank) {
        return Err(Error::InvalidRank);
    }
    let written = repository::events::insert_click(
        tx,
        &click.query_id,
        &click.document_id,
        click.rank as i32,
    )
    .await?;
    if written == 0 {
        return Err(Error::UnknownSearch);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_checked_before_the_database_sees_them() {
        assert!(is_uuid("123e4567-e89b-12d3-a456-426614174000"));
        assert!(!is_uuid("123e4567e89b12d3a456426614174000"));
        assert!(!is_uuid("zzze4567-e89b-12d3-a456-426614174000"));
        assert!(is_document("rfc:3058"));
        assert!(is_document("internals:16678"));
        assert!(!is_document("rfc:"));
        assert!(!is_document("other:1"));
        assert!(!is_document("rfc:1; drop"));
    }
}
