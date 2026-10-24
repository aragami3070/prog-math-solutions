import { Badge } from "../../components/Badge";
import { date } from "../../lib/format";
import { sourceName } from "../../lib/sources";
import type { CrawlRun } from "../../lib/types";

export function RunsTable({ runs }: { runs: CrawlRun[] }) {
  if (runs.length === 0) {
    return <div className="notice">Сбор ещё не запускался. Сервер сам не обращается к источникам.</div>;
  }
  return (
    <div className="table-wrap">
      <table>
        <thead>
          <tr>
            <th>Запуск / источник</th>
            <th>Состояние</th>
            <th>Обработано / найдено страниц</th>
            <th>Ошибки</th>
            <th>Начало</th>
          </tr>
        </thead>
        <tbody>
          {runs.map(run => (
            <tr key={run.id}>
              <td>
                <strong>#{run.id} · {sourceName(run.source)}</strong>
                {run.error && <details><summary>Причина / подробности</summary><p>{run.error}</p></details>}
              </td>
              <td><Badge value={run.status} /></td>
              <td>{run.succeeded} / {run.requested}</td>
              <td>{run.failed}</td>
              <td>{date(run.started_at)}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
