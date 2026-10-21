import { useState } from "react";
import { exportUrl, type ExportFormat } from "./api";

const formats: ExportFormat[] = ["jsonl", "json", "csv", "zip"];

export function ExportControls() {
  const [source, setSource] = useState("");
  const [completeOnly, setCompleteOnly] = useState(false);
  const [safe, setSafe] = useState(false);
  return <section className="crawl-controls" aria-label="Экспорт корпуса">
    <h2>Экспорт корпуса</h2>
    <p>Все собранные документы, а не только текущая страница. Полный текст и метаданные на момент начала выгрузки.</p>
    <div className="export-options">
      <label>Источник <select value={source} onChange={event => setSource(event.target.value)}>
        <option value="">Все источники</option><option value="internals">Rust Internals</option><option value="rfc">Rust RFC</option>
      </select></label>
      <label><input type="checkbox" checked={completeOnly} onChange={event => setCompleteOnly(event.target.checked)} /> Только подтверждённо полные</label>
      <label><input type="checkbox" checked={safe} onChange={event => setSafe(event.target.checked)} /> CSV для таблиц: защитить ячейки от формул</label>
    </div>
    <div className="export-options">
      {formats.map(format => <a className="text-link" href={exportUrl(format, source, completeOnly, safe)} target="_blank" rel="noopener noreferrer" key={format}>Скачать {format === "zip" ? "ZIP + manifest" : format.toUpperCase()} ↓</a>)}
    </div>
    <p className="footnote">ZIP содержит все три формата и manifest с количеством документов и SHA-256 файлов. Тема считается полной, если скачаны все её страницы подряд; фильтр полных документов исключает остальные. Подготовка файла может занять время; браузер начнёт скачивание, когда он будет готов. Ошибка API откроется в новой вкладке.</p>
  </section>;
}
