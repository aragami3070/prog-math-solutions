import { describe, expect, it, vi, afterEach } from "vitest";
import { pagination, searchParams, searchPath } from "./pagination";
import { download, exportUrl, get, startCrawl } from "./client";

afterEach(() => vi.unstubAllGlobals());

it("exports the whole selected source with explicit completeness and CSV options", () => {
  const url = new URL(exportUrl("zip", "internals", true, true), "http://localhost");
  expect(url.pathname).toBe("/api/sources/export");
  expect(Object.fromEntries(url.searchParams)).toEqual({ format: "zip", source: "internals", complete_only: "true", spreadsheet_safe: "true" });
  const all = new URL(exportUrl("jsonl"), "http://localhost");
  expect(all.searchParams.has("source")).toBe(false);
  expect(all.searchParams.has("page")).toBe(false);
});
describe("pagination", () => {
  it("validates URL values", () => {
    expect(pagination({ page: "2", page_size: "50" })).toEqual({ page: 2, page_size: 50 });
    for (const page of [0, -1, 1.5, Infinity, "bad", 1000001]) expect(pagination({ page }).page).toBe(1);
    expect(pagination({ page_size: 500 }).page_size).toBe(20);
  });
});
it("does not turn API errors into an empty corpus", async () => {
  vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response('{"error":"Database unavailable"}', { status: 503 })));
  await expect(get("/api/stats")).rejects.toThrow("Database unavailable");
});

it("starts a source crawl without a URL batch", async () => {
  const fetch = vi.fn().mockResolvedValue(new Response('{"id":42,"status":"running"}', { status: 202 }));
  vi.stubGlobal("fetch", fetch);
  expect((await startCrawl("internals")).id).toBe(42);
  expect(fetch).toHaveBeenCalledWith("/api/crawls", expect.objectContaining({ method: "POST", body: '{"source":"internals"}' }));
});

it("shows the reason when a source cannot start", async () => {
  vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response('{"error":"Source paused"}', { status: 409 })));
  await expect(startCrawl("rfc")).rejects.toThrow("Source paused");
});

it("reports an export error instead of saving it as a file", async () => {
  vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response('{"error":"Export failed"}', { status: 500 })));
  await expect(download("/api/sources/export?format=zip", "corpus.zip")).rejects.toThrow("Export failed");
});

it("builds a search path with only the filters that are set", () => {
  const params = { q: "async fn", source: "" as const, complete_only: false, fuzzy: false, synonyms: false, lemmas: false, fuzzy_match: false, mode: "lexical" as const, page: 2 };
  const url = new URL(searchPath(params), "http://localhost");
  expect(Object.fromEntries(url.searchParams)).toEqual({ q: "async fn", page: "2", page_size: "20" });
  const filtered = new URL(searchPath({ ...params, source: "rfc", complete_only: true }), "http://localhost");
  expect(filtered.searchParams.get("source")).toBe("rfc");
  expect(filtered.searchParams.get("complete_only")).toBe("true");
  expect(new URL(searchPath({ ...params, mode: "hybrid" }), "http://localhost").searchParams.get("mode")).toBe("hybrid");
  expect(new URL(searchPath({ ...params, fuzzy_match: true }), "http://localhost").searchParams.get("fuzzy_match")).toBe("true");
  const period = new URL(searchPath({ ...params, year_from: 2019, year_to: 2020 }), "http://localhost");
  expect([period.searchParams.get("year_from"), period.searchParams.get("year_to")]).toEqual(["2019", "2020"]);
});

it("validates search parameters from the URL", () => {
  expect(searchParams({ q: "x", source: "evil", complete_only: "true", page: "0" })).toEqual({ q: "x", source: "", complete_only: true, fuzzy: true, synonyms: true, lemmas: false, fuzzy_match: false, mode: "lexical", page: 1 });
  expect(searchParams({ q: "x", mode: "hybrid", lemmas: "true" })).toMatchObject({ mode: "hybrid", lemmas: true });
  expect(searchParams({ q: "x", mode: "evil" }).mode).toBe("lexical");
  expect(searchParams({ q: "x", fuzzy: "false", synonyms: "false" })).toMatchObject({ fuzzy: false, synonyms: false });
  expect(searchParams({ q: "x" }).fuzzy_match).toBe(false);
  expect(searchParams({ q: "x", fuzzy_match: "true" }).fuzzy_match).toBe(true);
  expect(searchParams({ q: "a".repeat(500) }).q).toHaveLength(200);
  expect(searchParams({ q: "x", year_from: "2019", year_to: "2020" })).toMatchObject({ year_from: 2019, year_to: 2020 });
  expect(searchParams({ q: "x", year_from: "1800", year_to: "abc" })).not.toHaveProperty("year_from");
  expect(searchParams({ q: "x", year_to: "abc" })).not.toHaveProperty("year_to");
});
