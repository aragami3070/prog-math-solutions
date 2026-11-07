use super::errors::Error;
use crate::{
    models::graph::{GraphOverview, OverviewEdge, OverviewNode},
    repository::graph,
    search::display_title,
};
use sqlx::{Postgres, Transaction};
use std::collections::HashMap;

/// Most nodes the whole-graph view returns; the most-linked documents are kept.
pub const NODE_CAP: i64 = 5000;

/// Every document that has a reference edge, with the edges between them: the data for a
/// force-directed picture of the corpus. Read-only; nothing is fetched.
pub async fn execute(tx: &mut Transaction<'_, Postgres>) -> Result<GraphOverview, Error> {
    // One extra row tells whether the cap cut something off.
    let mut rows = graph::overview_nodes(tx, NODE_CAP + 1).await?;
    let truncated = rows.len() as i64 > NODE_CAP;
    rows.truncate(NODE_CAP as usize);
    let db_ids: Vec<i64> = rows.iter().map(|row| row.id).collect();
    let ids: HashMap<i64, String> = rows
        .iter()
        .map(|row| (row.id, format!("{}:{}", row.source, row.native_id)))
        .collect();
    let edges = graph::overview_edges(tx, &db_ids)
        .await?
        .into_iter()
        .filter_map(|edge| {
            Some(OverviewEdge {
                source: ids.get(&edge.source_id)?.clone(),
                target: ids.get(&edge.target_id)?.clone(),
                occurrences: edge.occurrences,
            })
        })
        .collect();
    let nodes = rows
        .into_iter()
        .map(|row| OverviewNode {
            id: format!("{}:{}", row.source, row.native_id),
            title: display_title(&row.source, &row.title),
            source: row.source,
            url: row.canonical_url,
            completeness: row.completeness,
            degree: row.degree,
        })
        .collect();
    Ok(GraphOverview {
        nodes,
        edges,
        built_at: graph::built_at(tx).await?,
        truncated,
    })
}
