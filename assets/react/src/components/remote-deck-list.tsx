import { useQuery } from "@tanstack/react-query"
import { Link } from "@tanstack/react-router"
import { ExternalLink, Library } from "lucide-react"
import { remoteDecksQueryOptions, remoteDeckSourceLabels } from "@/lib/remote-decks"

export function RemoteDeckList() {
  const query = useQuery(remoteDecksQueryOptions)

  if (query.isPending) return <span className="loading loading-spinner loading-sm" />
  if (query.isError) {
    return <div className="alert alert-error">Your remote decks could not be loaded.</div>
  }

  const configured = query.data.sources.some((source) => source.configured)
  const errors = query.data.sources.filter((source) => source.error)

  return (
    <div className="space-y-3">
      {errors.map((source) => (
        <div key={source.source} className="alert alert-warning py-2 text-sm">
          <span>
            <strong>{remoteDeckSourceLabels[source.source]}:</strong> {source.error}
          </span>
        </div>
      ))}
      {query.data.decks.length === 0 ? (
        <div className="card border-base-300 bg-base-100 border p-6 text-center">
          <Library className="text-primary mx-auto size-7" />
          <p className="mt-2 font-semibold">
            {configured ? "No public decks found" : "Connect your deck hosts"}
          </p>
          <p className="text-base-content/60 mt-1 text-sm">
            {configured
              ? "Only public decks can be listed."
              : "Add your Moxfield or Archidekt username in Settings."}
          </p>
          {!configured && (
            <Link to="/settings" className="btn btn-primary btn-sm mx-auto mt-3">
              Open Settings
            </Link>
          )}
        </div>
      ) : (
        <div className="grid gap-3 sm:grid-cols-2">
          {query.data.decks.map((deck) => (
            <a
              key={`${deck.source}:${deck.url}`}
              href={deck.url}
              target="_blank"
              rel="noreferrer"
              className="card border-base-300 bg-base-200 hover:border-primary/40 border transition-colors"
            >
              <div className="card-body gap-2 p-4">
                <div className="flex items-start justify-between gap-3">
                  <strong>{deck.name}</strong>
                  <ExternalLink className="text-base-content/50 size-4 shrink-0" />
                </div>
                <span className="text-base-content/70 text-sm">
                  {deck.commanders.join(" + ") || "No commander listed"}
                </span>
                <div className="flex flex-wrap items-center gap-2">
                  <span className="badge badge-outline badge-sm">
                    {remoteDeckSourceLabels[deck.source]}
                  </span>
                  {deck.color_identity.length > 0 && (
                    <span className="badge badge-ghost badge-sm font-mono">
                      {deck.color_identity.join("")}
                    </span>
                  )}
                </div>
              </div>
            </a>
          ))}
        </div>
      )}
    </div>
  )
}
