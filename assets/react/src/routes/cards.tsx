import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { createFileRoute } from "@tanstack/react-router"
import { RefreshCw } from "lucide-react"
import { useState } from "react"
import { CardImage } from "@/components/card-image"
import { CardSearch } from "@/components/card-search"
import type { CardSummary } from "@/lib/cards"
import { fetchCatalogStatus, triggerCatalogSync } from "@/lib/cards"

export const Route = createFileRoute("/cards")({
  component: CardsPage,
})

function CardsPage() {
  const queryClient = useQueryClient()
  const [selected, setSelected] = useState<CardSummary | null>(null)
  const status = useQuery({
    queryKey: ["catalog"],
    queryFn: fetchCatalogStatus,
    refetchInterval: (query) => (query.state.data?.status === "running" ? 2_000 : 30_000),
  })
  const sync = useMutation({
    mutationFn: triggerCatalogSync,
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["catalog"] }),
  })

  return (
    <div className="grid gap-8 lg:grid-cols-[minmax(0,1fr)_22rem]">
      <section className="space-y-6">
        <div>
          <p className="text-primary text-sm font-semibold tracking-wide uppercase">Card catalog</p>
          <h1 className="mt-1 text-3xl font-bold tracking-tight">Find a card</h1>
          <p className="text-base-content/70 mt-2 max-w-2xl">
            Search the local Scryfall catalog. Once synced, card lookup works without an internet
            connection.
          </p>
        </div>

        <div className="card bg-base-200 border-base-300 overflow-visible border">
          <div className="card-body overflow-visible">
            <CardSearch label="Card name" value={selected} onChange={setSelected} />
          </div>
        </div>

        {selected ? (
          <article className="card bg-base-200 border-base-300 overflow-hidden border sm:card-side">
            <CardImage
              imageUris={selected.image_uris}
              name={selected.name}
              variant="card"
              className="w-full sm:w-56 sm:rounded-none"
            />
            <div className="card-body">
              <div className="flex items-start justify-between gap-4">
                <h2 className="card-title text-2xl">{selected.name}</h2>
                <span className="font-mono text-sm">{selected.mana_cost}</span>
              </div>
              <p className="text-base-content/70">{selected.type_line}</p>
              <div className="mt-auto flex flex-wrap gap-2 pt-4">
                {selected.can_be_commander && (
                  <span className="badge badge-primary">Commander</span>
                )}
                {selected.commander_pairing && (
                  <span className="badge badge-secondary">
                    {selected.commander_pairing.replaceAll("_", " ")}
                  </span>
                )}
                <span className="badge badge-outline">
                  {selected.color_identity.length ? selected.color_identity.join("") : "Colorless"}
                </span>
              </div>
            </div>
          </article>
        ) : (
          <div className="border-base-300 text-base-content/55 rounded-lg border border-dashed px-6 py-12 text-center">
            Select a card to preview its latest preferred printing.
          </div>
        )}
      </section>

      <aside className="card bg-base-200 border-base-300 h-fit border lg:sticky lg:top-20">
        <div className="card-body gap-4">
          <div>
            <h2 className="card-title text-lg">Catalog status</h2>
            <p className="text-base-content/65 mt-1 text-sm">
              Scryfall default cards, refreshed weekly.
            </p>
          </div>
          {status.isPending && (
            <span className="loading loading-dots loading-sm" aria-label="Loading catalog status" />
          )}
          {status.isError && <p className="text-error text-sm">Could not load catalog status.</p>}
          {status.data && (
            <dl className="grid grid-cols-2 gap-x-4 gap-y-2 text-sm">
              <dt className="text-base-content/60">Status</dt>
              <dd className="text-right font-medium capitalize">{status.data.status}</dd>
              <dt className="text-base-content/60">Cards</dt>
              <dd className="text-right font-mono">{status.data.card_count.toLocaleString()}</dd>
              <dt className="text-base-content/60">Last synced</dt>
              <dd className="text-right">
                {status.data.last_finished_at
                  ? new Date(status.data.last_finished_at).toLocaleDateString()
                  : "Never"}
              </dd>
            </dl>
          )}
          <button
            type="button"
            className="btn btn-primary btn-sm"
            disabled={sync.isPending || status.data?.status === "running"}
            onClick={() => sync.mutate()}
          >
            <RefreshCw
              className={sync.isPending ? "size-4 animate-spin" : "size-4"}
              aria-hidden="true"
            />
            Sync now
          </button>
          {sync.isError && <p className="text-error text-xs">Could not start the sync.</p>}
        </div>
      </aside>
    </div>
  )
}
