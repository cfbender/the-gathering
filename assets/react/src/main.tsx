import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { RouterProvider, createRouter } from "@tanstack/react-router"
import { StrictMode } from "react"
import { createRoot } from "react-dom/client"
import "./app.css"
import { RouteError } from "./components/route-error"
import { ToastProvider } from "./components/ui/toast"
import { installStaleBundleReload } from "./lib/stale-bundle"
import { StatsRangeProvider } from "./lib/stats-range"
import { ThemeProvider } from "./lib/theme"
import { routeTree } from "./routeTree.gen"

installStaleBundleReload()

const queryClient = new QueryClient({
  defaultOptions: { queries: { staleTime: 30_000, retry: 1 } },
})
const router = createRouter({
  routeTree,
  scrollRestoration: true,
  context: { queryClient },
  defaultErrorComponent: RouteError,
})

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router
  }
}

const rootElement = document.getElementById("root")
if (!rootElement) throw new Error("Missing #root element in the SPA shell")

createRoot(rootElement).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <ThemeProvider>
        <ToastProvider>
          <StatsRangeProvider>
            <RouterProvider router={router} />
          </StatsRangeProvider>
        </ToastProvider>
      </ThemeProvider>
    </QueryClientProvider>
  </StrictMode>,
)
