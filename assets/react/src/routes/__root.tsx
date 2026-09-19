import type { QueryClient } from "@tanstack/react-query"
import { Link, Outlet, createRootRouteWithContext } from "@tanstack/react-router"
import { Swords, Upload } from "lucide-react"
import { ThemeToggle } from "@/components/theme-toggle"
import { UserMenu } from "@/components/user-menu"
import { requireUser, useCurrentUser } from "@/lib/auth"

/** Router context available to every route's `loader` and `beforeLoad`. */
export interface RouterContext {
  queryClient: QueryClient
}

/** Only the sign-in and sign-up pages are reachable without a session. */
const publicPaths = new Set(["/login", "/register"])

export const Route = createRootRouteWithContext<RouterContext>()({
  beforeLoad: async ({ context, location }) => {
    if (!publicPaths.has(location.pathname)) {
      await requireUser(context.queryClient, location.href)
    }
  },
  component: RootLayout,
  notFoundComponent: NotFound,
})

function RootLayout() {
  const session = useCurrentUser()

  return (
    <div className="flex min-h-dvh flex-col">
      <header className="border-base-300 bg-base-100/80 sticky top-0 z-10 border-b backdrop-blur">
        <div className="mx-auto flex h-14 w-full max-w-6xl items-center justify-between gap-2 px-4 sm:gap-4">
          <Link to="/" className="flex items-center gap-2 font-semibold tracking-tight">
            <span className="bg-primary text-primary-content grid size-8 place-items-center rounded-lg">
              <Swords className="size-4" aria-hidden="true" />
            </span>
            <span className="hidden sm:inline">The Gathering</span>
          </Link>
          <div className="flex items-center gap-1 sm:gap-3">
            {/* Signed-out visitors only ever see the login/register pages, where navigation is noise. */}
            {session.data && (
              <>
                <nav aria-label="Main navigation" className="flex items-center">
                  {[
                    ["/games", "Games"],
                    ["/players", "Players"],
                    ["/decks", "Decks"],
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
                  {session.data.role === "admin" && (
                    <Link
                      to="/import"
                      aria-label="Import games"
                      className="btn btn-ghost btn-sm px-2 sm:px-3"
                      activeProps={{ className: "text-primary bg-primary/10" }}
                    >
                      <Upload className="size-4 lg:hidden" aria-hidden="true" />
                      <span className="hidden lg:inline">Import</span>
                    </Link>
                  )}
                </nav>
                <UserMenu />
              </>
            )}
            <div className={session.data ? "hidden sm:block" : undefined}>
              <ThemeToggle />
            </div>
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
