import { useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useStats } from "../../hooks/useStats";
import { startCrawl } from "../../lib/client";
import { number } from "../../lib/format";
import { SOURCES, sourceName } from "../../lib/sources";
import type { RunStatus, SourceId } from "../../lib/types";

const RESUMABLE: RunStatus[] = ["failed", "interrupted", "blocked"];

export function CrawlControls() {
  const [source, setSource] = useState<SourceId>("rfc");
  const client = useQueryClient();
  const stats = useStats();
  const mutation = useMutation({
    mutationFn: startCrawl,
    retry: false,
    onSuccess: () => {
      void client.invalidateQueries({ queryKey: ["stats"] });
      void client.invalidateQueries({ queryKey: ["documents"] });
    },
  });

  const runs = stats.data?.recent_runs;
  const active = runs?.find(run => run.status === "running");
  const selected = stats.data?.sources.find(item => item.source === source);
  const latest = runs?.find(run => run.source === source);
  const run = active ?? latest;
  const canStart = stats.isSuccess && !mutation.isPending && !active && !selected?.paused;
  const label = mutation.isPending ? "Запускаем…"
    : active ? "Сбор выполняется"
    : latest && RESUMABLE.includes(latest.status) ? "Продолжить сбор"
    : "Начать сбор";
  const queued = run ? Math.max(0, run.requested - run.succeeded - run.failed) : 0;

  return (
    <section className="crawl-controls" aria-label="Сбор источника">
      <div>
        <h2>Сбор страниц</h2>
        <p>Обход ссылок выбранного источника. Незавершённый сбор продолжится с сохранённой очереди.</p>
      </div>
      <form onSubmit={event => { event.preventDefault(); if (canStart) mutation.mutate(source); }}>
        <label>
          Источник{" "}
          <select
            value={source}
            disabled={mutation.isPending}
            onChange={event => { setSource(event.target.value as SourceId); mutation.reset(); }}
          >
            {SOURCES.map(item => <option key={item.id} value={item.id}>{item.name}</option>)}
          </select>
        </label>
        <button type="submit" disabled={!canStart}>{label}</button>
      </form>
      <div aria-live="polite">
        {mutation.isSuccess && <p>Запуск #{mutation.data.id} принят. Ход сбора обновляется автоматически.</p>}
        {run && (
          <p>
            {sourceName(run.source)} · #{run.id}: обработано {number(run.succeeded)}, с ошибками {number(run.failed)},
            в очереди {number(queued)}. Всего обнаружено {number(run.requested)} страниц.{" "}
            {run.status === "completed" ? "Обход завершён." : run.status === "interrupted" ? "Обход прерван." : ""}
          </p>
        )}
        {run?.error && <p className="error-text">{run.error}</p>}
      </div>
      {selected?.paused && (
        <p className="error-text">
          Источник приостановлен: {selected.pause_reason}. После проверки причины снимите паузу через API источника.
        </p>
      )}
      {mutation.isError && <p className="error-text" role="alert">{mutation.error.message}</p>}
      {stats.isError && (
        <p className="error-text" role="alert">
          Не удалось проверить состояние сбора: {stats.error.message}{" "}
          <button onClick={() => void stats.refetch()}>Повторить</button>
        </p>
      )}
      <p className="footnote">
        Число найденных страниц растёт во время обхода. Списки и продолжения тем учитываются как страницы,
        но не как отдельные документы.
      </p>
    </section>
  );
}
