import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { get, number, sourceName, startCrawl, type Statistics } from "./api";

export function CrawlControls() {
  const [source, setSource] = useState<"rfc" | "internals">("rfc");
  const client = useQueryClient();
  const stats = useQuery({ queryKey: ["stats"], queryFn: ({ signal }) => get<Statistics>("/api/stats", signal), refetchInterval: 5_000 });
  const mutation = useMutation({
    mutationFn: startCrawl,
    retry: false,
    onSuccess: () => { void client.invalidateQueries({ queryKey: ["stats"] }); void client.invalidateQueries({ queryKey: ["documents"] }); },
  });
  const active = stats.data?.recent_runs.find(run => run.status === "running");
  const selected = stats.data?.sources.find(item => item.source === source);
  const latest = stats.data?.recent_runs.find(run => run.source === source);
  const run = active ?? latest;
  return <section className="crawl-controls" aria-label="Сбор источника">
    <div><h2>Сбор страниц</h2><p>Обход ссылок выбранного источника. Незавершённый сбор продолжится с сохранённой очереди.</p></div>
    <form onSubmit={event => { event.preventDefault(); if (!active && !mutation.isPending && !selected?.paused && stats.isSuccess) mutation.mutate(source); }}>
      <label>Источник <select value={source} disabled={mutation.isPending} onChange={event => { setSource(event.target.value as "rfc" | "internals"); mutation.reset(); }}>
        <option value="rfc">Rust RFC</option><option value="internals">Rust Internals</option>
      </select></label>
      <button type="submit" disabled={mutation.isPending || !!active || !!selected?.paused || !stats.isSuccess}>
        {mutation.isPending ? "Запускаем…" : active ? "Сбор выполняется" : latest && ["failed", "interrupted", "blocked"].includes(latest.status) ? "Продолжить сбор" : "Начать сбор"}
      </button>
    </form>
    <div aria-live="polite">
      {mutation.isSuccess && <p>Запуск #{mutation.data.id} принят. Ход сбора обновляется автоматически.</p>}
      {run && <p>{sourceName(run.source)} · #{run.id}: обработано {number(run.succeeded)}, с ошибками {number(run.failed)}, в очереди {number(Math.max(0, run.requested - run.succeeded - run.failed))}. Всего обнаружено {number(run.requested)} страниц. {run.status === "completed" ? "Обход завершён." : run.status === "interrupted" ? "Обход прерван." : ""}</p>}
      {run?.error && <p className="error-text">{run.error}</p>}
    </div>
    {selected?.paused && <p className="error-text">Источник приостановлен: {selected.pause_reason}. После проверки причины снимите паузу через API источника.</p>}
    {mutation.isError && <p className="error-text" role="alert">{mutation.error.message}</p>}
    {stats.isError && <p className="error-text" role="alert">Не удалось проверить состояние сбора: {stats.error.message} <button onClick={() => void stats.refetch()}>Повторить</button></p>}
    <p className="footnote">Число найденных страниц растёт во время обхода. Списки и продолжения тем учитываются как страницы, но не как отдельные документы.</p>
  </section>;
}
