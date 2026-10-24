export function ErrorPanel({ error, retry }: { error: Error; retry: () => void }) {
  return (
    <div className="notice error" role="alert">
      <strong>Не удалось получить данные</strong>
      <p>{error.message}</p>
      <button onClick={retry}>Повторить</button>
    </div>
  );
}

export function Loading() {
  return <div className="notice" role="status">Загружаем данные корпуса…</div>;
}
