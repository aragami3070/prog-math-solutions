export function number(value: number) {
  return new Intl.NumberFormat("ru-RU").format(value);
}

export function date(value: string | null) {
  return value ? new Date(value).toLocaleString("ru-RU", { dateStyle: "short", timeStyle: "short" }) : "Ещё не собирали";
}

/** "1 источник", "2 источника", "5 источников". */
export function plural(count: number, one: string, few: string, many: string) {
  const tail = count % 100;
  const last = count % 10;
  if (tail >= 11 && tail <= 14) return many;
  if (last === 1) return one;
  if (last >= 2 && last <= 4) return few;
  return many;
}
