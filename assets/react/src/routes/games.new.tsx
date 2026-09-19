import { createFileRoute } from "@tanstack/react-router"
import { GameForm } from "@/components/game-form"

export const Route = createFileRoute("/games/new")({
  component: NewGamePage,
})

function NewGamePage() {
  return (
    <div className="mx-auto flex max-w-3xl flex-col gap-6">
      <div>
        <p className="text-primary text-sm font-semibold uppercase">Fresh from the table</p>
        <h1 className="text-3xl font-bold tracking-tight">Log a game</h1>
        <p className="text-base-content/70 mt-1">
          Start with the winner, add everyone else, and you’re done.
        </p>
      </div>
      <GameForm />
    </div>
  )
}
