import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { useNavigate } from "@tanstack/react-router"
import { Plus } from "lucide-react"
import { useState, type FormEvent } from "react"
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { DeckFormFields, type DeckFormValue } from "@/features/decks/deck-form-fields"
import { commanderNames, type DeckDetail } from "@/features/decks/decks"
import { canManagePlayer, getPlayers, invalidateGameRelated } from "@/features/games/games"
import { api, ApiError } from "@/lib/api"
import { useCurrentUser } from "@/lib/auth"

/** Opens a dialog that creates a deck for the viewer's player (or another they manage). */
export function NewDeckButton() {
  const [open, setOpen] = useState(false)
  return (
    <>
      <button type="button" className="btn btn-primary btn-sm" onClick={() => setOpen(true)}>
        <Plus className="size-4" /> New deck
      </button>
      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent className="sm:max-w-2xl">
          <DialogHeader>
            <DialogTitle>New deck</DialogTitle>
            <DialogClose onClose={() => setOpen(false)} />
          </DialogHeader>
          {open && <NewDeckForm onCancel={() => setOpen(false)} />}
        </DialogContent>
      </Dialog>
    </>
  )
}

function NewDeckForm({ onCancel }: { onCancel: () => void }) {
  const viewer = useCurrentUser()
  const players = useQuery({ queryKey: ["players"], queryFn: getPlayers })
  const owners = (players.data ?? [])
    .filter((player) => !player.archived_at && canManagePlayer(viewer.data, player))
    // The viewer's own player first, then guests and (for admins) everyone else.
    .sort((a, b) => Number(b.user_id === viewer.data?.id) - Number(a.user_id === viewer.data?.id))
  const [chosenOwner, setChosenOwner] = useState<string | null>(null)
  const ownerId = chosenOwner ?? (owners[0] ? String(owners[0].id) : "")

  const [details, setDetails] = useState<DeckFormValue>({
    commander: null,
    partner: null,
    colorIdentity: "",
    decklistUrl: "",
  })
  const [customName, setCustomName] = useState<string | null>(null)
  const [nameEditVersion, setNameEditVersion] = useState(0)
  const name =
    customName ??
    commanderNames({
      commander_name: details.commander?.name ?? "",
      partner_name: details.partner?.name ?? null,
    })

  const queryClient = useQueryClient()
  const navigate = useNavigate()
  const mutation = useMutation({
    mutationFn: () =>
      api<{ data: DeckDetail }>("/api/decks", {
        method: "POST",
        body: JSON.stringify({
          player_id: Number(ownerId),
          name: name.trim(),
          commander_card_id: details.commander?.catalog_id ?? null,
          commander_name: details.commander?.name ?? null,
          commander_printing_id: details.commander?.printing_id ?? null,
          partner_card_id: details.partner?.catalog_id ?? null,
          partner_name: details.partner?.name ?? null,
          partner_printing_id: details.partner?.printing_id ?? null,
          color_identity: details.colorIdentity,
          decklist_url: details.decklistUrl.trim() || null,
        }),
      }).then((body) => body.data),
    onSuccess: async (deck) => {
      await invalidateGameRelated(queryClient)
      await navigate({ to: "/decks/$deckId", params: { deckId: String(deck.id) } })
    },
  })
  const error = mutation.error instanceof ApiError ? mutation.error : null

  function submit(event: FormEvent) {
    event.preventDefault()
    mutation.mutate()
  }

  if (players.isPending) {
    return <span className="loading loading-spinner m-5" aria-label="Loading players" />
  }

  if (owners.length === 0) {
    return (
      <p className="text-base-content/70 p-5 text-sm">
        Decks belong to a player. Ask an administrator to link your account to a player, then come
        back to add decks.
      </p>
    )
  }

  return (
    <form className="space-y-4 p-5" onSubmit={submit}>
      <p className="text-base-content/65 text-sm">
        Pick a commander, or paste a public deck link to fill in the details.
      </p>
      <fieldset disabled={mutation.isPending} className="grid min-w-0 gap-4 sm:grid-cols-2">
        {owners.length > 1 && (
          <div className="form-control sm:col-span-2">
            <span className="label-text mb-1 text-sm font-medium">Player</span>
            <Select value={ownerId} onValueChange={setChosenOwner}>
              <SelectTrigger aria-label="Player">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {owners.map((player) => (
                  <SelectItem key={player.id} value={String(player.id)}>
                    {player.name}
                    {player.user_id === viewer.data?.id && " (you)"}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
        )}
        <DeckFormFields
          value={details}
          allowPrintings
          onChange={(patch) => {
            setDetails((current) => ({ ...current, ...patch }))
            mutation.reset()
          }}
          onResolvedName={setCustomName}
          manualEditVersion={nameEditVersion}
        />
        <label className="form-control sm:col-span-2">
          <span className="label-text mb-1 text-sm font-medium">Deck name</span>
          <input
            className="input input-bordered w-full"
            value={name}
            required
            maxLength={100}
            onChange={(event) => {
              setCustomName(event.target.value)
              setNameEditVersion((version) => version + 1)
            }}
          />
        </label>
      </fieldset>
      {mutation.isError && (
        <div role="alert" className="alert alert-error text-sm">
          {error?.detail ??
            (Object.keys(error?.errors ?? {})
              .flatMap((field) =>
                (error?.fieldErrors(field) ?? []).map(
                  (message) => `${field.replaceAll("_", " ")} ${message}`,
                ),
              )
              .join(". ") ||
              "Could not create the deck. Try again.")}
        </div>
      )}
      <div className="flex justify-end gap-2">
        <button
          type="button"
          className="btn btn-ghost"
          onClick={onCancel}
          disabled={mutation.isPending}
        >
          Cancel
        </button>
        <button
          type="submit"
          className="btn btn-primary"
          disabled={!details.commander || mutation.isPending}
        >
          {mutation.isPending ? "Creating…" : "Create deck"}
        </button>
      </div>
    </form>
  )
}
