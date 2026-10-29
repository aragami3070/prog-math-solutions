export type SourceId = "rfc" | "internals";
export type Completeness = "complete" | "partial" | "unknown";
export type RunStatus = "running" | "completed" | "failed" | "blocked" | "interrupted";
export type ExportFormat = "jsonl" | "json" | "csv" | "zip";

export interface DocumentSummary {
  id: number;
  source: SourceId;
  native_id: string;
  canonical_url: string;
  title: string;
  excerpt: string;
  text_chars: number;
  completeness: Completeness;
  completeness_reason: string;
  first_seen_at: string;
  last_seen_at: string;
}

export interface DocumentPage {
  items: DocumentSummary[];
  page: number;
  page_size: number;
  total: number;
  total_pages: number;
}

export interface CrawlRun {
  id: number;
  source: SourceId;
  status: RunStatus;
  requested: number;
  succeeded: number;
  failed: number;
  /** Pages the source reports as not existing (404/410): skipped, not errors. */
  missing: number;
  started_at: string;
  finished_at: string | null;
  error: string | null;
}

export interface SourceStats {
  source: SourceId;
  documents: number;
  complete: number;
  partial: number;
  unknown: number;
  text_chars: number;
  last_collected_at: string | null;
  paused: boolean;
  pause_reason: string | null;
}

export interface Statistics {
  total_documents: number;
  document_requests: number;
  http_errors: number;
  sources: SourceStats[];
  recent_runs: CrawlRun[];
}

export interface SnippetPart {
  text: string;
  matched: boolean;
}

export interface SearchHit {
  id: number;
  document_id: string;
  source: SourceId;
  title: string;
  url: string;
  completeness: Completeness;
  score: number;
  snippet: SnippetPart[];
}

export interface SearchResults {
  query: string;
  hits: SearchHit[];
  total: number;
  page: number;
  page_size: number;
  scope: {
    mode: string;
    analyzer_version: string;
    generation: string;
    indexed_documents: number;
    max_depth: number;
  };
  took_ms: number;
}

export interface RebuildResult {
  generation: string;
  documents: number;
  took_ms: number;
}
