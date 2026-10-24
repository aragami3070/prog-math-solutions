import { Badge } from "../../components/Badge";
import { date, number } from "../../lib/format";
import { sourceName } from "../../lib/sources";
import type { DocumentSummary } from "../../lib/types";

export function DocumentCard({ doc, position }: { doc: DocumentSummary; position: number }) {
  return (
    <article className="document">
      <span className="row-number">{String(position).padStart(2, "0")}</span>
      <div className="document-body">
        <div className="document-meta">
          <span className={`source source-${doc.source}`}>{sourceName(doc.source)}</span>
          <span>#{doc.native_id}</span>
          <span title={doc.completeness_reason}><Badge value={doc.completeness} /></span>
        </div>
        <h2>
          <a href={doc.canonical_url} target="_blank" rel="noopener noreferrer">
            {doc.title} <span aria-hidden>↗</span>
          </a>
        </h2>
        <p className="excerpt">{doc.excerpt}</p>
        <div className="document-footer">
          <span>{number(doc.text_chars)} символов</span>
          <span>Получено {date(doc.last_seen_at)}</span>
        </div>
      </div>
    </article>
  );
}
