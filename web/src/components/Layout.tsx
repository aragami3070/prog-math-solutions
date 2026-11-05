import { Link, Outlet } from "@tanstack/react-router";

const FIRST_PAGE = { page: 1, page_size: 20 };

export function Layout() {
  return (
    <div className="shell">
      <aside className="sidebar">
        <Link to="/" search={FIRST_PAGE} className="brand">
          <span className="mark">rk<span>.</span></span>
          <span>Rust<br />Knowledge</span>
        </Link>
        <p className="sidebar-label">РАБОЧАЯ ОБЛАСТЬ</p>
        <nav aria-label="Основная навигация">
          <Link to="/" search={FIRST_PAGE} activeOptions={{ exact: true }} activeProps={{ className: "active" }}>
            <span aria-hidden>▤</span> Собранные страницы
          </Link>
          <Link to="/search" search={{ q: "", source: "", complete_only: false, fuzzy: true, synonyms: true, lemmas: false, mode: "lexical", page: 1 }} activeProps={{ className: "active" }}>
            <span aria-hidden>⌕</span> Поиск
          </Link>
          <Link to="/graph" search={{ document: "" }} activeProps={{ className: "active" }}>
            <span aria-hidden>⋔</span> Связи
          </Link>
          <Link to="/stats" activeProps={{ className: "active" }}>
            <span aria-hidden>▥</span> Статистика
          </Link>
        </nav>
        <div className="sidebar-bottom">
          <span className="dot" /> Локальная коллекция
          <p>Internals & RFC<br />Без поиска и AI — пока только корпус.</p>
          <a href="/swagger-ui/">Документация API ↗</a>
        </div>
      </aside>
      <main id="main-content">
        <div className="topline">
          <span>КОРПУС / RUST</span>
          <span className="phase">Этап 01 · Сбор данных</span>
        </div>
        <Outlet />
      </main>
    </div>
  );
}

export function NotFound() {
  return <div className="notice">Страница не найдена.</div>;
}
