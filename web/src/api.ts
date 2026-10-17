export interface DocumentSummary {
  id: number;
  source: string;
  native_id: string;
  canonical_url: string;
  title: string;
  excerpt: string;
  text_chars: number;
  completeness: string;
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
  source: string;
  status: string;
  requested: number;
  succeeded: number;
  failed: number;
  started_at: string;
  finished_at: string | null;
  error: string | null;
}
export interface SourceStats {
  source: string;
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

export async function get<T>(path: string, signal?: AbortSignal): Promise<T> {
  const response = await fetch(path, { signal, headers: { Accept: "application/json" } });
  return decode<T>(response);
}

export async function startCrawl(source: "rfc" | "internals"): Promise<CrawlRun> {
  const response = await fetch("/api/crawls", {
    method: "POST",
    headers: { Accept: "application/json", "Content-Type": "application/json" },
    body: JSON.stringify({ source }),
  });
  return decode<CrawlRun>(response);
}

async function decode<T>(response: Response): Promise<T> {
  if (!response.ok) {
    let message = `Сервер вернул HTTP ${response.status}`;
    try { const data = await response.json(); if (typeof data.error === "string") message = data.error; } catch { /* Keep a bounded, readable error. */ }
    throw new Error(message);
  }
  return response.json() as Promise<T>;
}

export function pagination(search: Record<string, unknown>) {
  const page = Number(search.page ?? 1);
  const pageSize = Number(search.page_size ?? 20);
  return {
    page: Number.isInteger(page) && page >= 1 && page <= 1_000_000 ? page : 1,
    page_size: [20, 50, 100].includes(pageSize) ? pageSize : 20,
  };
}

export function sourceName(source: string) { return source === "rfc" ? "Rust RFC" : "Rust Internals"; }
export function number(value: number) { return new Intl.NumberFormat("ru-RU").format(value); }
export function date(value: string | null) {
  return value ? new Date(value).toLocaleString("ru-RU", { dateStyle: "short", timeStyle: "short" }) : "Ещё не собирали";
}
