import type { QueryClient } from "@tanstack/react-query"
import { Link, Outlet, createRootRouteWithContext } from "@tanstack/react-router"
import { Swords } from "lucide-react"
import { ThemeToggle } from "@/components/theme-toggle"

/** Router context available to every route's `loader` and `beforeLoad`. */
export interface RouterContext {
  queryClient: QueryClient
}

export const Route = createRootRouteWithContext<RouterContext>()({
  component: RootLayout,
  notFoundComponent: NotFound,
})

function RootLayout() {
  return (
    <div className="flex min-h-dvh flex-col">
      <header className="border-base-300 bg-base-100/80 sticky top-0 z-10 border-b backdrop-blur">
        <div className="mx-auto flex h-14 w-full max-w-6xl items-center justify-between gap-4 px-4">
          <Link to="/" className="flex items-center gap-2 font-semibold tracking-tight">
            <span className="bg-primary text-primary-content grid size-8 place-items-center rounded-lg">
              <Swords className="size-4" aria-hidden="true" />
            </span>
            The Gathering
          </Link>
          <div className="flex items-center gap-1 sm:gap-3">
            <nav aria-label="Main navigation" className="flex items-center">
              {[
                ["/games", "Games"],
                ["/players", "Players"],
                ["/decks", "Decks"],
                ["/cards", "Cards"],
              ].map(([to, label]) => (
                <Link
                  key={to}
                  to={to}
                  className="btn btn-ghost btn-sm px-2 sm:px-3"
                  activeProps={{ className: "text-primary bg-primary/10" }}
                >
                  {label}
                </Link>
              ))}
            </nav>
            <ThemeToggle />
          </div>
        </div>
      </header>
      <main className="mx-auto w-full max-w-6xl flex-1 px-4 py-8">
        <Outlet />
      </main>
    </div>
  )
}

function NotFound() {
  return (
    <section className="mx-auto max-w-md py-16 text-center">
      <p className="text-base-content/60 font-mono text-sm">404</p>
      <h1 className="mt-2 text-2xl font-bold tracking-tight">Nothing on this table</h1>
      <p className="text-base-content/70 mt-2">That page does not exist.</p>
      <Link to="/" className="btn btn-primary mt-6">
        Back to the games
      </Link>
    </section>
  )
}
