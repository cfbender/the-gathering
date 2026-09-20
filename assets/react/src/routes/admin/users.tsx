import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { createFileRoute } from "@tanstack/react-router"
import { PageHeader } from "@/components/app-shell"
import type { FormEvent } from "react"
import { Shield, Users } from "lucide-react"
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
  const sudoError = users.error ?? settings.error ?? toggleRegistration.error

  if (isSudoRequired(sudoError)) {
    return (
      <div className="mx-auto flex max-w-2xl flex-col gap-6">
        <PageHeader
          eyebrow="Administration"
          title="Users"
          description="Confirm your identity to manage server access."
        />
        <SudoPrompt
          error={sudoError}
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
        description="Accounts, access, and server registration."
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
        error={sudoError}
        onSuccess={() => {
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
        <div className="grid grid-cols-[minmax(0,1fr)] gap-3">
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
          </div>
        </div>
        {update.error && (
          <div className="alert alert-error py-2 text-sm">
            {errorMessage(update.error, "role") ?? errorMessage(update.error)}
          </div>
        )}
        <SudoPrompt
          error={update.error}
          onSuccess={() => {
            update.reset()
            void queryClient.invalidateQueries({ queryKey: ["admin", "users"] })
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
      </div>
    </div>
  )
}
