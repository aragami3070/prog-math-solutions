import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { getRouteApi } from "@tanstack/react-router";
import { DocumentCard } from "../../components/DocumentCard";
import { ErrorPanel, Loading } from "../../components/Feedback";
import { PageHeading } from "../../components/PageHeading";
import { Pagination } from "../../components/Pagination";
import { get, rebuildSearchIndex } from "../../lib/client";
import { number } from "../../lib/format";
import { searchPath } from "../../lib/pagination";
import { SOURCES } from "../../lib/sources";
import type { SearchResults } from "../../lib/types";

const route = getRouteApi("/search");
const PAGE_SIZE = 20;

export function SearchPage() {
  const params = route.useSearch();
  const navigate = route.useNavigate();
  const client = useQueryClient();
  const [draft, setDraft] = useState(params.q);
  const searching = params.q.trim() !== "";

  const query = useQuery({
    queryKey: ["search", params],
    queryFn: ({ signal }) => get<SearchResults>(searchPath(params, PAGE_SIZE), signal),
    enabled: searching,
    retry: false,
  });
  const rebuild = useMutation({
    mutationFn: rebuildSearchIndex,
    onSuccess: () => void client.invalidateQueries({ queryKey: ["search"] }),
  });

  const data = query.data;
  const submit = (event: React.FormEvent) => {
    event.preventDefault();
    void navigate({ search: { ...params, q: draft.trim(), page: 1 } });
  };

  return (
    <>
      <PageHeading
        eyebrow="ПОИСК ПО КОРПУСУ"
        title="Поиск"
        lead="Слова, «фраза в кавычках» или номер: rfc:3058, RFC 3058. Лексический поиск (BM25) с необязательным исправлением опечаток и синонимами."
        onRefresh={() => void query.refetch()}
        refreshing={query.isFetching}
      />
      <form className="search-form" role="search" onSubmit={submit}>
        <input
          type="search"
          value={draft}
          maxLength={200}
          placeholder="async fn, !Send, generic associated types…"
          aria-label="Поисковый запрос"
          onChange={event => setDraft(event.target.value)}
        />
        <button type="submit" disabled={draft.trim() === ""}>Найти</button>
        <label>
          Источник{" "}
          <select value={params.source} onChange={event => void navigate({ search: { ...params, source: event.target.value as typeof params.source, page: 1 } })}>
            <option value="">Все</option>
            {SOURCES.map(item => <option key={item.id} value={item.id}>{item.name}</option>)}
          </select>
        </label>
        <label>
          <input type="checkbox" checked={params.complete_only} onChange={event => void navigate({ search: { ...params, complete_only: event.target.checked, page: 1 } })} />{" "}
          Только полные
        </label>
        <label>
          <input type="checkbox" checked={params.fuzzy} onChange={event => void navigate({ search: { ...params, fuzzy: event.target.checked, page: 1 } })} />{" "}
          Исправлять опечатки
        </label>
        <label>
          <input type="checkbox" checked={params.synonyms} onChange={event => void navigate({ search: { ...params, synonyms: event.target.checked, page: 1 } })} />{" "}
          Синонимы
        </label>
      </form>

      {!searching ? (
        <div className="notice">Введите запрос. Поиск идёт по названию (вес выше) и тексту всех постов темы.</div>
      ) : query.isPending ? <Loading /> : query.isError ? (
        <IndexProblem error={query.error} rebuild={rebuild} retry={() => void query.refetch()} />
      ) : data && (
        <>
          <div className="toolbar">
            <span><strong>{number(data.total)}</strong> найдено · {data.took_ms} мс</span>
            <span title={`Анализатор ${data.scope.analyzer_version}, поколение ${data.scope.generation}`}>
              Индекс: {number(data.scope.indexed_documents)} документов
            </span>
          </div>
          {(data.rewrite.corrections.length > 0 || data.rewrite.expansions.length > 0) && (
            <p className="notice" role="status">
              {data.rewrite.corrections.length > 0 && <>Исправлено: {data.rewrite.corrections.map(item => `${item.from} → ${item.to}`).join(", ")}. Исходные слова тоже учтены. </>}
              {data.rewrite.expansions.length > 0 && <>Добавлены синонимы: {data.rewrite.expansions.map(item => `${item.from} ≈ ${item.to.join(" / ")}`).join("; ")}.</>}
            </p>
          )}
          {data.hits.length === 0 ? (
            <div className="empty"><h2>Ничего не найдено</h2><p>Попробуйте другие слова, уберите фильтры или включите исправление опечаток и синонимы.</p></div>
          ) : (
            <div className="documents">
              {data.hits.map(hit => (
                <DocumentCard
                  key={hit.id}
                  source={hit.source}
                  label={hit.document_id}
                  title={hit.title}
                  url={hit.url}
                  completeness={hit.completeness}
                >
                  {/* Plain text parts: matches are marked here, no HTML comes from the server. */}
                  {hit.snippet.map((part, index) => part.matched ? <mark key={index}>{part.text}</mark> : part.text)}
                </DocumentCard>
              ))}
            </div>
          )}
          <Pagination
            page={params.page}
            totalPages={Math.min(Math.ceil(data.total / PAGE_SIZE), Math.floor(data.scope.max_depth / PAGE_SIZE))}
            onChange={page => void navigate({ search: { ...params, page } })}
          />
          <p className="footnote">«Найдено» — все совпавшие документы индекса, а не только эта страница. Глубже {number(data.scope.max_depth)} результатов выдача недоступна.</p>
        </>
      )}
      <section className="crawl-controls" aria-label="Индекс поиска">
        <h2>Индекс</h2>
        <p>Индекс строится из базы целиком и не обновляется сам. Запускайте после сбора; старый индекс отвечает, пока строится новый.</p>
        <button onClick={() => rebuild.mutate()} disabled={rebuild.isPending}>{rebuild.isPending ? "Строим…" : "Перестроить индекс"}</button>
        <div aria-live="polite">
          {rebuild.isSuccess && <p>Готово: {number(rebuild.data.documents)} документов за {number(rebuild.data.took_ms)} мс.</p>}
          {rebuild.isError && <p className="error-text" role="alert">{rebuild.error.message}</p>}
        </div>
      </section>
    </>
  );
}

function IndexProblem({ error, rebuild, retry }: { error: Error; rebuild: { mutate: () => void; isPending: boolean }; retry: () => void }) {
  const notBuilt = error.message.includes("not built");
  if (!notBuilt) return <ErrorPanel error={error} retry={retry} />;
  return (
    <div className="notice">
      <strong>Индекс ещё не построен</strong>
      <p>Постройте его из собранных документов, затем повторите запрос.</p>
      <button onClick={rebuild.mutate} disabled={rebuild.isPending}>Построить индекс</button>
    </div>
  );
}
