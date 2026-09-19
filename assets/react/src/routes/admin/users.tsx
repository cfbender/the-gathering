import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { createFileRoute } from "@tanstack/react-router"
import type { FormEvent } from "react"
import { KeyRound, Shield, UserPlus, Users } from "lucide-react"
import { SudoPrompt } from "@/components/sudo-prompt"
import { api } from "@/lib/api"
import { errorMessage, isSudoRequired, requireAdmin } from "@/lib/auth"
import type { User } from "@/lib/auth"
import { formValue } from "@/lib/form"

interface Data<T> {
  data: T
}

interface AdminSettings {
  registration_enabled: boolean
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
  const settings = useQuery({
    queryKey: ["admin", "settings"],
    queryFn: async () => (await api<Data<AdminSettings>>("/api/admin/settings")).data,
  })
  const createUser = useMutation({
    mutationFn: (user: {
      username: string
      display_name: string
      password: string
      role: string
    }) => api<Data<User>>("/api/admin/users", { method: "POST", body: JSON.stringify({ user }) }),
    onSuccess: () => void queryClient.invalidateQueries({ queryKey: ["admin", "users"] }),
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
  const sudoError = users.error ?? settings.error ?? createUser.error ?? toggleRegistration.error

  function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const form = event.currentTarget
    createUser.mutate(
      {
        username: formValue(form, "username"),
        display_name: formValue(form, "display_name"),
        password: formValue(form, "password"),
        role: formValue(form, "role"),
      },
      { onSuccess: () => form.reset() },
    )
  }

  if (isSudoRequired(sudoError)) {
    return (
      <div className="mx-auto flex max-w-2xl flex-col gap-6">
        <div>
          <p className="text-primary text-sm font-semibold uppercase">Administration</p>
          <h1 className="text-3xl font-bold tracking-tight">Users</h1>
          <p className="text-base-content/70 mt-1">
            Confirm your identity to manage server access.
          </p>
        </div>
        <SudoPrompt
          error={sudoError}
          onSuccess={() => void queryClient.invalidateQueries({ queryKey: ["admin"] })}
        />
      </div>
    )
  }

  return (
    <div className="flex flex-col gap-6">
      <div className="flex flex-col justify-between gap-4 sm:flex-row sm:items-end">
        <div>
          <p className="text-primary text-sm font-semibold uppercase">Administration</p>
          <h1 className="text-3xl font-bold tracking-tight">Users</h1>
          <p className="text-base-content/70 mt-1">Accounts, access, and server registration.</p>
        </div>
        <label className="bg-base-200 border-base-300 flex cursor-pointer items-center gap-3 rounded-lg border px-4 py-3">
          <span className="text-sm font-medium">Open registration</span>
          <input
            type="checkbox"
            className="toggle toggle-primary"
            checked={settings.data?.registration_enabled ?? false}
            disabled={!settings.data || toggleRegistration.isPending}
            onChange={(event) => toggleRegistration.mutate(event.target.checked)}
          />
        </label>
      </div>

      <form className="card bg-base-200 border-base-300 border" onSubmit={submit}>
        <div className="card-body gap-4">
          <h2 className="card-title text-lg">
            <UserPlus className="size-5" /> Add a user
          </h2>
          {createUser.error && (
            <div className="alert alert-error text-sm">{errorMessage(createUser.error)}</div>
          )}
          <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
            <label className="fieldset">
              <span className="fieldset-legend">Username</span>
              <input name="username" className="input w-full" required />
              {errorMessage(createUser.error, "username") && (
                <span className="label text-error">
                  {errorMessage(createUser.error, "username")}
                </span>
              )}
            </label>
            <label className="fieldset">
              <span className="fieldset-legend">Display name</span>
              <input name="display_name" className="input w-full" required />
            </label>
            <label className="fieldset">
              <span className="fieldset-legend">Temporary password</span>
              <input
                name="password"
                type="password"
                minLength={12}
                maxLength={72}
                className="input w-full"
                required
              />
            </label>
            <label className="fieldset">
              <span className="fieldset-legend">Role</span>
              <select name="role" className="select w-full" defaultValue="member">
                <option value="member">Member</option>
                <option value="admin">Admin</option>
              </select>
            </label>
          </div>
          <div className="card-actions justify-end">
            <button className="btn btn-primary btn-sm" disabled={createUser.isPending}>
              Create user
            </button>
          </div>
        </div>
      </form>

      <SudoPrompt
        error={sudoError}
        onSuccess={() => {
          createUser.reset()
          toggleRegistration.reset()
          void queryClient.invalidateQueries({ queryKey: ["admin"] })
        }}
      />

      <section aria-labelledby="accounts-heading">
        <h2 id="accounts-heading" className="mb-3 flex items-center gap-2 text-lg font-semibold">
          <Users className="size-5" /> Accounts
        </h2>
        {users.isPending && <div className="skeleton h-32 w-full" />}
        {users.error && <div className="alert alert-error">{errorMessage(users.error)}</div>}
        <div className="grid gap-3">
          {users.data?.map((user) => (
            <UserCard key={user.id} user={user} />
          ))}
        </div>
      </section>
    </div>
  )
}

function UserCard({ user }: { user: User }) {
  const queryClient = useQueryClient()
  const update = useMutation({
    mutationFn: (attrs: { display_name?: string; role?: string; disabled?: boolean }) =>
      api<Data<User>>(`/api/admin/users/${user.id}`, {
        method: "PATCH",
        body: JSON.stringify({ user: attrs }),
      }),
    onSuccess: () => void queryClient.invalidateQueries({ queryKey: ["admin", "users"] }),
  })
  const resetPassword = useMutation({
    mutationFn: (password: string) =>
      api<Data<User>>(`/api/admin/users/${user.id}/password`, {
        method: "PATCH",
        body: JSON.stringify({ password }),
      }),
  })

  function saveName(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    update.mutate({ display_name: formValue(event.currentTarget, "display_name") })
  }

  function reset(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const form = event.currentTarget
    resetPassword.mutate(formValue(form, "password"), {
      onSuccess: () => form.reset(),
    })
  }

  return (
    <div className="card bg-base-200 border-base-300 border">
      <div className="card-body gap-4 p-4 sm:p-5">
        <div className="flex flex-col justify-between gap-3 sm:flex-row sm:items-center">
          <div className="min-w-0">
            <div className="flex flex-wrap items-center gap-2">
              <h3 className="truncate font-semibold">{user.display_name}</h3>
              <span className="badge badge-outline badge-sm gap-1">
                <Shield className="size-3" /> {user.role}
              </span>
              {user.disabled && <span className="badge badge-error badge-sm">disabled</span>}
            </div>
            <p className="text-base-content/60 text-sm">@{user.username}</p>
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
          </div>
        </div>
        {update.error && (
          <div className="alert alert-error py-2 text-sm">
            {errorMessage(update.error, "role") ?? errorMessage(update.error)}
          </div>
        )}
        <SudoPrompt
          error={update.error ?? resetPassword.error}
          onSuccess={() => {
            update.reset()
            resetPassword.reset()
            void queryClient.invalidateQueries({ queryKey: ["admin", "users"] })
          }}
        />
        <div className="grid gap-3 lg:grid-cols-2">
          <form className="join" onSubmit={saveName}>
            <input
              name="display_name"
              aria-label={`Display name for ${user.username}`}
              defaultValue={user.display_name}
              className="input input-sm join-item min-w-0 flex-1"
              required
            />
            <button className="btn btn-sm join-item" disabled={update.isPending}>
              Save name
            </button>
          </form>
          <form className="join" onSubmit={reset}>
            <label className="input input-sm join-item min-w-0 flex-1">
              <KeyRound className="size-4 opacity-60" />
              <input
                name="password"
                type="password"
                minLength={12}
                maxLength={72}
                placeholder="New password"
                aria-label={`New password for ${user.username}`}
                required
              />
            </label>
            <button className="btn btn-sm join-item" disabled={resetPassword.isPending}>
              Reset
            </button>
          </form>
        </div>
        {resetPassword.isSuccess && <p className="text-success text-sm">Password reset.</p>}
        {resetPassword.error && (
          <p className="text-error text-sm">
            {errorMessage(resetPassword.error, "password") ?? errorMessage(resetPassword.error)}
          </p>
        )}
      </div>
    </div>
  )
}
