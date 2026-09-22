import { useQuery } from "@tanstack/react-query"
import { createFileRoute, type SearchSchemaInput } from "@tanstack/react-router"
import { PageHeader } from "@/components/app-shell"
import { GameForm } from "@/features/games/game-form"
import type { DiscordResultDraft } from "@/features/games/use-game-draft"
import { api } from "@/lib/api"

export const Route = createFileRoute("/games/new")({
  validateSearch: (search: Record<string, unknown> & SearchSchemaInput): { discord?: string } => ({
    discord: typeof search.discord === "string" ? search.discord : undefined,
  }),
  component: NewGamePage,
})

function NewGamePage() {
  const { discord } = Route.useSearch()
  const draftQuery = useQuery({
    queryKey: ["discord-result-draft", discord],
    queryFn: () =>
      api<{ data: DiscordResultDraft }>(`/api/discord/result-drafts/${discord}`).then(
        (body) => body.data,
      ),
    enabled: Boolean(discord),
    retry: false,
  })

  return (
    <div className="mx-auto flex max-w-3xl flex-col gap-6">
      <PageHeader
        eyebrow="Fresh from the table"
        title="Log a game"
        description="Start with the winner, add everyone else, and you’re done."
      />
      {discord && draftQuery.isPending && (
        <div className="skeleton h-40 w-full" aria-label="Loading game" />
      )}
      {discord && draftQuery.isError && (
        <div role="alert" className="alert alert-error">
          This game link has expired or is unavailable. Run /log again in Discord to create a new
          link.
        </div>
      )}
      {(!discord || draftQuery.data) && <GameForm discordDraft={draftQuery.data} />}
    </div>
  )
}
