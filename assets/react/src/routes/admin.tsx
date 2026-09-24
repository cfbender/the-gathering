import { Link, Outlet, createFileRoute } from "@tanstack/react-router"
import { requireAdmin } from "@/lib/auth"

export const Route = createFileRoute("/admin")({
  beforeLoad: ({ context, location }) => requireAdmin(context.queryClient, location.href),
  component: AdminLayout,
})

function AdminLayout() {
  return (
    <div className="flex flex-col gap-6">
      <nav className="tabs tabs-box w-fit max-w-full" aria-label="Administration sections">
        <Link to="/admin/users" className="tab" activeProps={{ className: "tab-active" }}>
          Users
        </Link>
        <Link to="/admin/players" className="tab" activeProps={{ className: "tab-active" }}>
          Player identities
        </Link>
        <Link to="/admin/settings" className="tab" activeProps={{ className: "tab-active" }}>
          Settings
        </Link>
        <Link to="/admin/catalog" className="tab" activeProps={{ className: "tab-active" }}>
          Catalog
        </Link>
        <Link to="/admin/discord" className="tab" activeProps={{ className: "tab-active" }}>
          Discord
        </Link>
      </nav>
      <Outlet />
    </div>
  )
}
