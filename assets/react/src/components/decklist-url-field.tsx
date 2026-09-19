import { Check, Link as LinkIcon, LoaderCircle } from "lucide-react"
import { useEffect, useRef, useState } from "react"
import { ApiError } from "@/lib/api"
import {
  decklistSourceLabels,
  detectDecklistSource,
  useResolveDecklist,
  type Decklist,
} from "@/lib/decklists"

export interface DecklistUrlFieldProps {
  value: string
  onChange: (url: string) => void
  onResolved?: (decklist: Decklist) => void
  label?: string
}

export function DecklistUrlField({
  value,
  onChange,
  onResolved,
  label = "Deck-list URL",
}: DecklistUrlFieldProps) {
  const mutation = useResolveDecklist()
  const [preview, setPreview] = useState<Decklist | null>(null)
  const lastRequested = useRef("")
  const currentValue = useRef(value)
  currentValue.current = value
  const source = detectDecklistSource(value)

  const resolveUrl = (url: string) => {
    const trimmed = url.trim()
    if (!trimmed || trimmed === lastRequested.current) return
    lastRequested.current = trimmed
    setPreview(null)
    mutation.mutate(trimmed, {
      onSuccess: (decklist) => {
        if (currentValue.current.trim() === trimmed) setPreview(decklist)
      },
      onError: () => {
        lastRequested.current = ""
      },
    })
  }

  useEffect(() => {
    if (!value || !source || source === "other") return
    const timer = window.setTimeout(() => resolveUrl(value), 600)
    return () => window.clearTimeout(timer)
  }, [value, source])

  const error = mutation.error
  const errorMessage =
    error instanceof ApiError
      ? (error.fieldErrors("url")[0] ??
        (error.status === 404
          ? "That deck is private or could not be found."
          : error.status === 502
            ? "The deck service could not be reached. Try again shortly."
            : "Could not resolve this deck link."))
      : error
        ? "Could not resolve this deck link."
        : null

  return (
    <div className="flex flex-col gap-2">
      <label className="form-control flex flex-col gap-1.5">
        <span className="text-sm font-medium">{label}</span>
        <div className="relative">
          <input
            type="url"
            className="input input-bordered w-full pr-32"
            value={value}
            placeholder="https://moxfield.com/decks/…"
            aria-invalid={errorMessage ? "true" : undefined}
            aria-describedby={errorMessage ? "decklist-url-error" : undefined}
            onChange={(event) => {
              lastRequested.current = ""
              mutation.reset()
              setPreview(null)
              onChange(event.target.value)
            }}
            onBlur={() => resolveUrl(value)}
            onPaste={(event) => {
              const pasted = event.clipboardData.getData("text")
              window.setTimeout(() => resolveUrl(pasted), 0)
            }}
          />
          {source && (
            <span className="badge badge-ghost absolute top-1/2 right-3 -translate-y-1/2 gap-1">
              <LinkIcon className="size-3" aria-hidden="true" />
              {decklistSourceLabels[source]}
            </span>
          )}
        </div>
      </label>

      {mutation.isPending && (
        <p className="text-base-content/60 flex items-center gap-1.5 text-sm" role="status">
          <LoaderCircle className="size-3.5 animate-spin" aria-hidden="true" /> Resolving deck…
        </p>
      )}
      {errorMessage && (
        <p id="decklist-url-error" className="text-error text-sm" role="alert">
          {errorMessage}
        </p>
      )}

      {preview && (
        <div className="bg-base-200 border-base-300 flex items-start gap-3 rounded-box border p-3">
          <div className="min-w-0 flex-1">
            <p className="truncate font-semibold">{preview.name}</p>
            <p className="text-base-content/70 text-sm">
              {preview.commanders.map((commander) => commander.name).join(" + ") || "No commander"}
              {preview.author ? ` · by ${preview.author}` : ""}
            </p>
          </div>
          {onResolved && (
            <button
              type="button"
              className="btn btn-primary btn-sm shrink-0"
              onClick={() => onResolved(preview)}
            >
              <Check className="size-4" aria-hidden="true" /> Use these details
            </button>
          )}
        </div>
      )}
    </div>
  )
}
