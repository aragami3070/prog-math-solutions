import React from "react";
import ReactDOM from "react-dom/client";
import { QueryClient, QueryClientProvider, useQuery } from "@tanstack/react-query";
import { createRootRoute, createRoute, createRouter, Link, Outlet, RouterProvider } from "@tanstack/react-router";
import { date, get, number, pagination, sourceName, type DocumentPage, type Statistics } from "./api";
import "./style.css";
import { CrawlControls } from "./CrawlControls";
import { ExportControls } from "./ExportControls";

const queryClient = new QueryClient({ defaultOptions: { queries: { retry: 1, staleTime: 5_000 } } });

function Layout() {
  return <div className="shell">
    <aside className="sidebar">
      <Link to="/" search={{ page: 1, page_size: 20 }} className="brand"><span className="mark">rk<span>.</span></span><span>Rust<br />Knowledge</span></Link>
      <p className="sidebar-label">РАБОЧАЯ ОБЛАСТЬ</p>
      <nav aria-label="Основная навигация">
        <Link to="/" search={{ page: 1, page_size: 20 }} activeOptions={{ exact: true }} activeProps={{ className: "active" }}><span aria-hidden>▤</span> Собранные страницы</Link>
        <Link to="/stats" activeProps={{ className: "active" }}><span aria-hidden>▥</span> Статистика</Link>
      </nav>
      <div className="sidebar-bottom"><span className="dot" /> Локальная коллекция<p>Internals & RFC<br />Без поиска и AI — пока только корпус.</p><a href="/swagger-ui/">Документация API ↗</a></div>
    </aside>
    <main id="main-content"><div className="topline"><span>КОРПУС / RUST</span><span className="phase">Этап 01 · Сбор данных</span></div><Outlet /></main>
  </div>;
}

function ErrorPanel({ error, retry }: { error: Error; retry: () => void }) {
  return <div className="notice error" role="alert"><strong>Не удалось получить данные</strong><p>{error.message}</p><button onClick={retry}>Повторить</button></div>;
}
function Loading() { return <div className="notice" role="status">Загружаем данные корпуса…</div>; }
function Badge({ value }: { value: string }) {
  const labels: Record<string, string> = { complete: "Полный документ", partial: "Часть темы (не все страницы)", unknown: "Полнота неизвестна", running: "В процессе", completed: "Завершён", failed: "С ошибками", blocked: "Источник остановлен", interrupted: "Прерван" };
  return <span className={`badge badge-${value}`}>{labels[value] ?? value}</span>;
}

function Documents() {
  const search = documentsRoute.useSearch();
  const navigate = documentsRoute.useNavigate();
  const query = useQuery({ queryKey: ["documents", search], queryFn: ({ signal }) => get<DocumentPage>(`/api/documents?page=${search.page}&page_size=${search.page_size}`, signal), refetchInterval: 15_000 });
  const data = query.data;
  return <>
    <header className="page-heading"><div><p className="eyebrow">БИБЛИОТЕКА ИСТОЧНИКОВ</p><h1>Собранные страницы<span>.</span></h1><p className="lead">Дискуссии о развитии Rust и предложения по изменению языка.</p></div><button onClick={() => void query.refetch()} disabled={query.isFetching}>Обновить ↻</button></header>
    <CrawlControls />
    <div className="toolbar"><span>{data ? <><strong>{number(data.total)}</strong> документов в коллекции</> : "Все источники"}</span><label>На странице <select value={search.page_size} onChange={e => void navigate({ search: { page: 1, page_size: Number(e.target.value) } })}>{[20,50,100].map(size => <option key={size}>{size}</option>)}</select></label></div>
    {query.isPending ? <Loading /> : query.isError ? <ErrorPanel error={query.error} retry={() => void query.refetch()} /> : data && <>
      {data.items.length === 0 ? <div className="empty"><span className="empty-icon">[ ]</span><h2>{data.total === 0 ? "Корпус пока пуст" : "На этой странице нет документов"}</h2><p>{data.total === 0 ? "Выберите источник и запустите сбор выше. Страницы появятся здесь автоматически." : "Перейдите на первую страницу коллекции."}</p>{data.total === 0 ? <a className="text-link" href="/swagger-ui/">Открыть API ↗</a> : <button onClick={() => void navigate({ search: { ...search, page: 1 } })}>На первую страницу</button>}</div> : <div className="documents">{data.items.map((doc, index) => <article key={doc.id} className="document">
        <span className="row-number">{String((data.page - 1) * data.page_size + index + 1).padStart(2, "0")}</span>
        <div className="document-body"><div className="document-meta"><span className={`source source-${doc.source}`}>{sourceName(doc.source)}</span><span>#{doc.native_id}</span><span title={doc.completeness_reason}><Badge value={doc.completeness} /></span></div><h2><a href={doc.canonical_url} target="_blank" rel="noopener noreferrer">{doc.title} <span aria-hidden>↗</span></a></h2><p className="excerpt">{doc.excerpt}</p><div className="document-footer"><span>{number(doc.text_chars)} символов</span><span>Получено {date(doc.last_seen_at)}</span></div></div>
      </article>)}</div>}
      <div className="pagination"><span>Страница {data.total_pages === 0 ? 0 : data.page} / {number(data.total_pages)}</span><div><button disabled={search.page === 1} onClick={() => void navigate({ search: { ...search, page: search.page - 1 } })}>← Назад</button><button disabled={search.page >= data.total_pages} onClick={() => void navigate({ search: { ...search, page: search.page + 1 } })}>Далее →</button></div></div>
    </>}
  </>;
}

function Stats() {
  const query = useQuery({ queryKey: ["stats"], queryFn: ({ signal }) => get<Statistics>("/api/stats", signal), refetchInterval: 5_000 });
  const data = query.data;
  return <>
    <header className="page-heading"><div><p className="eyebrow">СОСТОЯНИЕ КОЛЛЕКЦИИ</p><h1>Статистика<span>.</span></h1><p className="lead">Что получили, из каких источников и как прошёл сбор.</p></div><button onClick={() => void query.refetch()} disabled={query.isFetching}>Обновить ↻</button></header>
    {query.isPending ? <Loading /> : query.isError ? <ErrorPanel error={query.error} retry={() => void query.refetch()} /> : data && <>
      <div className="metrics"><Metric label="Уникальных документов" value={data.total_documents} accent /><Metric label="HTTP-запросов к страницам" value={data.document_requests} /><Metric label="HTTP / сетевых ошибок" value={data.http_errors} /></div>
      <div className="section-heading"><h2>Источники</h2><span>02 источника</span></div>
      <div className="source-grid">{data.sources.map(source => <section className="source-card" key={source.source}><div className="source-card-heading"><h3>{sourceName(source.source)}</h3><span className={source.paused ? "state paused" : "state"}>{source.paused ? "Приостановлен" : "Не приостановлен"}</span></div><strong className="source-total">{number(source.documents)} <small>документов</small></strong><dl><div><dt>Полных документов</dt><dd>{number(source.complete)}</dd></div><div><dt>Частичных / непроверенных</dt><dd>{number(source.partial)} / {number(source.unknown)}</dd></div><div><dt>Символов текста</dt><dd>{number(source.text_chars)}</dd></div><div><dt>Последний сбор</dt><dd>{date(source.last_collected_at)}</dd></div></dl>{source.pause_reason && <p className="error-text">{source.pause_reason}</p>}</section>)}</div>
      <p className="footnote">Число документов не равно числу HTTP-запросов. Повторный сбор обновляет документ, а «полный документ» означает, что скачаны все страницы темы подряд (на момент сбора).</p>
      <div className="section-heading"><h2>Последние запуски</h2><span>Последние 10</span></div>
      {data.recent_runs.length === 0 ? <div className="notice">Сбор ещё не запускался. Сервер сам не обращается к источникам.</div> : <div className="table-wrap"><table><thead><tr><th>Запуск / источник</th><th>Состояние</th><th>Обработано / найдено страниц</th><th>Ошибки</th><th>Начало</th></tr></thead><tbody>{data.recent_runs.map(run => <tr key={run.id}><td><strong>#{run.id} · {sourceName(run.source)}</strong>{run.error && <details><summary>Причина / подробности</summary><p>{run.error}</p></details>}</td><td><Badge value={run.status} /></td><td>{run.succeeded} / {run.requested}</td><td>{run.failed}</td><td>{date(run.started_at)}</td></tr>)}</tbody></table></div>}
    </>}
    <div className="stats-export"><ExportControls /></div>
  </>;
}
function Metric({ label, value, accent }: { label: string; value: number; accent?: boolean }) { return <section className={`metric ${accent ? "metric-accent" : ""}`}><span>{label}</span><strong>{number(value)}</strong></section>; }

const rootRoute = createRootRoute({ component: Layout, notFoundComponent: () => <div className="notice">Страница не найдена.</div> });
const documentsRoute = createRoute({ getParentRoute: () => rootRoute, path: "/", validateSearch: pagination, component: Documents });
const statsRoute = createRoute({ getParentRoute: () => rootRoute, path: "/stats", component: Stats });
const router = createRouter({ routeTree: rootRoute.addChildren([documentsRoute, statsRoute]) });
declare module "@tanstack/react-router" { interface Register { router: typeof router } }

ReactDOM.createRoot(document.getElementById("root")!).render(<React.StrictMode><QueryClientProvider client={queryClient}><RouterProvider router={router} /></QueryClientProvider></React.StrictMode>);
