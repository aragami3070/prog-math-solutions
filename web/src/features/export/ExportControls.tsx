import { useState } from "react";
import { useMutation } from "@tanstack/react-query";
import { download, exportUrl } from "../../lib/client";
import { SOURCES } from "../../lib/sources";
import type { ExportFormat } from "../../lib/types";

const FORMATS: ExportFormat[] = ["jsonl", "json", "csv", "zip"];

export function ExportControls() {
  const [source, setSource] = useState("");
  const [completeOnly, setCompleteOnly] = useState(false);
  const [safe, setSafe] = useState(false);
  const save = useMutation({
    mutationFn: (format: ExportFormat) => download(exportUrl(format, source, completeOnly, safe), `corpus.${format}`),
    retry: false,
  });

  return (
    <section className="crawl-controls" aria-label="Экспорт корпуса">
      <h2>Экспорт корпуса</h2>
      <p>Все собранные документы, а не только текущая страница. Полный текст и метаданные на момент начала выгрузки.</p>
      <div className="export-options">
        <label>
          Источник{" "}
          <select value={source} onChange={event => setSource(event.target.value)}>
            <option value="">Все источники</option>
            {SOURCES.map(item => <option key={item.id} value={item.id}>{item.name}</option>)}
          </select>
        </label>
        <label>
          <input type="checkbox" checked={completeOnly} onChange={event => setCompleteOnly(event.target.checked)} />{" "}
          Только подтверждённо полные
        </label>
        <label>
          <input type="checkbox" checked={safe} onChange={event => setSafe(event.target.checked)} />{" "}
          CSV для таблиц: защитить ячейки от формул
        </label>
      </div>
      <div className="export-options">
        {FORMATS.map(format => (
          <button className="text-link" key={format} disabled={save.isPending} onClick={() => save.mutate(format)}>
            Скачать {format === "zip" ? "ZIP + manifest" : format.toUpperCase()} ↓
          </button>
        ))}
      </div>
      <div aria-live="polite">
        {save.isPending && <p>Готовим файл, это может занять время…</p>}
        {save.isError && <p className="error-text" role="alert">Не удалось выгрузить: {save.error.message}</p>}
      </div>
      <p className="footnote">
        ZIP содержит все три формата и manifest с количеством документов и SHA-256 файлов. Тема считается полной,
        если скачаны все её страницы подряд; фильтр полных документов исключает остальные.
      </p>
    </section>
  );
}
