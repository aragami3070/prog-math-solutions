import { date, number } from "../../lib/format";
import { sourceName } from "../../lib/sources";
import type { SourceStats } from "../../lib/types";

export function SourceCard({ source }: { source: SourceStats }) {
  return (
    <section className="source-card">
      <div className="source-card-heading">
        <h3>{sourceName(source.source)}</h3>
        <span className={source.paused ? "state paused" : "state"}>
          {source.paused ? "Приостановлен" : "Не приостановлен"}
        </span>
      </div>
      <strong className="source-total">{number(source.documents)} <small>документов</small></strong>
      <dl>
        <div><dt>Полных документов</dt><dd>{number(source.complete)}</dd></div>
        <div><dt>Частичных / непроверенных</dt><dd>{number(source.partial)} / {number(source.unknown)}</dd></div>
        <div><dt>Символов текста</dt><dd>{number(source.text_chars)}</dd></div>
        <div><dt>Последний сбор</dt><dd>{date(source.last_collected_at)}</dd></div>
      </dl>
      {source.pause_reason && <p className="error-text">{source.pause_reason}</p>}
    </section>
  );
}
