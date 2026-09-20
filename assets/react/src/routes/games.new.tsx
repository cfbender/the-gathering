import { createFileRoute } from "@tanstack/react-router"
import { PageHeader } from "@/components/app-shell"
import { GameForm } from "@/components/game-form"

export const Route = createFileRoute("/games/new")({
  component: NewGamePage,
})

function NewGamePage() {
  return (
    <div className="mx-auto flex max-w-3xl flex-col gap-6">
      <PageHeader
        eyebrow="Fresh from the table"
        title="Log a game"
        description="Start with the winner, add everyone else, and you’re done."
      />
      <GameForm />
    </div>
  )
}
