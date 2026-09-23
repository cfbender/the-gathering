import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { useState } from "react"
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { CommanderField } from "@/features/decks/commander-field"
import { canManageDeck, commanderNames, type DeckSummary } from "@/features/decks/decks"
import { getPlayers, invalidateGameRelated } from "@/features/games/games"
import { api, ApiError } from "@/lib/api"
import { useCurrentUser } from "@/lib/auth"
import {
  cardSnapshot,
  combinedColorIdentity,
  getCard,
  selectCatalogCard,
  type SelectedCard,
} from "@/lib/cards"

interface Props {
  playerId: number
  deck?: DeckSummary
  onChoose: (id: number) => void
}

/** Shared by the popover and Decks tab; other seats may select, but only owners/admins edit. */
export function CommanderActions({ playerId, deck, onChoose }: Props) {
  const viewer = useCurrentUser()
  const players = useQuery({ queryKey: ["players"], queryFn: getPlayers })
  const player = players.data?.find((candidate) => candidate.id === playerId)
  const [editing, setEditing] = useState<"new" | "edit" | null>(null)
  const allowed =
    viewer.data && (viewer.data.role === "admin" || player?.user_id === viewer.data.id)
  if (!allowed) return null

  return (
    <>
      <div className="mt-2 grid gap-1 border-t border-white/10 pt-2">
        <button
          type="button"
          className="btn btn-ghost btn-sm justify-start"
          onClick={() => setEditing("new")}
        >
          New commander…
        </button>
        {deck && canManageDeck(viewer.data, deck) && (
          <button
            type="button"
            className="btn btn-ghost btn-sm justify-start"
            onClick={() => setEditing("edit")}
          >
            Edit commander…
          </button>
        )}
      </div>
      {editing && (
        <NewCommanderDialog
          playerId={playerId}
          deck={editing === "edit" ? deck : undefined}
          onChoose={onChoose}
          onClose={() => setEditing(null)}
        />
      )}
    </>
  )
}

function NewCommanderDialog({
  playerId,
  deck,
  onChoose,
  onClose,
}: Props & { onClose: () => void }) {
  const commander = useQuery({
    queryKey: ["cards", deck?.commander_card_id],
    queryFn: () => getCard(deck!.commander_card_id!),
    enabled: Boolean(deck?.commander_card_id),
  })
  const partner = useQuery({
    queryKey: ["cards", deck?.partner_card_id],
    queryFn: () => getCard(deck!.partner_card_id!),
    enabled: Boolean(deck?.partner_card_id),
  })
  const loading = commander.isLoading || partner.isLoading
  const snapshot = (field: "commander" | "partner", card: typeof commander.data) => {
    const selected = card
      ? selectCatalogCard(card)
      : cardSnapshot(deck?.[`${field}_card_id`], deck?.[`${field}_name`])
    return selected ? { ...selected, printing_id: deck?.[`${field}_printing_id`] ?? null } : null
  }

  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) onClose()
      }}
    >
      <DialogContent className="sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>{deck ? "Edit commander" : "New commander"}</DialogTitle>
          <DialogClose onClose={onClose} />
        </DialogHeader>
        {loading ? (
          <p role="status" className="p-5">
            Loading commander cards…
          </p>
        ) : (
          <CommanderForm
            playerId={playerId}
            deck={deck}
            onChoose={onChoose}
            onClose={onClose}
            initialCommander={snapshot("commander", commander.data)}
            initialPartner={snapshot("partner", partner.data)}
          />
        )}
      </DialogContent>
    </Dialog>
  )
}

export function commanderPayload(
  name: string,
  commander: SelectedCard | null,
  partner: SelectedCard | null,
  previousColors = "",
) {
  return {
    name: name.trim(),
    commander_card_id: commander?.catalog_id ?? null,
    commander_name: commander?.name ?? null,
    commander_printing_id: commander?.printing_id ?? null,
    partner_card_id: partner?.catalog_id ?? null,
    partner_name: partner?.name ?? null,
    partner_printing_id: partner?.printing_id ?? null,
    color_identity: combinedColorIdentity([commander, partner]) ?? previousColors,
  }
}

function CommanderForm({
  playerId,
  deck,
  onChoose,
  onClose,
  initialCommander,
  initialPartner,
}: Props & {
  onClose: () => void
  initialCommander: SelectedCard | null
  initialPartner: SelectedCard | null
}) {
  const queryClient = useQueryClient()
  const [commander, setCommander] = useState(initialCommander)
  const [partner, setPartner] = useState(initialPartner)
  const [customName, setCustomName] = useState<string | null>(deck?.name ?? null)
  const name =
    customName ??
    commanderNames({ commander_name: commander?.name ?? "", partner_name: partner?.name ?? null })
  const mutation = useMutation({
    mutationFn: () =>
      api<{ data: DeckSummary }>(deck ? `/api/decks/${deck.id}` : "/api/decks", {
        method: deck ? "PATCH" : "POST",
        body: JSON.stringify({
          deck: {
            ...commanderPayload(name, commander, partner, deck?.color_identity),
            // Artwork-only edits must not replace a manually maintained color identity.
            ...(deck && commander?.id === initialCommander?.id && partner?.id === initialPartner?.id
              ? { color_identity: deck.color_identity }
              : {}),
            ...(!deck ? { player_id: playerId } : {}),
          },
        }),
      }).then((body) => body.data),
    onSuccess: async (saved) => {
      await invalidateGameRelated(queryClient)
      onChoose(saved.id)
      onClose()
    },
  })

  return (
    <form
      className="space-y-4 p-5"
      onSubmit={(event) => {
        event.preventDefault()
        mutation.mutate()
      }}
    >
      <p className="text-base-content/65 text-sm">
        Choose your commander and an optional partner, Background, or companion. Saving selects this
        deck at the table.
      </p>
      <fieldset disabled={mutation.isPending} className="grid min-w-0 gap-4 sm:grid-cols-2">
        <CommanderField value={commander} onChange={setCommander} required allowPrintings />
        <CommanderField
          label="Partner (optional)"
          mode="all"
          value={partner}
          onChange={setPartner}
          allowPrintings
        />
        <label className="form-control sm:col-span-2">
          <span className="mb-1 text-sm font-medium">Deck name</span>
          <input
            className="input input-bordered w-full"
            value={name}
            required
            maxLength={100}
            onChange={(event) => setCustomName(event.target.value)}
          />
        </label>
      </fieldset>
      {mutation.error && (
        <div role="alert" className="text-error text-sm">
          {mutation.error instanceof ApiError ? (
            <>
              {mutation.error.detail}
              {Object.entries(mutation.error.errors)
                .filter(([, messages]) => Array.isArray(messages))
                .map(([field, messages]) => (
                  <p key={field}>
                    {field.replaceAll("_", " ")}: {(messages as string[]).join(", ")}
                  </p>
                ))}
            </>
          ) : (
            "Could not save the commander. Try again."
          )}
        </div>
      )}
      <div className="flex justify-end gap-2">
        <button
          type="button"
          className="btn btn-ghost"
          onClick={onClose}
          disabled={mutation.isPending}
        >
          Cancel
        </button>
        <button
          type="submit"
          className="btn btn-primary"
          disabled={!commander || mutation.isPending}
        >
          {mutation.isPending ? "Saving…" : deck ? "Save commander" : "Create and select"}
        </button>
      </div>
    </form>
  )
}
