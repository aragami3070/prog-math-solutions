const LABELS: Record<string, string> = {
  complete: "Полный документ",
  partial: "Часть темы (не все страницы)",
  unknown: "Полнота неизвестна",
  running: "В процессе",
  completed: "Завершён",
  failed: "С ошибками",
  blocked: "Источник остановлен",
  interrupted: "Прерван",
};

export function Badge({ value }: { value: string }) {
  return <span className={`badge badge-${value}`}>{LABELS[value] ?? value}</span>;
}
