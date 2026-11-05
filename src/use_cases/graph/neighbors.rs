use super::errors::Error;
use crate::{
    models::graph::{
        Evidence, Graph, GraphEdge, GraphLimits, GraphNode, GraphQuery, MAX_LIMIT, NodeRow,
    },
    repository::graph,
    search::display_title,
};
use sqlx::{Postgres, Transaction};
use std::collections::{HashMap, HashSet};

fn node(row: &NodeRow, role: &'static str) -> GraphNode {
    GraphNode {
        id: format!("{}:{}", row.source, row.native_id),
        db_id: row.id,
        source: row.source.clone(),
        title: display_title(&row.source, &row.title),
        url: row.canonical_url.clone(),
        completeness: row.completeness.clone(),
        role,
    }
}

fn valid_document(text: &str) -> bool {
    text.split_once(':').is_some_and(|(source, id)| {
        matches!(source, "rfc" | "internals")
            && !id.is_empty()
            && id.len() <= 9
            && id.bytes().all(|b| b.is_ascii_digit())
    })
}

/// The one-hop neighbourhood of a document: who it links to and who links to it. Links already
/// resolved to corpus documents; nothing is fetched. Beyond the node limit the most-linked
/// neighbours stay and `truncated` says the rest exist.
pub async fn execute(
    tx: &mut Transaction<'_, Postgres>,
    query: GraphQuery,
) -> Result<Graph, Error> {
    if !valid_document(&query.document) {
        return Err(Error::InvalidDocument);
    }
    if !(1..=MAX_LIMIT).contains(&query.limit) {
        return Err(Error::InvalidLimit);
    }
    let center = graph::node(tx, &query.document)
        .await?
        .ok_or(Error::NotFound)?;
    let all = graph::edges_of(tx, center.id).await?;
    let total_edges = all.len() as i64;

    // Edges come most-linked first: keep them while the neighbour budget lasts.
    let mut kept = Vec::new();
    let mut neighbours: Vec<i64> = Vec::new();
    let mut seen = HashSet::new();
    for edge in all {
        let other = if edge.source_id == center.id {
            edge.target_id
        } else {
            edge.source_id
        };
        if seen.insert(other) {
            if neighbours.len() == query.limit as usize {
                continue;
            }
            neighbours.push(other);
        }
        kept.push(edge);
    }
    let truncated = (kept.len() as i64) < total_edges;

    let mut roles: HashMap<i64, (bool, bool)> = HashMap::new(); // (center links to it, it links to center)
    for edge in &kept {
        if edge.source_id == center.id {
            roles.entry(edge.target_id).or_default().0 = true;
        } else {
            roles.entry(edge.source_id).or_default().1 = true;
        }
    }
    let rows: HashMap<i64, NodeRow> = graph::nodes(tx, &neighbours)
        .await?
        .into_iter()
        .map(|row| (row.id, row))
        .collect();
    let id_of = |db_id: i64| -> Option<String> {
        if db_id == center.id {
            Some(format!("{}:{}", center.source, center.native_id))
        } else {
            rows.get(&db_id)
                .map(|row| format!("{}:{}", row.source, row.native_id))
        }
    };
    let nodes: Vec<GraphNode> = neighbours
        .iter()
        .filter_map(|id| rows.get(id).map(|row| (row, roles[id])))
        .map(|(row, (out, incoming))| {
            node(
                row,
                match (out, incoming) {
                    (true, true) => "both",
                    (true, false) => "references",
                    _ => "referenced_by",
                },
            )
        })
        .collect();
    let edges = kept
        .into_iter()
        .filter_map(|edge| {
            Some(GraphEdge {
                source: id_of(edge.source_id)?,
                target: id_of(edge.target_id)?,
                kind: "references",
                occurrences: edge.occurrences,
                evidence: serde_json::from_value::<Vec<Evidence>>(edge.evidence)
                    .unwrap_or_default(),
            })
        })
        .collect();
    Ok(Graph {
        center: node(&center, "center"),
        nodes,
        edges,
        edge_types: vec!["references"],
        built_at: graph::built_at(tx).await?,
        truncated,
        total_edges,
        limits: GraphLimits { nodes: query.limit },
    })
}

#[cfg(test)]
mod tests {
    use super::valid_document;

    #[test]
    fn only_document_ids_are_accepted() {
        assert!(valid_document("rfc:3058"));
        assert!(valid_document("internals:16678"));
        assert!(!valid_document("rfc:"));
        assert!(!valid_document("rfc:30 58"));
        assert!(!valid_document("other:1"));
        assert!(!valid_document("3058"));
        assert!(!valid_document("rfc:1' OR 1=1"));
    }
}
