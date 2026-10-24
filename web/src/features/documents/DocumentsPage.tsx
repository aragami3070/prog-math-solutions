import { useQuery } from "@tanstack/react-query";
import { getRouteApi } from "@tanstack/react-router";
import { ErrorPanel, Loading } from "../../components/Feedback";
import { PageHeading } from "../../components/PageHeading";
import { Pagination } from "../../components/Pagination";
import { get } from "../../lib/client";
import { number } from "../../lib/format";
import { PAGE_SIZES } from "../../lib/pagination";
import type { DocumentPage } from "../../lib/types";
import { CrawlControls } from "../crawl/CrawlControls";
import { DocumentCard } from "./DocumentCard";

const route = getRouteApi("/");
const POLL_MS = 15_000;

export function DocumentsPage() {
  const search = route.useSearch();
  const navigate = route.useNavigate();
  const query = useQuery({
    queryKey: ["documents", search],
    queryFn: ({ signal }) => get<DocumentPage>(`/api/documents?page=${search.page}&page_size=${search.page_size}`, signal),
    refetchInterval: POLL_MS,
  });
  const data = query.data;
  const goTo = (page: number) => void navigate({ search: { ...search, page } });

  return (
    <>
      <PageHeading
        eyebrow="БИБЛИОТЕКА ИСТОЧНИКОВ"
        title="Собранные страницы"
        lead="Дискуссии о развитии Rust и предложения по изменению языка."
        onRefresh={() => void query.refetch()}
        refreshing={query.isFetching}
      />
      <CrawlControls />
      <div className="toolbar">
        <span>{data ? <><strong>{number(data.total)}</strong> документов в коллекции</> : "Все источники"}</span>
        <label>
          На странице{" "}
          <select value={search.page_size} onChange={event => void navigate({ search: { page: 1, page_size: Number(event.target.value) } })}>
            {PAGE_SIZES.map(size => <option key={size}>{size}</option>)}
          </select>
        </label>
      </div>
      {query.isPending ? <Loading /> : query.isError ? <ErrorPanel error={query.error} retry={() => void query.refetch()} /> : data && (
        <>
          {data.items.length === 0 ? (
            <EmptyState total={data.total} onFirstPage={() => goTo(1)} />
          ) : (
            <div className="documents">
              {data.items.map((doc, index) => (
                <DocumentCard key={doc.id} doc={doc} position={(data.page - 1) * data.page_size + index + 1} />
              ))}
            </div>
          )}
          <Pagination page={search.page} totalPages={data.total_pages} onChange={goTo} />
        </>
      )}
    </>
  );
}

function EmptyState({ total, onFirstPage }: { total: number; onFirstPage: () => void }) {
  const corpusEmpty = total === 0;
  return (
    <div className="empty">
      <span className="empty-icon">[ ]</span>
      <h2>{corpusEmpty ? "Корпус пока пуст" : "На этой странице нет документов"}</h2>
      <p>
        {corpusEmpty
          ? "Выберите источник и запустите сбор выше. Страницы появятся здесь автоматически."
          : "Перейдите на первую страницу коллекции."}
      </p>
      {corpusEmpty
        ? <a className="text-link" href="/swagger-ui/">Открыть API ↗</a>
        : <button onClick={onFirstPage}>На первую страницу</button>}
    </div>
  );
}
