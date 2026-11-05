import { createRootRoute, createRoute, createRouter } from "@tanstack/react-router";
import { Layout, NotFound } from "./components/Layout";
import { DocumentsPage } from "./features/documents/DocumentsPage";
import { StatsPage } from "./features/stats/StatsPage";
import { GraphPage } from "./features/graph/GraphPage";
import { SearchPage } from "./features/search/SearchPage";
import { pagination, searchParams } from "./lib/pagination";

const rootRoute = createRootRoute({ component: Layout, notFoundComponent: NotFound });
const documentsRoute = createRoute({ getParentRoute: () => rootRoute, path: "/", validateSearch: pagination, component: DocumentsPage });
const searchRoute = createRoute({ getParentRoute: () => rootRoute, path: "/search", validateSearch: searchParams, component: SearchPage });
const graphRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/graph",
  validateSearch: (search: Record<string, unknown>) => ({ document: typeof search.document === "string" ? search.document.slice(0, 40) : "" }),
  component: GraphPage,
});
const statsRoute = createRoute({ getParentRoute: () => rootRoute, path: "/stats", component: StatsPage });

export const router = createRouter({ routeTree: rootRoute.addChildren([documentsRoute, searchRoute, graphRoute, statsRoute]) });

declare module "@tanstack/react-router" {
  interface Register { router: typeof router }
}
