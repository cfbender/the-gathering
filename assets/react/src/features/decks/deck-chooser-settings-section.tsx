import { useQuery } from "@tanstack/react-query"
import { Dices, SlidersHorizontal } from "lucide-react"
import { getDeckScript } from "@/features/decks/deck-chooser"

/** Settings entry point for the deck chooser script editor. */
export function DeckChooserSettingsSection() {
  const script = useQuery({ queryKey: ["deck-chooser", "script"], queryFn: getDeckScript })
  return (
    <section className="card border-base-300 bg-base-200 border" aria-labelledby="deck-chooser">
      <div className="card-body gap-4">
        <div>
          <h2 id="deck-chooser" className="card-title text-lg">
            <Dices className="size-5" /> Deck chooser
          </h2>
          <p className="text-base-content/60 mt-1 text-sm">
            Decide how “Choose a deck” weighs your decks with a Lua script: give quick losses a
            rematch, favor colors you have not played lately, or change the odds by time of night.
          </p>
        </div>
        <div className="flex flex-wrap items-center justify-between gap-3">
          <p className="text-sm">
            {script.isPending
              ? "Checking your chooser…"
              : script.isError
                ? "Could not load your chooser script."
                : script.data === null
                  ? "Picks use the built-in weights."
                  : "Picks use your saved script."}
          </p>
          <a href="/decks/choose/script" className="btn btn-primary btn-sm">
            <SlidersHorizontal className="size-4" /> Customize the chooser
          </a>
        </div>
      </div>
    </section>
  )
}
