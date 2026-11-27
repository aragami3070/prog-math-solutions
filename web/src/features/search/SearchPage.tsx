import { useEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { getRouteApi } from "@tanstack/react-router";
import { DocumentCard } from "../../components/DocumentCard";
import { ErrorPanel, Loading } from "../../components/Feedback";
import { PageHeading } from "../../components/PageHeading";
import { Pagination } from "../../components/Pagination";
import { get, rebuildSearchIndex } from "../../lib/client";
import { number } from "../../lib/format";
import { POOL_SIZES, searchPath, selectSearchTopic } from "../../lib/pagination";
import { SOURCES } from "../../lib/sources";
import type { SearchResults } from "../../lib/types";

const route = getRouteApi("/search");
const PAGE_SIZE = 20;

export function SearchPage() {
  const params = route.useSearch();
  const navigate = route.useNavigate();
  const client = useQueryClient();
  const [draft, setDraft] = useState(params.q);
  useEffect(() => setDraft(params.q), [params.q]);
  const searching = params.q.trim() !== "" || !!params.topic;

  const taxonomy = useQuery({
    queryKey: ["taxonomy", "v3"],
    queryFn: ({ signal }) => get<{ version: string; topics: { id: string; label: string }[] }>("/api/taxonomy", signal),
    retry: false,
  });
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
        lead="Слова, «фраза в кавычках» или номер: rfc:3058, RFC 3058. Лексический (BM25), векторный (смысл) или гибридный режим; опечатки, синонимы и формы слов — по желанию."
        onRefresh={() => void query.refetch()}
        refreshing={query.isFetching}
      />
      <form className="search-form" role="search" onSubmit={submit}>
        <div className="search-query">
        <input
          type="search"
          value={draft}
          maxLength={200}
          placeholder="async fn, !Send, generic associated types…"
          aria-label="Поисковый запрос"
          onChange={event => setDraft(event.target.value)}
        />
        <button type="submit" disabled={draft.trim() === "" && !params.topic}>Найти</button>
        </div>
        <div className="search-filters">
        <label className="search-field">
          Режим{" "}
          <select value={params.mode} disabled={!!params.topic} onChange={event => void navigate({ search: { ...params, mode: event.target.value as typeof params.mode, page: 1 } })}>
            <option value="lexical">Лексический</option>
            <option value="vector">Векторный</option>
            <option value="hybrid">Гибридный</option>
          </select>
        </label>
        <div className="search-topic">
        <label className="search-field">
          Тема{" "}
          <select value={params.topic ?? ""} onChange={event => void navigate({ search: selectSearchTopic(params, event.target.value) })}>
            <option value="">Все темы</option>
            {params.topic && !taxonomy.data?.topics.some(topic => topic.id === params.topic) && <option value={params.topic}>{params.topic}</option>}
            {taxonomy.data?.topics.map(topic => <option key={topic.id} value={topic.id}>{topic.label}</option>)}
          </select>
        </label>
        {taxonomy.isPending && <span role="status">Загрузка тем…</span>}
        {taxonomy.isError && <span role="alert">Не удалось загрузить темы. <button type="button" onClick={() => void taxonomy.refetch()}>Повторить</button></span>}
        </div>
        <label className="search-field">
          Источник{" "}
          <select value={params.source} onChange={event => void navigate({ search: { ...params, source: event.target.value as typeof params.source, page: 1 } })}>
            <option value="">Все</option>
            {SOURCES.map(item => <option key={item.id} value={item.id}>{item.name}</option>)}
          </select>
        </label>
        {params.mode !== "lexical" && (
          <label className="search-field" title="Сколько кандидатов даёт каждый список (векторный и лексический). Глубже этого число результатов не листается; больше пул — дальше по выдаче и дороже запрос.">
            Пул кандидатов{" "}
            <select value={params.pool ?? 100} onChange={event => void navigate({ search: { ...params, pool: Number(event.target.value), page: 1 } })}>
              {POOL_SIZES.map(size => <option key={size} value={size}>{size}</option>)}
            </select>
          </label>
        )}
        </div>
        <fieldset className="search-options">
        <legend>Дополнительные параметры</legend>
        <label>
          <input type="checkbox" checked={params.complete_only} onChange={event => void navigate({ search: { ...params, complete_only: event.target.checked, page: 1 } })} />{" "}
          Только полные
        </label>
        <label>
          <input type="checkbox" checked={params.fuzzy} onChange={event => void navigate({ search: { ...params, fuzzy: event.target.checked, page: 1 } })} />{" "}
          Исправлять опечатки
        </label>
        <label title="Каждое слово запроса находит и близкие слова в индексе (расстояние правки 1–2, Tantivy FuzzyTermQuery); запрос не переписывается">
          <input type="checkbox" checked={params.fuzzy_match} onChange={event => void navigate({ search: { ...params, fuzzy_match: event.target.checked, page: 1 } })} />{" "}
          Нечёткое совпадение
        </label>
        <label>
          <input type="checkbox" checked={params.synonyms} onChange={event => void navigate({ search: { ...params, synonyms: event.target.checked, page: 1 } })} />{" "}
          Синонимы
        </label>
        <label>
          <input type="checkbox" checked={params.lemmas} onChange={event => void navigate({ search: { ...params, lemmas: event.target.checked, page: 1 } })} />{" "}
          Формы слов
        </label>
        </fieldset>
      </form>

      {params.topic && <p className="notice" role="status">Тема классификации: <strong>{taxonomy.data?.topics.find(topic => topic.id === params.topic)?.label ?? params.topic}</strong>. Фильтр доступен только в лексическом поиске; машинные метки могут быть непроверенными. {" "}<button type="button" onClick={() => void navigate({ search: selectSearchTopic(params, "") })}>Убрать тему</button></p>}
      {!searching ? (
        <div className="notice">Введите запрос или выберите тему. Поиск идёт по названию (вес выше) и тексту всех постов темы.</div>
      ) : query.isPending ? <Loading /> : query.isError ? (
        <IndexProblem error={query.error} rebuild={rebuild} retry={() => void query.refetch()} />
      ) : data && (
        <>
          <div className="toolbar">
            <span><strong>{number(data.total)}</strong> {data.scope.total_is_pool ? "кандидатов" : "найдено"} · {data.took_ms} мс · режим: {data.scope.mode}</span>
            <span title={`Анализатор ${data.scope.analyzer_version}, поколение ${data.scope.generation}`}>
              Индекс: {number(data.scope.indexed_documents)} документов
            </span>
          </div>
          {data.scope.degraded && (
            <p className="notice" role="alert">
              Режим «{data.scope.requested_mode}» недоступен: {data.scope.degraded}. Показан лексический поиск.
            </p>
          )}
          {data.rewrite.unavailable.length > 0 && (
            <p className="notice" role="status">Не сработало: {data.rewrite.unavailable.join(", ")} (нет таблицы лемм).</p>
          )}
          {(data.rewrite.corrections.length > 0 || data.rewrite.expansions.length > 0 || data.rewrite.word_forms.length > 0) && (
            <p className="notice" role="status">
              {data.rewrite.corrections.length > 0 && <>Исправлено: {data.rewrite.corrections.map(item => `${item.from} → ${item.to}`).join(", ")}. Исходные слова тоже учтены. </>}
              {data.rewrite.expansions.length > 0 && <>Добавлены синонимы: {data.rewrite.expansions.map(item => `${item.from} ≈ ${item.to.join(" / ")}`).join("; ")}. </>}
              {data.rewrite.word_forms.length > 0 && <>Формы слов: {data.rewrite.word_forms.map(item => `${item.from} ≈ ${item.to.join(" / ")}`).join("; ")}.</>}
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
