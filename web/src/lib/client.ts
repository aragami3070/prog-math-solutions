import type { CrawlRun, ExportFormat, RebuildResult, SourceId } from "./types";

export async function get<T>(path: string, signal?: AbortSignal): Promise<T> {
  const response = await fetch(path, { signal, headers: { Accept: "application/json" } });
  return decode<T>(response);
}

export async function startCrawl(source: SourceId): Promise<CrawlRun> {
  const response = await fetch("/api/crawls", {
    method: "POST",
    headers: { Accept: "application/json", "Content-Type": "application/json" },
    body: JSON.stringify({ source }),
  });
  return decode<CrawlRun>(response);
}

export function exportUrl(format: ExportFormat, source = "", completeOnly = false, spreadsheetSafe = false) {
  const query = new URLSearchParams({ format, complete_only: String(completeOnly), spreadsheet_safe: String(spreadsheetSafe) });
  if (source) query.set("source", source);
  return `/api/sources/export?${query}`;
}

/** Downloads a file through fetch, so an API error is reported on the page instead of a blank tab. */
export async function download(url: string, fallbackName: string) {
  const response = await fetch(url);
  if (!response.ok) throw new Error(await errorMessage(response));
  const blob = await response.blob();
  const name = /filename="?([^";]+)"?/.exec(response.headers.get("content-disposition") ?? "")?.[1] ?? fallbackName;
  const link = Object.assign(document.createElement("a"), { href: URL.createObjectURL(blob), download: name });
  link.click();
  URL.revokeObjectURL(link.href);
}

async function errorMessage(response: Response) {
  try {
    const data = await response.json();
    if (typeof data.error === "string") return data.error;
  } catch { /* Keep a bounded, readable error. */ }
  return `Сервер вернул HTTP ${response.status}`;
}

async function decode<T>(response: Response): Promise<T> {
  if (!response.ok) throw new Error(await errorMessage(response));
  return response.json() as Promise<T>;
}

export async function rebuildSearchIndex(): Promise<RebuildResult> {
  const response = await fetch("/api/search/rebuild", { method: "POST", headers: { Accept: "application/json" } });
  return decode<RebuildResult>(response);
}
