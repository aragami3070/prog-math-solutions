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

/** Candidate pool sizes offered for the vector and hybrid modes; the server default is 100. */
export const POOL_SIZES = [50, 100, 200, 300, 500];

export type SearchMode = "lexical" | "vector" | "hybrid";

export interface SearchParams {
  q: string;
  topic?: string;
  source: "" | "rfc" | "internals";
  complete_only: boolean;
  /** Typo repair and synonym expansion; on unless the URL turns them off. */
  fuzzy: boolean;
  synonyms: boolean;
  lemmas: boolean;
  /** Fuzzy matching inside the index (edit distance); off unless the URL turns it on. */
  fuzzy_match: boolean;
  mode: SearchMode;
  /** Candidates per ranking in vector and hybrid modes; omitted means the server default. */
  pool?: number;
  page: number;
}

/** Validates `/search?q=&source=&complete_only=&page=` from the URL. */
export function searchParams(search: Record<string, unknown>): SearchParams {
  const page = Number(search.page ?? 1);
  const topic = typeof search.topic === "string" && /^[a-z0-9]+(?:-[a-z0-9]+)*$/.test(search.topic) && search.topic.length <= 100 ? search.topic : undefined;
  return {
    q: typeof search.q === "string" ? search.q.slice(0, 200) : "",
    ...(topic && { topic }),
    source: search.source === "rfc" || search.source === "internals" ? search.source : "",
    complete_only: search.complete_only === true || search.complete_only === "true",
    fuzzy: !(search.fuzzy === false || search.fuzzy === "false"),
    synonyms: !(search.synonyms === false || search.synonyms === "false"),
    lemmas: search.lemmas === true || search.lemmas === "true",
    fuzzy_match: search.fuzzy_match === true || search.fuzzy_match === "true",
    mode: topic ? "lexical" : search.mode === "vector" || search.mode === "hybrid" ? search.mode : "lexical",
    ...(POOL_SIZES.includes(Number(search.pool)) && { pool: Number(search.pool) }),
    page: Number.isInteger(page) && page >= 1 && page <= 1_000_000 ? page : 1,
  };
}

/** Selecting a topic starts a lexical search; clearing it leaves the current mode unchanged. */
export function selectSearchTopic(params: SearchParams, topic: string): SearchParams {
  return { ...params, topic: topic || undefined, mode: topic ? "lexical" : params.mode, page: 1 };
}

/** The API path for a search; `page_size` is fixed. */
export function searchPath({ q, topic, source, complete_only, fuzzy, synonyms, lemmas, fuzzy_match, mode, pool, page }: SearchParams, pageSize = 20) {
  const query = new URLSearchParams({ q, page: String(page), page_size: String(pageSize) });
  if (topic) query.set("topic", topic);
  if (source) query.set("source", source);
  if (complete_only) query.set("complete_only", "true");
  if (fuzzy) query.set("fuzzy", "true");
  if (synonyms) query.set("synonyms", "true");
  if (lemmas) query.set("lemmas", "true");
  if (fuzzy_match) query.set("fuzzy_match", "true");
  if (mode !== "lexical") query.set("mode", mode);
  if (pool !== undefined && mode !== "lexical") query.set("pool", String(pool));
  return `/api/search?${query}`;
}
