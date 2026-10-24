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
