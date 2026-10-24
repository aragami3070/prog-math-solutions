import { number } from "../lib/format";

interface Props {
  page: number;
  totalPages: number;
  onChange: (page: number) => void;
}

export function Pagination({ page, totalPages, onChange }: Props) {
  return (
    <div className="pagination">
      <span>Страница {totalPages === 0 ? 0 : page} / {number(totalPages)}</span>
      <div>
        <button disabled={page === 1} onClick={() => onChange(page - 1)}>← Назад</button>
        <button disabled={page >= totalPages} onClick={() => onChange(page + 1)}>Далее →</button>
      </div>
    </div>
  );
}
