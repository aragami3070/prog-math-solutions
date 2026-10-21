import { describe, expect, it, vi, afterEach } from "vitest";
import { exportUrl, get, pagination, startCrawl } from "./api";

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
