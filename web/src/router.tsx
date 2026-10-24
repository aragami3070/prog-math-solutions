import { createRootRoute, createRoute, createRouter } from "@tanstack/react-router";
import { Layout, NotFound } from "./components/Layout";
import { DocumentsPage } from "./features/documents/DocumentsPage";
import { StatsPage } from "./features/stats/StatsPage";
import { pagination } from "./lib/pagination";

const rootRoute = createRootRoute({ component: Layout, notFoundComponent: NotFound });
const documentsRoute = createRoute({ getParentRoute: () => rootRoute, path: "/", validateSearch: pagination, component: DocumentsPage });
const statsRoute = createRoute({ getParentRoute: () => rootRoute, path: "/stats", component: StatsPage });

export const router = createRouter({ routeTree: rootRoute.addChildren([documentsRoute, statsRoute]) });

declare module "@tanstack/react-router" {
  interface Register { router: typeof router }
}
