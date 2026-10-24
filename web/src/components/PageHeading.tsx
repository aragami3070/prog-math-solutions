interface Props {
  eyebrow: string;
  title: string;
  lead: string;
  onRefresh: () => void;
  refreshing: boolean;
}

export function PageHeading({ eyebrow, title, lead, onRefresh, refreshing }: Props) {
  return (
    <header className="page-heading">
      <div>
        <p className="eyebrow">{eyebrow}</p>
        <h1>{title}<span>.</span></h1>
        <p className="lead">{lead}</p>
      </div>
      <button onClick={onRefresh} disabled={refreshing}>Обновить ↻</button>
    </header>
  );
}
