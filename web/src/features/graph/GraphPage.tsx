import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { getRouteApi } from "@tanstack/react-router";
import { DocumentCard } from "../../components/DocumentCard";
import { ErrorPanel, Loading } from "../../components/Feedback";
import { PageHeading } from "../../components/PageHeading";
import { get } from "../../lib/client";
import type { Graph, GraphEdge, GraphNode, GraphOverview, OverviewNode } from "../../lib/types";
import { sourceName } from "../../lib/sources";
import { GraphCanvas, NODE_COLORS } from "./GraphCanvas";

const route = getRouteApi("/graph");

const SHOWN_NEIGHBOURS = 30;

/**
 * The whole reference graph as a picture, plus the selected document's neighbours as lists with
 * the linking URLs. The selected document lives in the URL (`?document=`), so a link to a view
 * opens on the same node.
 */
export function GraphPage() {
  const { document } = route.useSearch();
  const navigate = route.useNavigate();
  const [draft, setDraft] = useState("");
  const [focus, setFocus] = useState<{ id: string } | null>(null);
  const overview = useQuery({
    queryKey: ["graph-overview"],
    queryFn: ({ signal }) => get<GraphOverview>("/api/graph/overview", signal),
    staleTime: 5 * 60_000,
  });
  const select = (id: string | null, centre = false) => {
    void navigate({ search: { document: id ?? "" }, replace: true });
    if (id && centre) setFocus({ id });
  };
  const nodes = overview.data?.nodes;
  const byId = useMemo(() => new Map((nodes ?? []).map(node => [node.id, node])), [nodes]);
  const adjacency = useMemo(() => {
    const out = new Map<string, Set<string>>();
    const into = new Map<string, Set<string>>();
    const add = (map: Map<string, Set<string>>, key: string, value: string) => map.set(key, (map.get(key) ?? new Set()).add(value));
    for (const edge of overview.data?.edges ?? []) {
      add(out, edge.source, edge.target);
      add(into, edge.target, edge.source);
    }
    return { out, into };
  }, [overview.data]);
  const needle = draft.trim().toLowerCase();
  const matches = useMemo(() => needle === "" ? [] : (nodes ?? [])
    .filter(node => node.title.toLowerCase().includes(needle) || node.id.includes(needle))
    .sort((a, b) => b.degree - a.degree)
    .slice(0, 8), [nodes, needle]);
  const selected = byId.get(document);
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
      <form className="search-form" onSubmit={event => {
        event.preventDefault();
        const exact = /^(rfc|internals):\d{1,9}$/.test(needle) ? needle : matches[0]?.id;
        if (exact) { select(exact, true); setDraft(""); }
      }}>
        <input type="search" value={draft} maxLength={80} placeholder="Найти документ в графе: заголовок или rfc:3058" aria-label="Найти документ" onChange={event => setDraft(event.target.value)} />
        <button type="submit" disabled={needle === ""}>Показать</button>
      </form>
      {matches.length > 0 && (
        <ul className="graph-matches" aria-label="Найденные документы">
          {matches.map(node => (
            <li key={node.id}><button type="button" onClick={() => { select(node.id, true); setDraft(""); }}>
              <span className={`source source-${node.source}`}>{sourceName(node.source)}</span> {node.title} <small>· связей: {node.degree}</small>
            </button></li>
          ))}
        </ul>
      )}
      {overview.isPending ? <Loading />
        : overview.isError ? <ErrorPanel error={overview.error} retry={() => void overview.refetch()} />
        : overview.data.nodes.length === 0 ? <div className="empty"><h2>Граф пуст</h2><p>Рёбер нет: граф ещё не построен (POST /api/graph/rebuild).</p></div>
        : (
          <>
            <div className="toolbar">
              <span><strong>{overview.data.nodes.length}</strong> документов · <strong>{overview.data.edges.length}</strong> ссылок{overview.data.truncated ? " (показаны самые связанные)" : ""}</span>
              <span className="graph-legend">
                {Object.entries(NODE_COLORS).map(([source, color]) => <span key={source}><i style={{ background: color }} />{sourceName(source as OverviewNode["source"])}</span>)}
                <span>размер — число связей</span>
              </span>
              <span>{overview.data.built_at ? `Построен ${new Date(overview.data.built_at).toLocaleString("ru-RU")}` : ""}</span>
            </div>
            <div className="graph-layout">
              <GraphCanvas nodes={overview.data.nodes} edges={overview.data.edges} selectedId={document} focus={focus} onSelect={id => select(id)} />
              <aside className="graph-panel" aria-label="Выбранный документ">
                {selected ? (
                  <>
                    <div className="document-meta"><span className={`source source-${selected.source}`}>{sourceName(selected.source)}</span><span>{selected.id}</span></div>
                    <h2>{selected.title}</h2>
                    <p><a href={selected.url} target="_blank" rel="noopener noreferrer">Открыть оригинал ↗</a></p>
                    {([["Ссылается на", adjacency.out.get(selected.id)], ["Ссылаются на него", adjacency.into.get(selected.id)]] as const).map(([title, ids]) => ids && (
                      <section key={title}>
                        <h3>{title} ({ids.size})</h3>
                        <ul>
                          {[...ids].map(id => byId.get(id)).filter((node): node is OverviewNode => node !== undefined)
                            .sort((a, b) => b.degree - a.degree).slice(0, SHOWN_NEIGHBOURS)
                            .map(node => <li key={node.id}><button type="button" onClick={() => select(node.id, true)}>{node.title}</button></li>)}
                        </ul>
                        {ids.size > SHOWN_NEIGHBOURS && <small>и ещё {ids.size - SHOWN_NEIGHBOURS}</small>}
                      </section>
                    ))}
                  </>
                ) : document !== "" ? (
                  <p>{document}: в графе нет рёбер у этого документа, ниже его карточка.</p>
                ) : (
                  <p>Нажмите на узел, чтобы увидеть связи. Двойной клик открывает оригинал, колесо — масштаб, перетаскивание фона — сдвиг.</p>
                )}
              </aside>
            </div>
          </>
        )}
      {document === "" ? null
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
