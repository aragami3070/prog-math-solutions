import type { SourceId } from "./types";

export const SOURCES: { id: SourceId; name: string }[] = [
  { id: "rfc", name: "Rust RFC" },
  { id: "internals", name: "Rust Internals" },
];

export function sourceName(source: SourceId) {
  return SOURCES.find(item => item.id === source)?.name ?? source;
}
