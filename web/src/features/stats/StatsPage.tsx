import { ErrorPanel, Loading } from "../../components/Feedback";
import { PageHeading } from "../../components/PageHeading";
import { useStats } from "../../hooks/useStats";
import { number, plural } from "../../lib/format";
import { ExportControls } from "../export/ExportControls";
import { RunsTable } from "./RunsTable";
import { SourceCard } from "./SourceCard";

function Metric({ label, value, accent }: { label: string; value: number; accent?: boolean }) {
  return (
    <section className={`metric ${accent ? "metric-accent" : ""}`}>
      <span>{label}</span>
      <strong>{number(value)}</strong>
    </section>
  );
}

export function StatsPage() {
  const query = useStats();
  const data = query.data;
  return (
    <>
      <PageHeading
        eyebrow="СОСТОЯНИЕ КОЛЛЕКЦИИ"
        title="Статистика"
        lead="Что получили, из каких источников и как прошёл сбор."
        onRefresh={() => void query.refetch()}
        refreshing={query.isFetching}
      />
      {query.isPending ? <Loading /> : query.isError ? <ErrorPanel error={query.error} retry={() => void query.refetch()} /> : data && (
        <>
          <div className="metrics">
            <Metric label="Уникальных документов" value={data.total_documents} accent />
            <Metric label="HTTP-запросов к страницам" value={data.document_requests} />
            <Metric label="HTTP / сетевых ошибок" value={data.http_errors} />
          </div>
          <div className="section-heading">
            <h2>Источники</h2>
            <span>{data.sources.length} {plural(data.sources.length, "источник", "источника", "источников")}</span>
          </div>
          <div className="source-grid">
            {data.sources.map(source => <SourceCard key={source.source} source={source} />)}
          </div>
          <p className="footnote">
            Число документов не равно числу HTTP-запросов. Повторный сбор обновляет документ, а «полный документ»
            означает, что скачаны все страницы темы подряд (на момент сбора).
          </p>
          <div className="section-heading"><h2>Последние запуски</h2><span>Последние 10</span></div>
          <RunsTable runs={data.recent_runs} />
        </>
      )}
      <div className="stats-export"><ExportControls /></div>
    </>
  );
}
