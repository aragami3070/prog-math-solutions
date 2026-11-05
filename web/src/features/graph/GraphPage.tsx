import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { getRouteApi } from "@tanstack/react-router";
import { DocumentCard } from "../../components/DocumentCard";
import { ErrorPanel, Loading } from "../../components/Feedback";
import { PageHeading } from "../../components/PageHeading";
import { get } from "../../lib/client";
import type { Graph, GraphEdge, GraphNode } from "../../lib/types";

const route = getRouteApi("/graph");

/** Neighbours as lists: who the document links to and who links to it, with the linking URLs. */
export function GraphPage() {
  const { document } = route.useSearch();
  const navigate = route.useNavigate();
  const [draft, setDraft] = useState(document);
  const query = useQuery({
    queryKey: ["graph", document],
    queryFn: ({ signal }) => get<Graph>(`/api/graph?document=${encodeURIComponent(document)}`, signal),
    enabled: document !== "",
    retry: false,
  });
  const data = query.data;
  const edgeOf = (node: GraphNode): GraphEdge | undefined =>
    data?.edges.find(edge => edge.source === node.id || edge.target === node.id);
  const section = (title: string, nodes: GraphNode[]) => nodes.length > 0 && (
    <section aria-label={title}>
      <h2>{title} ({nodes.length})</h2>
      <div className="documents">
        {nodes.map(node => {
          const edge = edgeOf(node);
          return (
            <DocumentCard key={node.id} source={node.source} label={node.id} title={node.title} url={node.url} completeness={node.completeness}
              footer={edge && <a href={`/graph?document=${encodeURIComponent(node.id)}`}>связи этого документа · ссылок: {edge.occurrences}</a>}>
              {edge?.evidence[0]?.anchor_text && <>«{edge.evidence[0].anchor_text}»</>}
            </DocumentCard>
          );
        })}
      </div>
    </section>
  );

  return (
    <>
      <PageHeading
        eyebrow="СВЯЗИ ДОКУМЕНТОВ"
        title="Связи"
        lead="Ссылка из одного документа на другой, найденная в тексте постов. Ссылка — это только ссылка: она не говорит о поддержке или влиянии."
        onRefresh={() => void query.refetch()}
        refreshing={query.isFetching}
      />
      <form className="search-form" onSubmit={event => { event.preventDefault(); void navigate({ search: { document: draft.trim() } }); }}>
        <input type="text" value={draft} maxLength={40} placeholder="rfc:3058 или internals:16678" aria-label="Документ" onChange={event => setDraft(event.target.value)} />
        <button type="submit" disabled={draft.trim() === ""}>Показать</button>
      </form>
      {document === "" ? <div className="notice">Введите ID документа, например rfc:3058.</div>
        : query.isPending ? <Loading />
        : query.isError ? <ErrorPanel error={query.error} retry={() => void query.refetch()} />
        : data && (
          <>
            <DocumentCard source={data.center.source} label={data.center.id} title={data.center.title} url={data.center.url} completeness={data.center.completeness} />
            <div className="toolbar">
              <span><strong>{data.nodes.length}</strong> соседей · связей: {data.total_edges}</span>
              <span>{data.built_at ? `Граф построен ${new Date(data.built_at).toLocaleString("ru-RU")}` : "Граф ещё не построен"}</span>
            </div>
            {data.truncated && <p className="notice" role="status">Показаны самые связанные {data.limits.nodes} соседей, остальные не поместились.</p>}
            {data.nodes.length === 0 && <div className="empty"><h2>Связей нет</h2><p>Ссылок на другие документы корпуса не найдено{data.built_at ? "." : ": граф ещё не построен (POST /api/graph/rebuild)."}</p></div>}
            {section("Ссылается на", data.nodes.filter(node => node.role === "references" || node.role === "both"))}
            {section("Ссылаются на него", data.nodes.filter(node => node.role === "referenced_by" || node.role === "both"))}
          </>
        )}
    </>
  );
}
