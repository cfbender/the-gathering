import { useQuery } from "@tanstack/react-query"
import { Link } from "@tanstack/react-router"
import { ChevronDown, Play, Plus, Users, Video } from "lucide-react"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { MAX_PLAYERS, activeTablesQuery, tablePlayerNames, type ActiveTable } from "./rooms"

/** Games-page header actions for the webcam table. With no live table it is one Play button
 * that opens a new room. While a table is live the primary action becomes Join (a menu when
 * several tables are live) and starting another table steps back to a smaller button. */
export function PlayActions() {
  const tables = useQuery(activeTablesQuery)
  const live = tables.data ?? []

  if (live.length === 0) {
    return (
      <Link to="/table/new" className="btn btn-secondary">
        <Play className="size-4" fill="currentColor" /> Play
      </Link>
    )
  }

  const newTable = (
    <Link to="/table/new" className="btn btn-ghost btn-sm">
      <Plus className="size-3.5" /> New table
    </Link>
  )

  if (live.length === 1) {
    const [table] = live as [ActiveTable]
    return (
      <>
        {newTable}
        <Link
          to="/table/$roomId"
          params={{ roomId: table.id }}
          className="btn btn-secondary"
          aria-disabled={table.full || undefined}
          title={tablePlayerNames(table)}
        >
          <Video className="size-4" /> {table.full ? "Table full" : "Join"}
        </Link>
      </>
    )
  }

  return (
    <>
      {newTable}
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <button type="button" className="btn btn-secondary">
            <Video className="size-4" /> Join
            <span className="badge badge-sm badge-neutral">{live.length}</span>
            <ChevronDown className="size-3.5" />
          </button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" className="w-64">
          <DropdownMenuLabel>Live tables</DropdownMenuLabel>
          {live.map((table) => (
            <DropdownMenuItem key={table.id} asChild disabled={table.full}>
              <Link to="/table/$roomId" params={{ roomId: table.id }}>
                <Users className="size-4 shrink-0" />
                <span className="min-w-0 flex-1 truncate">{tablePlayerNames(table)}</span>
                <span className="text-base-content/60 shrink-0 text-xs tabular-nums">
                  {table.full ? "Full" : `${table.players.length}/${MAX_PLAYERS}`}
                </span>
              </Link>
            </DropdownMenuItem>
          ))}
        </DropdownMenuContent>
      </DropdownMenu>
    </>
  )
}
