import { Link } from "@tanstack/react-router"
import { LogIn, LogOut, Settings, Shield, UserRound } from "lucide-react"
import { useCurrentUser, useLogout } from "@/lib/auth"

export function UserMenu() {
  const session = useCurrentUser()
  const logout = useLogout()

  if (!session.data) {
    return (
      <Link
        to="/login"
        search={{ returnTo: "/", error: undefined }}
        className="btn btn-ghost btn-sm gap-2"
      >
        <LogIn className="size-4" aria-hidden="true" />
        <span className="hidden sm:inline">Sign in</span>
      </Link>
    )
  }

  const user = session.data

  return (
    <div className="dropdown dropdown-end">
      <button type="button" tabIndex={0} className="btn btn-ghost btn-sm gap-2">
        {user.avatar_url ? (
          <img
            src={user.avatar_url}
            alt=""
            className="size-6 rounded-full"
            referrerPolicy="no-referrer"
          />
        ) : (
          <UserRound className="size-4" aria-hidden="true" />
        )}
        <span className="max-w-28 truncate">{user.display_name}</span>
      </button>
      <ul
        tabIndex={-1}
        className="menu dropdown-content bg-base-200 border-base-300 z-20 mt-2 w-52 rounded-box border p-2 shadow-lg"
      >
        <li className="menu-title truncate px-3 py-2">@{user.username}</li>
        <li>
          <Link to="/settings">
            <Settings className="size-4" /> Settings
          </Link>
        </li>
        {user.role === "admin" && (
          <li>
            <Link to="/admin/users">
              <Shield className="size-4" /> Admin
            </Link>
          </li>
        )}
        <li>
          <button
            type="button"
            disabled={logout.isPending}
            onClick={() =>
              logout.mutate(undefined, {
                // Dropping the cookie session invalidates the CSRF token in the SPA shell.
                onSuccess: () => window.location.assign("/login?returnTo=%2F"),
              })
            }
          >
            <LogOut className="size-4" /> Sign out
          </button>
        </li>
      </ul>
    </div>
  )
}
