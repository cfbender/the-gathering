import { useQuery } from "@tanstack/react-query"
import { LoaderCircle } from "lucide-react"
import { useCallback, useEffect, useRef, useState } from "react"
import { CommanderField } from "@/components/commander-field"
import { DecklistUrlField } from "@/components/decklist-url-field"
import { ColorIdentity } from "@/components/mana-symbols"
import { combinedColorIdentity, type SelectedCard } from "@/lib/cards"
import { detailsFromDecklist, useResolveDecklist, type Decklist } from "@/lib/decklists"
import {
  decklistFromRemoteDeck,
  remoteDecksQueryOptions,
  remoteDeckSourceLabels,
} from "@/lib/remote-decks"

export interface DeckFormValue {
  commander: SelectedCard | null
  partner: SelectedCard | null
  colorIdentity: string
  decklistUrl: string
}

interface DeckFormFieldsProps {
  value: DeckFormValue
  onChange: (patch: Partial<DeckFormValue>) => void
  onResolvedName?: (name: string) => void
  manualEditVersion?: number
}

export function DeckFormFields({
  value,
  onChange,
  onResolvedName,
  manualEditVersion,
}: DeckFormFieldsProps) {
  const applyDecklist = useApplyDecklist(onChange, onResolvedName, manualEditVersion)

  function changeCard(field: "commander" | "partner", card: SelectedCard | null) {
    applyDecklist.cancel()
    const cards = field === "commander" ? [card, value.partner] : [value.commander, card]
    const colorIdentity = combinedColorIdentity(cards)
    onChange({ [field]: card, ...(colorIdentity === null ? {} : { colorIdentity }) })
  }

  return (
    <>
      <CommanderField
        value={value.commander}
        onChange={(card) => changeCard("commander", card)}
        required
      />
      <CommanderField
        label="Partner (optional)"
        mode="partner"
        value={value.partner}
        onChange={(card) => changeCard("partner", card)}
      />
      <label className="form-control min-w-0">
        <span className="label-text mb-1 text-sm font-medium">Color identity</span>
        <span className="relative">
          <input
            className="input input-bordered min-w-0 w-full pr-24 font-mono uppercase"
            value={value.colorIdentity}
            placeholder="WUBRG"
            pattern="(?!.*(.).*\\1)[WUBRG]*"
            onChange={(event) => {
              applyDecklist.cancel()
              onChange({ colorIdentity: event.target.value.toUpperCase() })
            }}
          />
          <ColorIdentity
            colors={value.colorIdentity}
            className="pointer-events-none absolute top-1/2 right-3 -translate-y-1/2 text-lg"
          />
        </span>
      </label>
      <div className="min-w-0 sm:col-span-2">
        <RemoteDeckPicker onPick={applyDecklist.apply} />
        <DecklistUrlField
          value={value.decklistUrl}
          onChange={(decklistUrl) => {
            applyDecklist.cancel()
            onChange({ decklistUrl })
          }}
          onResolved={applyDecklist.apply}
        />
        {applyDecklist.isPending && (
          <p className="text-base-content/60 mt-1 text-sm" role="status">
            Looking up commanders…
          </p>
        )}
        {applyDecklist.error && (
          <p className="text-error mt-1 text-sm" role="alert">
            {applyDecklist.error}
          </p>
        )}
      </div>
    </>
  )
}

function useApplyDecklist(
  onChange: (patch: Partial<DeckFormValue>) => void,
  onResolvedName?: (name: string) => void,
  manualEditVersion?: number,
) {
  const callbacks = useRef({ onChange, onResolvedName })
  callbacks.current = { onChange, onResolvedName }
  const currentManualEdit = useRef(manualEditVersion)
  currentManualEdit.current = manualEditVersion
  const requestId = useRef(0)
  const [isPending, setIsPending] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const cancel = useCallback(() => {
    requestId.current += 1
    setIsPending(false)
    setError(null)
  }, [])

  const previousManualEdit = useRef(manualEditVersion)
  useEffect(() => {
    if (manualEditVersion === previousManualEdit.current) return
    previousManualEdit.current = manualEditVersion
    cancel()
  }, [cancel, manualEditVersion])

  const apply = useCallback(async (decklist: Decklist | Promise<Decklist>) => {
    const currentRequest = ++requestId.current
    const startingManualEdit = currentManualEdit.current
    setIsPending(true)
    setError(null)

    try {
      const details = await detailsFromDecklist(await decklist)
      if (
        currentRequest !== requestId.current ||
        startingManualEdit !== currentManualEdit.current
      ) {
        return
      }
      callbacks.current.onResolvedName?.(details.name)
      callbacks.current.onChange(details)
    } catch {
      if (
        currentRequest === requestId.current &&
        startingManualEdit === currentManualEdit.current
      ) {
        setError("That deck's commander details could not be loaded. Try again.")
      }
    } finally {
      if (currentRequest === requestId.current) setIsPending(false)
    }
  }, [])

  return { apply, cancel, isPending, error }
}

function RemoteDeckPicker({
  onPick,
}: {
  onPick: (decklist: Decklist | Promise<Decklist>) => void
}) {
  const remoteDecks = useQuery(remoteDecksQueryOptions)
  const resolve = useResolveDecklist()
  const [selectedUrl, setSelectedUrl] = useState("")
  const decks = remoteDecks.data?.decks ?? []

  if (decks.length === 0) return null

  return (
    <div className="bg-primary/5 border-primary/20 mb-3 rounded-box border p-3">
      <label className="form-control flex flex-col gap-1.5">
        <span className="text-sm font-medium">Quick pick from my hosted decks</span>
        <select
          className="select select-bordered select-sm w-full"
          value={selectedUrl}
          onChange={(event) => {
            const url = event.target.value
            const deck = decks.find((candidate) => candidate.url === url)
            if (!url || !deck) return
            setSelectedUrl(url)
            if (deck.source === "manavault") {
              onPick(decklistFromRemoteDeck(deck))
            } else {
              onPick(resolve.mutateAsync(url))
            }
          }}
        >
          <option value="">Choose a hosted deck…</option>
          {decks.map((deck) => (
            <option key={`${deck.source}:${deck.url}`} value={deck.url}>
              {deck.name} · {remoteDeckSourceLabels[deck.source]}
            </option>
          ))}
        </select>
      </label>
      {resolve.isPending && (
        <p className="text-base-content/60 mt-2 flex items-center gap-1.5 text-xs" role="status">
          <LoaderCircle className="size-3 animate-spin" /> Loading deck details…
        </p>
      )}
    </div>
  )
}
