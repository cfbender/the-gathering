import { useQuery } from "@tanstack/react-query"
import { Link, createFileRoute } from "@tanstack/react-router"
import { EmptyPanel, PageHeader } from "@/components/app-shell"
import { CalendarDays, Plus } from "lucide-react"
import { useState } from "react"
import { getGames } from "@/features/games/games"
import { GameCard } from "@/features/games/game-card"
import { GameTable } from "@/features/games/game-table"
import { GameViewToggle, type GameView } from "@/features/games/game-view-toggle"
import { PlayActions } from "@/features/webcam-table/play-actions"
import {
  parseGamesSearch,
  patchGamesSearch,
  toGameFilters,
  type GameFilters,
} from "@/features/games/game-filters"
import { GameFiltersPanel } from "@/features/games/game-filters-panel"

export const Route = createFileRoute("/games/")({
  validateSearch: parseGamesSearch,
  component: GamesPage,
})

function GamesPage() {
  // Filters live in the URL so other pages can link to a filtered list.
  const search = Route.useSearch()
  const navigate = Route.useNavigate()
  const filters = toGameFilters(search)
  const page = search.page ?? 1
  const [view, setView] = useState<GameView>("cards")

  function update(patch: Partial<GameFilters>) {
    void navigate({ search: patchGamesSearch(search, patch), replace: true })
  }

  function clear() {
    void navigate({ search: {}, replace: true })
  }

  function goToPage(next: number) {
    void navigate({ search: { ...search, page: next > 1 ? next : undefined } })
  }

  const params = { ...filters, page }
  const games = useQuery({ queryKey: ["games", params], queryFn: () => getGames(params) })

  return (
    <div className="flex flex-col gap-6">
      <PageHeader
        eyebrow="Playgroup history"
        title="Games"
        actions={
          <>
            <PlayActions />
            <Link to="/games/new" className="btn btn-primary">
              <Plus className="size-4" /> Log game
            </Link>
          </>
        }
      />

      <GameFiltersPanel
        search={search}
        total={games.data?.pagination.total}
        onChange={update}
        onClear={clear}
      />

      <div className="flex flex-wrap items-center justify-between gap-3">
        <p className="text-base-content/65 text-sm">
          {games.data &&
            `${games.data.pagination.total} ${games.data.pagination.total === 1 ? "game" : "games"}`}
        </p>
        <GameViewToggle value={view} onChange={setView} />
      </div>

      {games.isPending && (
        <div className="flex justify-center py-16">
          <span className="loading loading-spinner loading-lg" />
        </div>
      )}
      {games.isError && <div className="alert alert-error">Could not load games.</div>}
      {games.data?.data.length === 0 && (
        <EmptyPanel
          icon={<CalendarDays className="size-10" />}
          title="No games at this table yet"
          description="Log the first game, or clear your filters to see more history."
          action={
            <Link to="/games/new" className="btn btn-primary">
              Log the first game
            </Link>
          }
        />
      )}
      {games.data &&
        games.data.data.length > 0 &&
        (view === "table" ? (
          <GameTable games={games.data.data} />
        ) : (
          <div className="grid gap-4 lg:grid-cols-2">
            {games.data.data.map((game) => (
              <GameCard key={game.id} game={game} />
            ))}
          </div>
        ))}
      {games.data && games.data.pagination.total_pages > 1 && (
        <div className="join self-center">
          <button className="btn join-item" disabled={page <= 1} onClick={() => goToPage(page - 1)}>
            Previous
          </button>
          <span className="btn join-item pointer-events-none">
            {page} / {games.data.pagination.total_pages}
          </span>
          <button
            className="btn join-item"
            disabled={page >= games.data.pagination.total_pages}
            onClick={() => goToPage(page + 1)}
          >
            Next
          </button>
        </div>
      )}
    </div>
  )
}
