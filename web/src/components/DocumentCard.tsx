import type { ReactNode } from "react";
import { sourceName } from "../lib/sources";
import type { Completeness, SourceId } from "../lib/types";
import { Badge } from "./Badge";

interface Props {
  source: SourceId;
  /** Shown next to the source, e.g. `#21306` or `rfc:3058`. */
  label: string;
  title: string;
  url: string;
  completeness: Completeness;
  completenessReason?: string;
  /** Row number in a list; omit where numbering means nothing, e.g. ranked results. */
  position?: number;
  /** The text under the title: an excerpt or a snippet. */
  children?: ReactNode;
  footer?: ReactNode;
}

/** One document in any list: the catalogue, search results, and whatever comes next. */
export function DocumentCard({ source, label, title, url, completeness, completenessReason, position, children, footer }: Props) {
  return (
    <article className={position === undefined ? "document document-plain" : "document"}>
      {position !== undefined && <span className="row-number">{String(position).padStart(2, "0")}</span>}
      <div className="document-body">
        <div className="document-meta">
          <span className={`source source-${source}`}>{sourceName(source)}</span>
          <span>{label}</span>
          <span title={completenessReason}><Badge value={completeness} /></span>
        </div>
        <h2>
          <a href={url} target="_blank" rel="noopener noreferrer">
            {title} <span aria-hidden>↗</span>
          </a>
        </h2>
        {children && <p className="excerpt">{children}</p>}
        {footer && <div className="document-footer">{footer}</div>}
      </div>
    </article>
  );
}
