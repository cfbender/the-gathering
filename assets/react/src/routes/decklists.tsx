import { createFileRoute } from "@tanstack/react-router"
import { useState } from "react"
import { DecklistLink } from "@/components/decklist-link"
import { DecklistUrlField } from "@/components/decklist-url-field"
import type { Decklist } from "@/lib/decklists"

export const Route = createFileRoute("/decklists")({
  component: DecklistsDemo,
})

function DecklistsDemo() {
  const [url, setUrl] = useState("")
  const [selected, setSelected] = useState<Decklist | null>(null)

  return (
    <div className="mx-auto flex max-w-2xl flex-col gap-8">
      <header className="flex flex-col gap-2">
        <p className="text-primary text-sm font-semibold tracking-wide uppercase">
          Integration demo
        </p>
        <h1 className="text-3xl font-bold tracking-tight">Deck-list resolver</h1>
        <p className="text-base-content/70">
          Paste a public Moxfield, Archidekt, or ManaVault deck link to preview its metadata.
        </p>
      </header>

      <section className="card bg-base-200 border-base-300 border">
        <div className="card-body">
          <DecklistUrlField value={url} onChange={setUrl} onResolved={setSelected} />
        </div>
      </section>

      {selected && (
        <section
          className="card bg-base-200 border-base-300 border"
          aria-labelledby="result-heading"
        >
          <div className="card-body gap-3">
            <div className="flex items-start justify-between gap-4">
              <div>
                <h2 id="result-heading" className="card-title">
                  {selected.name}
                </h2>
                <p className="text-base-content/70 text-sm">
                  {selected.commanders.map((commander) => commander.name).join(" + ")}
                </p>
              </div>
              <DecklistLink url={selected.url} />
            </div>
            <dl className="grid grid-cols-2 gap-3 text-sm sm:grid-cols-4">
              <Result label="Source" value={selected.source} />
              <Result label="Author" value={selected.author ?? "Not provided"} />
              <Result label="Colors" value={selected.color_identity?.join("") || "Colorless"} />
              <Result label="Cards" value={String(selected.card_count ?? "Unknown")} />
            </dl>
          </div>
        </section>
      )}
    </div>
  )
}

function Result({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <dt className="text-base-content/60">{label}</dt>
      <dd className="font-medium capitalize">{value}</dd>
    </div>
  )
}
