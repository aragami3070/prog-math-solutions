export const PAGE_SIZES = [20, 50, 100];

/** Validates `?page=&page_size=` from the URL so a hand-edited link cannot break the list. */
export function pagination(search: Record<string, unknown>) {
  const page = Number(search.page ?? 1);
  const pageSize = Number(search.page_size ?? PAGE_SIZES[0]);
  return {
    page: Number.isInteger(page) && page >= 1 && page <= 1_000_000 ? page : 1,
    page_size: PAGE_SIZES.includes(pageSize) ? pageSize : PAGE_SIZES[0],
  };
}

export interface SearchParams {
  q: string;
  source: "" | "rfc" | "internals";
  complete_only: boolean;
  page: number;
}

/** Validates `/search?q=&source=&complete_only=&page=` from the URL. */
export function searchParams(search: Record<string, unknown>): SearchParams {
  const page = Number(search.page ?? 1);
  return {
    q: typeof search.q === "string" ? search.q.slice(0, 200) : "",
    source: search.source === "rfc" || search.source === "internals" ? search.source : "",
    complete_only: search.complete_only === true || search.complete_only === "true",
    page: Number.isInteger(page) && page >= 1 && page <= 1_000_000 ? page : 1,
  };
}

/** The API path for a search; `page_size` is fixed. */
export function searchPath({ q, source, complete_only, page }: SearchParams, pageSize = 20) {
  const query = new URLSearchParams({ q, page: String(page), page_size: String(pageSize) });
  if (source) query.set("source", source);
  if (complete_only) query.set("complete_only", "true");
  return `/api/search?${query}`;
}
