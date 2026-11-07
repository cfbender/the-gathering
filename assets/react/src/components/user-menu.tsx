import { useQuery } from "@tanstack/react-query"
import { Link } from "@tanstack/react-router"
import { ChevronDown, Download, LogOut, Settings, Shield, UserRound } from "lucide-react"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { getPlayers } from "@/features/games/games"
import { useCurrentUser, useLogout } from "@/lib/auth"
import { promptInstall, useCanInstall } from "@/lib/pwa"

export function UserMenu() {
  const session = useCurrentUser()
  const logout = useLogout()
  const canInstall = useCanInstall()
  const players = useQuery({
    queryKey: ["players"],
    queryFn: getPlayers,
    enabled: Boolean(session.data),
  })

  if (!session.data) return null

  const user = session.data
  // The profile is the player linked to this account; unlinked accounts have none.
  const player = players.data?.find((candidate) => candidate.user_id === user.id)

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <button
          type="button"
          className="btn btn-ghost h-11 min-h-11 gap-2 rounded-full px-2 sm:px-3"
          aria-label={`Account menu for ${user.display_name}`}
        >
          {user.avatar_url ? (
            <img
              src={user.avatar_url}
              alt=""
              className="size-7 rounded-full"
              referrerPolicy="no-referrer"
            />
          ) : (
            <span className="bg-primary/15 text-primary grid size-7 place-items-center rounded-full">
              <UserRound className="size-4" aria-hidden="true" />
            </span>
          )}
          <span className="hidden max-w-28 truncate text-sm font-bold sm:inline">
            {user.display_name}
          </span>
          <ChevronDown
            className="text-base-content/60 hidden size-4 sm:inline"
            aria-hidden="true"
          />
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" sideOffset={8} className="glass-menu w-60">
        <DropdownMenuLabel className="min-w-0">
          <span className="text-base-content block truncate text-sm font-black">
            {user.display_name}
          </span>
          <span className="block truncate font-medium" title={`@${user.username}`}>
            @{user.username}
          </span>
        </DropdownMenuLabel>
        <DropdownMenuSeparator />
        {player && (
          <DropdownMenuItem asChild>
            <Link to="/players/$playerId" params={{ playerId: String(player.id) }}>
              <UserRound className="size-4" /> Profile
            </Link>
          </DropdownMenuItem>
        )}
        <DropdownMenuItem asChild>
          <Link to="/settings">
            <Settings className="size-4" /> Settings
          </Link>
        </DropdownMenuItem>
        {user.role === "admin" && (
          <DropdownMenuItem asChild>
            <Link to="/admin/users">
              <Shield className="size-4" /> Admin
            </Link>
          </DropdownMenuItem>
        )}
        {canInstall && (
          <DropdownMenuItem onSelect={() => void promptInstall()}>
            <Download className="size-4" /> Install app
          </DropdownMenuItem>
        )}
        <DropdownMenuSeparator />
        <DropdownMenuItem
          destructive
          disabled={logout.isPending}
          onSelect={() =>
            logout.mutate(undefined, {
              // Dropping the cookie session invalidates the CSRF token in the SPA shell.
              onSuccess: () => window.location.assign("/login?returnTo=%2F"),
            })
          }
        >
          <LogOut className="size-4" /> Sign out
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}
