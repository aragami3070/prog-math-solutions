use super::errors::Error;
use crate::{
    models::{pagination::Pagination, sources::DocumentPage},
    repository::sources,
};
use sqlx::{Postgres, Transaction};

pub fn validate(pagination: &Pagination) -> Result<(), Error> {
    if !pagination.valid() {
        return Err(Error::InvalidPagination);
    }
    Ok(())
}

pub async fn execute(
    tx: &mut Transaction<'_, Postgres>,
    pagination: Pagination,
) -> Result<DocumentPage, Error> {
    validate(&pagination)?;
    let total = sources::count_documents(tx).await?;
    let items = sources::documents(tx, &pagination).await?;
    let size = i64::from(pagination.page_size);
    Ok(DocumentPage {
        items,
        total,
        page: pagination.page,
        page_size: pagination.page_size,
        total_pages: (total + size - 1) / size,
    })
}
