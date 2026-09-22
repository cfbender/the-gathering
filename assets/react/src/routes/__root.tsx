import type { QueryClient } from "@tanstack/react-query"
import { Link, Outlet, createRootRouteWithContext, useLocation } from "@tanstack/react-router"
import { Crown, Gamepad2, Layers, Menu, Palette, Upload, Users } from "lucide-react"
import { useEffect, useState } from "react"
import { ThemeToggle } from "@/components/theme-toggle"
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover"
import { UserMenu } from "@/components/user-menu"
import { requireUser, useCurrentUser } from "@/lib/auth"
import { cn } from "@/lib/cn"

/** Router context available to every route's `loader` and `beforeLoad`. */
export interface RouterContext {
  queryClient: QueryClient
}

/** Sign-in, bootstrap, and invitations are reachable without a session. */
const publicPaths = new Set(["/login", "/register", "/invite"])

export const Route = createRootRouteWithContext<RouterContext>()({
  beforeLoad: async ({ context, location }) => {
    if (!publicPaths.has(location.pathname)) {
      await requireUser(context.queryClient, location.href)
    }
  },
  component: RootLayout,
  notFoundComponent: NotFound,
})

const navItems = [
  { to: "/games" as const, label: "Games", icon: Gamepad2, adminOnly: false },
  { to: "/players" as const, label: "Players", icon: Users, adminOnly: false },
  { to: "/decks" as const, label: "Decks", icon: Layers, adminOnly: false },
  { to: "/commanders" as const, label: "Commanders", icon: Crown, adminOnly: false },
  { to: "/colors" as const, label: "Colors", icon: Palette, adminOnly: false },
  { to: "/import" as const, label: "Import", icon: Upload, adminOnly: true },
]

function navItemActive(pathname: string, to: (typeof navItems)[number]["to"]) {
  return pathname === to || pathname.startsWith(`${to}/`)
}

function RootLayout() {
  const session = useCurrentUser()
  const { pathname } = useLocation()
  const [mobileMenuOpen, setMobileMenuOpen] = useState(false)

  useEffect(() => {
    setMobileMenuOpen(false)
  }, [pathname])

  const user = session.data
  const items = navItems.filter((item) => !item.adminOnly || user?.role === "admin")

  if (pathname.startsWith("/table/")) {
    return (
      <main className="bg-neutral text-neutral-content min-h-dvh">
        <Outlet />
      </main>
    )
  }

  return (
    <div className="app-shell-root bg-base-100 text-base-content flex min-h-dvh flex-col">
      <header className="app-shell-header bg-base-100/95 sticky top-0 z-30 backdrop-blur">
        <div className="navbar mx-auto min-h-16 w-full max-w-7xl px-0">
          <Link
            to="/"
            className="flex min-h-11 min-w-11 items-center gap-3 text-2xl font-black tracking-normal"
          >
            <img src="/images/logo.svg" alt="" className="size-7 shrink-0" width={28} height={28} />
            <span className="hidden truncate sm:inline">The Gathering</span>
          </Link>

          {/* Signed-out visitors only ever see the login/register pages, where navigation is noise. */}
          {user ? (
            <>
              <nav
                aria-label="Main navigation"
                className="ml-auto hidden items-center gap-1 lg:flex"
              >
                {items.map((item) => (
                  <Link
                    key={item.to}
                    to={item.to}
                    className={cn(
                      "focus-visible:ring-primary/35 rounded-full px-3.5 py-2 text-sm leading-5 font-bold transition-colors focus-visible:ring-2 focus-visible:outline-none",
                      navItemActive(pathname, item.to)
                        ? "text-primary-content bg-[color-mix(in_oklch,var(--color-primary),var(--color-base-100)_18%)]"
                        : "text-base-content hover:text-primary",
                    )}
                  >
                    {item.label}
                  </Link>
                ))}
              </nav>

              <div className="ml-2 hidden lg:block">
                <ThemeToggle />
              </div>
              <div className="ml-auto lg:ml-2">
                <UserMenu />
              </div>

              <div className="ml-1 lg:hidden">
                <Popover open={mobileMenuOpen} onOpenChange={setMobileMenuOpen}>
                  <PopoverTrigger asChild>
                    <button
                      className="btn btn-ghost btn-square h-11 min-h-11 w-11"
                      type="button"
                      aria-label={mobileMenuOpen ? "Close navigation" : "Open navigation"}
                    >
                      <Menu className="size-7" />
                    </button>
                  </PopoverTrigger>
                  <PopoverContent align="end" sideOffset={12} className="glass-menu w-64 p-3">
                    <nav className="grid gap-1" aria-label="Site">
                      {items.map((item) => (
                        <Link
                          key={item.to}
                          to={item.to}
                          activeProps={{ className: "bg-base-200 text-primary" }}
                          className="btn btn-ghost justify-start"
                          onClick={() => setMobileMenuOpen(false)}
                        >
                          <item.icon className="size-4" />
                          {item.label}
                        </Link>
                      ))}
                    </nav>
                    <div className="border-base-300 mt-3 flex items-center justify-end border-t pt-3">
                      <ThemeToggle onSelect={() => setMobileMenuOpen(false)} />
                    </div>
                  </PopoverContent>
                </Popover>
              </div>
            </>
          ) : (
            <div className="ml-auto">
              <ThemeToggle />
            </div>
          )}
        </div>
      </header>
      <main className="app-shell-main flex-1">
        <div className="mx-auto w-full max-w-7xl py-8 sm:py-10">
          <Outlet />
        </div>
      </main>
    </div>
  )
}

function NotFound() {
  return (
    <section className="card border-base-300 bg-base-200 mx-auto max-w-md border p-10 text-center">
      <p className="text-base-content/60 font-mono text-sm">404</p>
      <h1 className="mt-2 text-2xl font-black tracking-normal">Nothing on this table</h1>
      <p className="text-base-content/70 mt-2">That page does not exist.</p>
      <Link to="/" className="btn btn-primary mt-6">
        Back to the games
      </Link>
    </section>
  )
}
