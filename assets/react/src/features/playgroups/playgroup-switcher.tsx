import { useQuery } from "@tanstack/react-query"
import { Link } from "@tanstack/react-router"
import { ChevronDown, Settings2, UsersRound } from "lucide-react"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { playgroupsQueryOptions, useSwitchPlaygroup } from "@/features/playgroups/playgroups"
import { useCurrentUser } from "@/lib/auth"
import { cn } from "@/lib/cn"

/** Header control naming the active playgroup; every list and stat on the page belongs to it. */
export function PlaygroupSwitcher({ className }: { className?: string }) {
  const session = useCurrentUser()
  const playgroups = useQuery({ ...playgroupsQueryOptions, enabled: Boolean(session.data) })
  const switchPlaygroup = useSwitchPlaygroup()

  const user = session.data
  if (!user) return null

  const active = playgroups.data?.find((playgroup) => playgroup.id === user.playgroup_id)
  const name = active?.name ?? (user.playgroup_id === null ? "No playgroup" : "Playgroup")

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <button
          type="button"
          className={cn(
            "btn btn-ghost h-11 min-h-11 min-w-0 max-w-44 shrink gap-1.5 rounded-full px-2.5 sm:max-w-56 sm:px-3",
            className,
          )}
          aria-label={`Playgroup: ${name}`}
          disabled={switchPlaygroup.isPending}
        >
          <UsersRound className="text-primary size-4 shrink-0" aria-hidden="true" />
          <span className="truncate text-sm font-bold">{name}</span>
          <ChevronDown className="text-base-content/60 size-4 shrink-0" aria-hidden="true" />
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" sideOffset={8} className="glass-menu w-60">
        <DropdownMenuLabel>Playgroup</DropdownMenuLabel>
        <DropdownMenuRadioGroup
          value={user.playgroup_id === null ? "" : String(user.playgroup_id)}
          onValueChange={(value) => {
            const id = Number(value)
            if (id !== user.playgroup_id) switchPlaygroup.mutate(id)
          }}
        >
          {(playgroups.data ?? []).map((playgroup) => (
            <DropdownMenuRadioItem key={playgroup.id} value={String(playgroup.id)}>
              <span className="truncate">{playgroup.name}</span>
            </DropdownMenuRadioItem>
          ))}
        </DropdownMenuRadioGroup>
        {playgroups.data?.length === 0 && (
          <p className="text-base-content/70 px-3 py-2 text-sm">
            You are not in a playgroup yet. Ask an administrator for an invite link.
          </p>
        )}
        {switchPlaygroup.error && (
          <p role="alert" className="text-error px-3 py-2 text-sm">
            Could not switch playgroup.
          </p>
        )}
        {user.role === "admin" && (
          <>
            <DropdownMenuSeparator />
            <DropdownMenuItem asChild>
              <Link to="/admin/playgroups">
                <Settings2 className="size-4" /> Manage playgroups
              </Link>
            </DropdownMenuItem>
          </>
        )}
      </DropdownMenuContent>
    </DropdownMenu>
  )
}
