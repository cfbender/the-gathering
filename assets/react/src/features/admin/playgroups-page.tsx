import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { Pencil, Plus, Trash2, UserPlus, X } from "lucide-react"
import type { FormEvent } from "react"
import { useState } from "react"
import { PageHeader } from "@/components/app-shell"
import { SudoPrompt } from "@/components/sudo-prompt"
import { ConfirmDialog } from "@/components/ui/confirm-dialog"
import { PlaygroupInvite } from "@/features/playgroups/playgroup-invite"
import {
  addPlaygroupMember,
  adminPlaygroupsQueryOptions,
  createPlaygroup,
  deletePlaygroup,
  invalidatePlaygroups,
  removePlaygroupMember,
  updatePlaygroup,
  useSwitchPlaygroup,
} from "@/features/playgroups/playgroups"
import type { AdminPlaygroup, PlaygroupInput } from "@/features/playgroups/playgroups"
import { api } from "@/lib/api"
import { errorMessage, isSudoRequired, useCurrentUser } from "@/lib/auth"
import type { User } from "@/lib/auth"
import { formValue } from "@/lib/form"

const readInput = (form: HTMLFormElement): PlaygroupInput => ({
  name: formValue(form, "name").trim(),
  discord_guild_id: formValue(form, "discord_guild_id").trim(),
})

export function PlaygroupsPage() {
  const queryClient = useQueryClient()
  const session = useCurrentUser()
  const playgroups = useQuery(adminPlaygroupsQueryOptions)
  const users = useQuery({
    queryKey: ["admin", "users"],
    queryFn: async () => (await api<{ data: User[] }>("/api/admin/users")).data,
  })
  const switchPlaygroup = useSwitchPlaygroup()
  const [editing, setEditing] = useState<number | null>(null)
  const [pendingDelete, setPendingDelete] = useState<AdminPlaygroup | null>(null)
  const invalidate = () => invalidatePlaygroups(queryClient)

  const create = useMutation({
    mutationFn: createPlaygroup,
    onSuccess: invalidate,
  })
  const update = useMutation({
    mutationFn: ({ id, input }: { id: number; input: PlaygroupInput }) =>
      updatePlaygroup(id, input),
    onSuccess: () => {
      setEditing(null)
      invalidate()
    },
  })
  const remove = useMutation({
    mutationFn: (playgroup: AdminPlaygroup) => deletePlaygroup(playgroup.id),
    onSuccess: () => {
      invalidate()
      // Deleting the active playgroup moves the member to another of theirs on the server.
      void queryClient.invalidateQueries({ queryKey: ["session"] })
    },
  })

  function submitCreate(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const form = event.currentTarget
    create.mutate(readInput(form), { onSuccess: () => form.reset() })
  }

  const user = session.data
  const activeId = user?.playgroup_id ?? null

  return (
    <div className="flex flex-col gap-6">
      <PageHeader
        eyebrow="Administration"
        title="Playgroups"
        description="Each playgroup keeps its own players, decks, and games, and members only see the playgroups they belong to. Add members here or share a playgroup's invite link."
      />

      <form
        onSubmit={submitCreate}
        className="card border-base-300 bg-base-200 border"
        aria-labelledby="new-playgroup-heading"
      >
        <div className="card-body gap-4 p-4 sm:p-5">
          <h2 id="new-playgroup-heading" className="flex items-center gap-2 text-lg font-semibold">
            <Plus className="size-5" /> New playgroup
          </h2>
          <PlaygroupFields error={create.error} />
          {create.error && !isSudoRequired(create.error) && !hasFieldErrors(create.error) && (
            <p role="alert" className="text-error text-sm">
              {errorMessage(create.error)}
            </p>
          )}
          <SudoPrompt
            error={create.error}
            onSuccess={() => create.variables && create.mutate(create.variables)}
          />
          <div className="flex flex-wrap items-center gap-3">
            <button type="submit" className="btn btn-primary" disabled={create.isPending}>
              {create.isPending ? "Creating…" : "Create playgroup"}
            </button>
            <span className="text-base-content/60 text-sm">You become its first member.</span>
          </div>
        </div>
      </form>

      {playgroups.isPending && <p role="status">Loading playgroups…</p>}
      {playgroups.error && (
        <div role="alert" className="alert alert-error">
          {errorMessage(playgroups.error)}
        </div>
      )}
      {remove.error && !isSudoRequired(remove.error) && (
        <div role="alert" className="alert alert-error">
          {errorMessage(remove.error)}
        </div>
      )}
      <SudoPrompt
        error={remove.error}
        onSuccess={() => remove.variables && remove.mutate(remove.variables)}
      />
      {remove.isSuccess && (
        <p role="status" className="text-success text-sm">
          Deleted {remove.variables.name}.
        </p>
      )}

      {playgroups.data && user && (
        <ul className="grid gap-3" aria-label="Playgroups">
          {playgroups.data.map((playgroup) =>
            editing === playgroup.id ? (
              <li key={playgroup.id} className="card border-primary/40 bg-base-200 border">
                <form
                  className="card-body gap-4 p-4 sm:p-5"
                  aria-label={`Edit ${playgroup.name}`}
                  onSubmit={(event) => {
                    event.preventDefault()
                    update.mutate({ id: playgroup.id, input: readInput(event.currentTarget) })
                  }}
                >
                  <PlaygroupFields
                    error={update.variables?.id === playgroup.id ? update.error : null}
                    defaults={playgroup}
                  />
                  {update.error &&
                    !isSudoRequired(update.error) &&
                    !hasFieldErrors(update.error) && (
                      <p role="alert" className="text-error text-sm">
                        {errorMessage(update.error)}
                      </p>
                    )}
                  <SudoPrompt
                    error={update.error}
                    onSuccess={() => update.variables && update.mutate(update.variables)}
                  />
                  <div className="flex gap-2">
                    <button type="submit" className="btn btn-primary" disabled={update.isPending}>
                      {update.isPending ? "Saving…" : "Save"}
                    </button>
                    <button
                      type="button"
                      className="btn btn-ghost"
                      onClick={() => {
                        update.reset()
                        setEditing(null)
                      }}
                    >
                      Cancel
                    </button>
                  </div>
                </form>
              </li>
            ) : (
              <li key={playgroup.id} className="card border-base-300 bg-base-200 border">
                <div className="card-body gap-4 p-4 sm:p-5">
                  <div className="flex flex-col gap-4 sm:flex-row sm:items-start sm:justify-between">
                    <div className="min-w-0">
                      <div className="flex flex-wrap items-center gap-2">
                        <span className="break-words text-lg font-semibold">{playgroup.name}</span>
                        {playgroup.id === activeId && (
                          <span className="badge badge-primary badge-sm">Viewing</span>
                        )}
                        {playgroup.invite_enabled && (
                          <span className="badge badge-outline badge-sm">Invite link active</span>
                        )}
                      </div>
                      <dl className="mt-2 grid gap-1 text-sm">
                        <div className="flex flex-wrap gap-x-2">
                          <dt className="text-base-content/60">Discord server</dt>
                          <dd className="break-all font-mono">
                            {playgroup.discord_guild_id ?? "Not linked"}
                          </dd>
                        </div>
                      </dl>
                    </div>
                    <div className="flex flex-wrap gap-2 sm:shrink-0">
                      {playgroup.id !== activeId &&
                        playgroup.members.some((member) => member.id === user.id) && (
                          <button
                            type="button"
                            className="btn btn-sm"
                            disabled={switchPlaygroup.isPending}
                            onClick={() => switchPlaygroup.mutate(playgroup.id)}
                          >
                            View
                          </button>
                        )}
                      <button
                        type="button"
                        className="btn btn-outline btn-sm"
                        aria-label={`Edit ${playgroup.name}`}
                        onClick={() => {
                          update.reset()
                          setEditing(playgroup.id)
                        }}
                      >
                        <Pencil className="size-4" /> Edit
                      </button>
                      <button
                        type="button"
                        className="btn btn-outline btn-error btn-sm"
                        aria-label={`Delete ${playgroup.name}`}
                        disabled={remove.isPending || playgroups.data.length < 2}
                        onClick={() => setPendingDelete(playgroup)}
                      >
                        <Trash2 className="size-4" /> Delete
                      </button>
                    </div>
                  </div>
                  <Members playgroup={playgroup} users={users.data ?? []} currentUser={user} />
                  <PlaygroupInvite playgroupId={playgroup.id} name={playgroup.name} />
                </div>
              </li>
            ),
          )}
        </ul>
      )}

      <ConfirmDialog
        open={pendingDelete !== null}
        onOpenChange={(open) => !open && setPendingDelete(null)}
        title={`Delete ${pendingDelete?.name ?? "playgroup"}?`}
        confirmLabel="Delete playgroup"
        destructive
        onConfirm={() => pendingDelete && remove.mutate(pendingDelete)}
      >
        Only a playgroup with no games or decks can be deleted. Its memberships and players go with
        it, and members viewing it return to another of their playgroups.
      </ConfirmDialog>
    </div>
  )
}

/** A playgroup's members with removal, and a picker to add any other account. */
function Members({
  playgroup,
  users,
  currentUser,
}: {
  playgroup: AdminPlaygroup
  users: User[]
  currentUser: User
}) {
  const queryClient = useQueryClient()
  const [pendingRemove, setPendingRemove] = useState<AdminPlaygroup["members"][number] | null>(null)
  const add = useMutation({
    mutationFn: (userId: number) => addPlaygroupMember(playgroup.id, userId),
    onSuccess: () => {
      invalidatePlaygroups(queryClient)
      void queryClient.invalidateQueries({ queryKey: ["session"] })
    },
  })
  const remove = useMutation({
    mutationFn: (userId: number) => removePlaygroupMember(playgroup.id, userId),
    onSuccess: () => {
      invalidatePlaygroups(queryClient)
      void queryClient.invalidateQueries({ queryKey: ["session"] })
    },
  })
  const memberIds = new Set(playgroup.members.map((member) => member.id))
  const candidates = users.filter((user) => !memberIds.has(user.id) && !user.disabled)
  const error = add.error ?? remove.error

  function submitAdd(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const form = event.currentTarget
    const userId = Number(formValue(form, "user_id"))
    if (userId) add.mutate(userId, { onSuccess: () => form.reset() })
  }

  return (
    <section className="flex flex-col gap-3" aria-label={`Members of ${playgroup.name}`}>
      <h3 className="font-semibold">
        Members{" "}
        <span className="text-base-content/60 font-normal">({playgroup.members.length})</span>
      </h3>
      {playgroup.members.length === 0 ? (
        <p className="text-base-content/60 text-sm">No members yet.</p>
      ) : (
        <ul className="flex flex-wrap gap-2" aria-label={`${playgroup.name} members`}>
          {playgroup.members.map((member) => (
            <li
              key={member.id}
              className="bg-base-100 border-base-300 flex items-center gap-1 rounded-full border py-1 pr-1 pl-3 text-sm"
            >
              <span className="font-medium">{member.display_name}</span>
              <span className="text-base-content/60">@{member.username}</span>
              {member.role === "admin" && <span className="badge badge-ghost badge-xs">admin</span>}
              <button
                type="button"
                className="btn btn-ghost btn-circle btn-xs"
                aria-label={`Remove ${member.display_name} from ${playgroup.name}`}
                disabled={remove.isPending}
                onClick={() => setPendingRemove(member)}
              >
                <X className="size-3.5" />
              </button>
            </li>
          ))}
        </ul>
      )}
      <form onSubmit={submitAdd} className="flex flex-col gap-2 sm:flex-row sm:items-end">
        <label className="form-control flex min-w-0 flex-1 flex-col gap-1 sm:max-w-sm">
          <span className="text-sm font-medium">Add a member</span>
          <select
            name="user_id"
            className="select w-full"
            defaultValue=""
            disabled={candidates.length === 0 || add.isPending}
          >
            <option value="" disabled>
              {candidates.length === 0 ? "Everyone is already a member" : "Choose an account"}
            </option>
            {candidates.map((user) => (
              <option key={user.id} value={user.id}>
                {user.display_name} (@{user.username})
              </option>
            ))}
          </select>
        </label>
        <button
          type="submit"
          className="btn btn-outline btn-sm sm:btn-md"
          disabled={candidates.length === 0 || add.isPending}
        >
          <UserPlus className="size-4" /> {add.isPending ? "Adding…" : "Add"}
        </button>
      </form>
      {error && !isSudoRequired(error) && (
        <p role="alert" className="text-error text-sm">
          {errorMessage(error, "user_id") || errorMessage(error)}
        </p>
      )}
      <SudoPrompt
        error={error}
        onSuccess={() => {
          if (add.error && add.variables) add.mutate(add.variables)
          else if (remove.error && remove.variables) remove.mutate(remove.variables)
        }}
      />
      <ConfirmDialog
        open={pendingRemove !== null}
        onOpenChange={(open) => !open && setPendingRemove(null)}
        title={`Remove ${pendingRemove?.display_name ?? "member"} from ${playgroup.name}?`}
        confirmLabel="Remove member"
        destructive
        onConfirm={() => pendingRemove && remove.mutate(pendingRemove.id)}
      >
        {pendingRemove?.id === currentUser.id
          ? "You will no longer see this playgroup. Your player and games stay, and an administrator can add you back."
          : "They will no longer see this playgroup. Their player and games stay, and they can be added back later."}
      </ConfirmDialog>
    </section>
  )
}

function hasFieldErrors(error: unknown) {
  return Boolean(errorMessage(error, "name") || errorMessage(error, "discord_guild_id"))
}

function PlaygroupFields({ error, defaults }: { error: unknown; defaults?: AdminPlaygroup }) {
  const nameError = errorMessage(error, "name")
  const guildError = errorMessage(error, "discord_guild_id")
  return (
    <div className="grid gap-4 sm:grid-cols-2">
      <label className="form-control flex flex-col gap-2">
        <span className="text-sm font-medium">Name</span>
        <input
          name="name"
          className="input w-full"
          defaultValue={defaults?.name ?? ""}
          required
          maxLength={80}
          aria-invalid={nameError ? true : undefined}
        />
        {nameError && <span className="text-error text-sm">{nameError}</span>}
      </label>
      <label className="form-control flex flex-col gap-2">
        <span className="text-sm font-medium">Discord server ID (optional)</span>
        <input
          name="discord_guild_id"
          className="input w-full font-mono"
          defaultValue={defaults?.discord_guild_id ?? ""}
          inputMode="numeric"
          placeholder="Games reported from this server are recorded here"
          aria-invalid={guildError ? true : undefined}
        />
        {guildError && <span className="text-error text-sm">{guildError}</span>}
      </label>
    </div>
  )
}
