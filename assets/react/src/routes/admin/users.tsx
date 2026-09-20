import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { createFileRoute } from "@tanstack/react-router"
import { PageHeader } from "@/components/app-shell"
import type { FormEvent } from "react"
import { BarChart3, Bot, Link2, Shield, Sparkles, Trash2, Trophy, Users } from "lucide-react"
import { useState } from "react"
import { SudoPrompt } from "@/components/sudo-prompt"
import { ConfirmDialog } from "@/components/ui/confirm-dialog"
import { api } from "@/lib/api"
import { errorMessage, isSudoRequired, requireAdmin } from "@/lib/auth"
import type { User } from "@/lib/auth"
import { formValue } from "@/lib/form"
import { getPlayers, invalidateGameRelated, linkUserPlayer } from "@/lib/games"
import type { Player } from "@/lib/games"

interface Data<T> {
  data: T
}

interface AdminSettings {
  registration_enabled: boolean
  detailed_stats_from: string | null
}

interface BackfillSummary {
  decks_split: number
  decks_linked: number
  colors_filled: number
  mvps_linked: number
  unmatched: string[]
}

interface PendingDiscordGame {
  id: number
  external_id: string
  guild_id: string
  channel_id: string
  played_at: string
  players: Array<{
    discord_id: string
    display_name: string
    commander_name: string | null
  }>
}

export const Route = createFileRoute("/admin/users")({
  beforeLoad: ({ context, location }) => requireAdmin(context.queryClient, location.href),
  component: AdminUsersPage,
})

function AdminUsersPage() {
  const queryClient = useQueryClient()
  const users = useQuery({
    queryKey: ["admin", "users"],
    queryFn: async () => (await api<Data<User[]>>("/api/admin/users")).data,
  })
  const players = useQuery({ queryKey: ["players"], queryFn: getPlayers })
  const settings = useQuery({
    queryKey: ["admin", "settings"],
    queryFn: async () => (await api<Data<AdminSettings>>("/api/admin/settings")).data,
  })
  const pendingDiscordGames = useQuery({
    queryKey: ["admin", "discord", "pending"],
    queryFn: async () => (await api<Data<PendingDiscordGame[]>>("/api/admin/discord/pending")).data,
  })
  const toggleRegistration = useMutation({
    mutationFn: (registration_enabled: boolean) =>
      api<Data<AdminSettings>>("/api/admin/settings", {
        method: "PATCH",
        body: JSON.stringify({ settings: { registration_enabled } }),
      }),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["admin", "settings"] })
      void queryClient.invalidateQueries({ queryKey: ["registration"] })
    },
  })
  const querySudoError = users.error ?? settings.error ?? pendingDiscordGames.error

  if (isSudoRequired(querySudoError)) {
    return (
      <div className="mx-auto flex max-w-2xl flex-col gap-6">
        <PageHeader
          eyebrow="Administration"
          title="Users"
          description="Confirm your identity to manage server access."
        />
        <SudoPrompt
          error={querySudoError}
          onSuccess={() => void queryClient.invalidateQueries({ queryKey: ["admin"] })}
        />
      </div>
    )
  }

  return (
    <div className="flex flex-col gap-6">
      <PageHeader
        eyebrow="Administration"
        title="Users"
        description="Accounts, access, and server settings."
        actions={
          <label className="bg-base-100/60 border-base-300 rounded-field flex cursor-pointer items-center gap-3 border px-4 py-3">
            <span className="text-sm font-medium">Open registration</span>
            <input
              type="checkbox"
              className="toggle toggle-primary"
              checked={settings.data?.registration_enabled ?? false}
              disabled={!settings.data || toggleRegistration.isPending}
              onChange={(event) => toggleRegistration.mutate(event.target.checked)}
            />
          </label>
        }
      />

      <SudoPrompt
        error={toggleRegistration.error}
        onSuccess={() => {
          if (toggleRegistration.variables !== undefined) {
            toggleRegistration.mutate(toggleRegistration.variables)
          }
        }}
      />

      <PendingDiscordGames games={pendingDiscordGames.data} error={pendingDiscordGames.error} />

      <StatsCutoff settings={settings.data} />

      <CatalogBackfill />

      <section aria-labelledby="accounts-heading">
        <h2 id="accounts-heading" className="mb-3 flex items-center gap-2 text-lg font-semibold">
          <Users className="size-5" /> Accounts
        </h2>
        {users.isPending && <div className="skeleton h-32 w-full" />}
        {users.error && <div className="alert alert-error">{errorMessage(users.error)}</div>}
        <div className="grid grid-cols-[minmax(0,1fr)] gap-3">
          {users.data?.map((user) => (
            <UserCard key={user.id} user={user} players={players.data ?? []} />
          ))}
        </div>
      </section>
    </div>
  )
}

/**
 * Imported games name commanders and MVP cards without Scryfall IDs. This
 * links them to the local catalog so art, color identity, and card pages work.
 * It also runs on its own after every import and catalog sync.
 */
function CatalogBackfill() {
  const queryClient = useQueryClient()
  const run = useMutation({
    mutationFn: async () =>
      (await api<Data<BackfillSummary>>("/api/admin/catalog/backfill", { method: "POST" })).data,
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["decks"] })
      void queryClient.invalidateQueries({ queryKey: ["games"] })
      void queryClient.invalidateQueries({ queryKey: ["players"] })
      void queryClient.invalidateQueries({ queryKey: ["stats"] })
    },
  })
  const summary = run.data

  return (
    <section
      className="card bg-base-200 border-base-300 border"
      aria-labelledby="catalog-backfill-heading"
    >
      <div className="card-body gap-3 p-4 sm:p-5">
        <h2 id="catalog-backfill-heading" className="flex items-center gap-2 text-lg font-semibold">
          <Sparkles className="size-5" /> Link imported cards to the catalog
        </h2>
        <p className="text-base-content/70 text-sm">
          Imported games name commanders and MVP cards without card IDs. Linking them by name fills
          in card art and missing color identities and splits Mythic Track&apos;s &ldquo;Commander
          || Partner&rdquo; decks. This runs automatically after imports and catalog syncs; run it
          by hand after fixing a misspelled name.
        </p>
        <div>
          <button
            type="button"
            className="btn btn-primary"
            disabled={run.isPending}
            onClick={() => run.mutate()}
          >
            {run.isPending ? "Linking…" : "Link cards now"}
          </button>
        </div>
        {summary && (
          <p className="text-sm" role="status">
            Linked {summary.decks_linked} commander{summary.decks_linked === 1 ? "" : "s"}, split{" "}
            {summary.decks_split} partner deck{summary.decks_split === 1 ? "" : "s"}, filled{" "}
            {summary.colors_filled} color identit{summary.colors_filled === 1 ? "y" : "ies"}, linked{" "}
            {summary.mvps_linked} MVP card{summary.mvps_linked === 1 ? "" : "s"}.
            {summary.unmatched.length > 0 && (
              <>
                {" "}
                No catalog match for:{" "}
                <span className="font-medium">{summary.unmatched.join(", ")}</span>.
              </>
            )}
          </p>
        )}
        {run.error && !isSudoRequired(run.error) && (
          <p className="text-error text-sm">{errorMessage(run.error)}</p>
        )}
        <SudoPrompt error={run.error} onSuccess={() => run.mutate()} />
      </div>
    </section>
  )
}

function PendingDiscordGames({
  games,
  error,
}: {
  games: PendingDiscordGame[] | undefined
  error: Error | null
}) {
  return (
    <section aria-labelledby="pending-discord-heading">
      <h2
        id="pending-discord-heading"
        className="mb-1 flex items-center gap-2 text-lg font-semibold"
      >
        <Bot className="size-5" /> Pending Discord games
      </h2>
      <p className="text-base-content/65 mb-3 text-sm">
        SpellBot games awaiting a winner. Unresolved reports are kept for 30 days.
      </p>
      {error && !isSudoRequired(error) && (
        <div className="alert alert-error mb-3">{errorMessage(error)}</div>
      )}
      {games && games.length === 0 && (
        <div className="border-base-300 text-base-content/60 rounded-box border border-dashed p-5 text-sm">
          No pending Discord games.
        </div>
      )}
      <div className="grid gap-3 lg:grid-cols-2">
        {games?.map((game) => (
          <PendingDiscordGameCard key={game.id} game={game} />
        ))}
      </div>
    </section>
  )
}

function PendingDiscordGameCard({ game }: { game: PendingDiscordGame }) {
  const queryClient = useQueryClient()
  const [winner, setWinner] = useState("")
  const [confirmDiscard, setConfirmDiscard] = useState(false)
  const refresh = () => {
    void queryClient.invalidateQueries({ queryKey: ["admin", "discord", "pending"] })
    void queryClient.invalidateQueries({ queryKey: ["games"] })
    void queryClient.invalidateQueries({ queryKey: ["players"] })
    void queryClient.invalidateQueries({ queryKey: ["decks"] })
  }
  const resolve = useMutation({
    mutationFn: (winnerDiscordId: string) =>
      api<void>(`/api/admin/discord/pending/${game.id}`, {
        method: "PATCH",
        body: JSON.stringify({ winner_discord_id: winnerDiscordId }),
      }),
    onSuccess: refresh,
  })
  const discard = useMutation({
    mutationFn: () => api<void>(`/api/admin/discord/pending/${game.id}`, { method: "DELETE" }),
    onSuccess: refresh,
  })
  const gameId = game.external_id.replace("spellbot:", "")
  const mutationError = resolve.error ?? discard.error

  return (
    <article className="card bg-base-200 border-base-300 border">
      <div className="card-body gap-4 p-4 sm:p-5">
        <div>
          <div className="flex items-center justify-between gap-3">
            <h3 className="font-semibold">{gameId}</h3>
            <time className="text-base-content/60 text-xs" dateTime={game.played_at}>
              {new Date(game.played_at).toLocaleString()}
            </time>
          </div>
          <p className="text-base-content/55 mt-1 text-xs">Channel {game.channel_id}</p>
        </div>
        <ul className="grid gap-1 text-sm">
          {game.players.map((player) => (
            <li key={player.discord_id} className="flex justify-between gap-3">
              <span>{player.display_name}</span>
              {player.commander_name && (
                <span className="text-base-content/60 truncate">{player.commander_name}</span>
              )}
            </li>
          ))}
        </ul>
        <div className="flex flex-col gap-2 sm:flex-row">
          <select
            className="select select-sm min-w-0 flex-1"
            aria-label={`Winner for ${gameId}`}
            value={winner}
            onChange={(event) => setWinner(event.target.value)}
          >
            <option value="">Choose winner</option>
            {game.players.map((player) => (
              <option key={player.discord_id} value={player.discord_id}>
                {player.display_name}
              </option>
            ))}
          </select>
          <button
            type="button"
            className="btn btn-primary btn-sm"
            disabled={!winner || resolve.isPending || discard.isPending}
            onClick={() => resolve.mutate(winner)}
          >
            <Trophy className="size-4" /> Record winner
          </button>
          <button
            type="button"
            className="btn btn-ghost btn-sm text-error"
            disabled={resolve.isPending || discard.isPending}
            onClick={() => setConfirmDiscard(true)}
          >
            <Trash2 className="size-4" /> Discard
          </button>
        </div>
        {mutationError && !isSudoRequired(mutationError) && (
          <p className="text-error text-sm">{errorMessage(mutationError)}</p>
        )}
        <SudoPrompt
          error={mutationError}
          onSuccess={() => {
            if (isSudoRequired(resolve.error) && resolve.variables) {
              resolve.mutate(resolve.variables)
            }
            if (isSudoRequired(discard.error)) discard.mutate()
          }}
        />
      </div>
      <ConfirmDialog
        open={confirmDiscard}
        onOpenChange={setConfirmDiscard}
        title={`Discard ${gameId}?`}
        confirmLabel="Discard report"
        destructive
        onConfirm={() => discard.mutate()}
      >
        This removes the pending report without adding it to game history.
      </ConfirmDialog>
    </article>
  )
}

/**
 * Games before the cutoff still count toward win/loss records, but their seat,
 * duration, turn, and MVP data are left out of statistics. For pods that only
 * started recording those details partway through their history.
 */
function StatsCutoff({ settings }: { settings: AdminSettings | undefined }) {
  const queryClient = useQueryClient()
  const save = useMutation({
    mutationFn: (detailed_stats_from: string | null) =>
      api<Data<AdminSettings>>("/api/admin/settings", {
        method: "PATCH",
        body: JSON.stringify({ settings: { detailed_stats_from } }),
      }),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["admin", "settings"] })
      void queryClient.invalidateQueries({ queryKey: ["stats"] })
    },
  })

  function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    save.mutate(formValue(event.currentTarget, "detailed_stats_from") || null)
  }

  return (
    <section
      className="card bg-base-200 border-base-300 border"
      aria-labelledby="stats-cutoff-heading"
    >
      <form className="card-body gap-3 p-4 sm:p-5" onSubmit={submit}>
        <h2 id="stats-cutoff-heading" className="flex items-center gap-2 text-lg font-semibold">
          <BarChart3 className="size-5" /> Detailed statistics from
        </h2>
        <p className="text-base-content/70 text-sm">
          Games before this date still count toward every win/loss record. Their seat positions,
          game length, turn counts, and MVP cards are ignored, so a pod that only started recording
          those later keeps its win rates without muddying the rest. Leave blank to use every game.
        </p>
        <div className="flex flex-col gap-2 sm:flex-row">
          <input
            key={settings?.detailed_stats_from ?? ""}
            type="date"
            name="detailed_stats_from"
            aria-label="Detailed statistics from"
            className="input min-w-0 flex-1"
            defaultValue={settings?.detailed_stats_from ?? ""}
            disabled={!settings || save.isPending}
          />
          <button type="submit" className="btn btn-primary" disabled={!settings || save.isPending}>
            Save
          </button>
          {settings?.detailed_stats_from && (
            <button
              type="button"
              className="btn btn-ghost"
              disabled={save.isPending}
              onClick={() => save.mutate(null)}
            >
              Clear
            </button>
          )}
        </div>
        {save.error && !isSudoRequired(save.error) && (
          <p className="text-error text-sm">{errorMessage(save.error)}</p>
        )}
        <SudoPrompt error={save.error} onSuccess={() => save.mutate(save.variables ?? null)} />
      </form>
    </section>
  )
}

function UserCard({ user, players }: { user: User; players: Player[] }) {
  const queryClient = useQueryClient()
  const [confirmDelete, setConfirmDelete] = useState(false)
  const update = useMutation({
    mutationFn: (attrs: {
      username?: string
      display_name?: string
      role?: string
      disabled?: boolean
    }) =>
      api<Data<User>>(`/api/admin/users/${user.id}`, {
        method: "PATCH",
        body: JSON.stringify({ user: attrs }),
      }),
    onSuccess: () => void queryClient.invalidateQueries({ queryKey: ["admin", "users"] }),
  })
  const deleteUser = useMutation({
    mutationFn: () => api<void>(`/api/admin/users/${user.id}`, { method: "DELETE" }),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["admin", "users"] })
      void queryClient.invalidateQueries({ queryKey: ["players"] })
      void queryClient.invalidateQueries({ queryKey: ["games"] })
    },
  })
  const mutationError = update.error ?? deleteUser.error

  function saveNames(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    update.mutate({
      username: formValue(event.currentTarget, "username"),
      display_name: formValue(event.currentTarget, "display_name"),
    })
  }

  return (
    <div className="card bg-base-200 border-base-300 min-w-0 border">
      <div className="card-body gap-4 p-4 sm:p-5">
        <div className="flex flex-col justify-between gap-3 sm:flex-row sm:items-center">
          <div className="min-w-0">
            <div className="flex flex-wrap items-center gap-2">
              <h3 className="max-w-full min-w-0 truncate font-semibold" title={user.display_name}>
                {user.display_name}
              </h3>
              <span className="badge badge-outline badge-sm gap-1">
                <Shield className="size-3" /> {user.role}
              </span>
              {user.disabled && <span className="badge badge-error badge-sm">disabled</span>}
            </div>
            <p className="text-base-content/60 truncate text-sm" title={`@${user.username}`}>
              @{user.username}
            </p>
          </div>
          <div className="flex flex-wrap gap-2">
            <select
              aria-label={`Role for ${user.username}`}
              className="select select-sm"
              value={user.role}
              disabled={update.isPending}
              onChange={(event) => update.mutate({ role: event.target.value })}
            >
              <option value="member">Member</option>
              <option value="admin">Admin</option>
            </select>
            <button
              type="button"
              className={
                user.disabled ? "btn btn-success btn-sm" : "btn btn-error btn-outline btn-sm"
              }
              disabled={update.isPending}
              onClick={() => update.mutate({ disabled: !user.disabled })}
            >
              {user.disabled ? "Enable" : "Disable"}
            </button>
            <button
              type="button"
              className="btn btn-ghost btn-sm text-error"
              disabled={update.isPending || deleteUser.isPending}
              onClick={() => setConfirmDelete(true)}
            >
              <Trash2 className="size-4" /> Delete user
            </button>
          </div>
        </div>
        {mutationError && !isSudoRequired(mutationError) && (
          <div className="alert alert-error py-2 text-sm">
            {errorMessage(mutationError, "role") ?? errorMessage(mutationError)}
          </div>
        )}
        <SudoPrompt
          error={mutationError}
          onSuccess={() => {
            if (update.isError && update.variables) update.mutate(update.variables)
            else if (deleteUser.isError) deleteUser.mutate()
          }}
        />
        <form className="grid gap-2 sm:grid-cols-[1fr_1fr_auto]" onSubmit={saveNames}>
          <label className="floating-label">
            <span>Username</span>
            <input
              name="username"
              aria-label={`Username for ${user.username}`}
              defaultValue={user.username}
              className="input input-sm w-full"
              autoCapitalize="none"
              autoCorrect="off"
              required
            />
          </label>
          <label className="floating-label">
            <span>Display name</span>
            <input
              name="display_name"
              aria-label={`Display name for ${user.username}`}
              defaultValue={user.display_name}
              className="input input-sm w-full"
              required
            />
          </label>
          <button className="btn btn-sm" disabled={update.isPending}>
            Save names
          </button>
        </form>
        {errorMessage(update.error, "username") && (
          <p className="text-error text-sm">{errorMessage(update.error, "username")}</p>
        )}
        <LinkedPlayer user={user} players={players} />
      </div>
      <ConfirmDialog
        open={confirmDelete}
        onOpenChange={setConfirmDelete}
        title={`Delete ${user.display_name}?`}
        confirmLabel="Delete user"
        destructive
        onConfirm={() => deleteUser.mutate()}
      >
        Their account and sessions will be permanently deleted. Games and their player record,
        including decks and game history, will be kept, and the player will be unlinked from the
        account.
      </ConfirmDialog>
    </div>
  )
}

/**
 * Which player's games belong to this account. Imported histories (CSV, Mythic
 * Track) create players nobody has claimed yet; choosing one here moves the
 * account's current player, if any, into it.
 */
function LinkedPlayer({ user, players }: { user: User; players: Player[] }) {
  const queryClient = useQueryClient()
  const current = players.find((player) => player.user_id === user.id)
  const [pending, setPending] = useState<Player | null>(null)
  const link = useMutation({
    mutationFn: (player: Player) => linkUserPlayer(user.id, player.id),
    onSuccess: () => {
      void invalidateGameRelated(queryClient)
    },
  })
  const options = players.filter((player) => player.user_id === null || player.user_id === user.id)

  return (
    <div className="border-base-300 flex flex-col gap-2 border-t pt-3">
      <label className="flex flex-wrap items-center gap-2 text-sm">
        <span className="flex items-center gap-1 font-medium">
          <Link2 className="size-4" /> Player
        </span>
        <select
          aria-label={`Player for ${user.username}`}
          className="select select-sm min-w-0 flex-1"
          value={current?.id ?? ""}
          disabled={link.isPending}
          onChange={(event) => {
            const player = players.find((item) => String(item.id) === event.target.value)
            if (player && player.id !== current?.id) setPending(player)
          }}
        >
          <option value="" disabled>
            {current ? current.name : "Not linked to a player"}
          </option>
          {options.map((player) => (
            <option key={player.id} value={player.id}>
              {player.name}
            </option>
          ))}
        </select>
      </label>
      {link.error && !isSudoRequired(link.error) && (
        <p className="text-error text-sm">
          {errorMessage(link.error, "merge") ?? errorMessage(link.error)}
        </p>
      )}
      <SudoPrompt
        error={link.error}
        onSuccess={() => link.variables && link.mutate(link.variables)}
      />
      <ConfirmDialog
        open={pending !== null}
        onOpenChange={(open) => !open && setPending(null)}
        title={`Link ${user.display_name} to ${pending?.name ?? ""}?`}
        confirmLabel="Link player"
        onConfirm={() => pending && link.mutate(pending)}
      >
        {current ? (
          <>
            <strong>{current.name}</strong>'s games and decks move to{" "}
            <strong>{pending?.name}</strong>, and {current.name} is removed. This cannot be undone.
          </>
        ) : (
          <>
            Games recorded for <strong>{pending?.name}</strong> will count for this account, and
            Discord sign-in will use this player from now on.
          </>
        )}
      </ConfirmDialog>
    </div>
  )
}
