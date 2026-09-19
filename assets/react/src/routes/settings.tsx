import { useMutation, useQueryClient } from "@tanstack/react-query"
import { createFileRoute } from "@tanstack/react-router"
import type { FormEvent } from "react"
import { KeyRound, UserRound } from "lucide-react"
import { SudoPrompt } from "@/components/sudo-prompt"
import { api } from "@/lib/api"
import { errorMessage, requireUser } from "@/lib/auth"
import type { User } from "@/lib/auth"
import { formValue } from "@/lib/form"

interface Data<T> {
  data: T
}

export const Route = createFileRoute("/settings")({
  beforeLoad: ({ context, location }) => requireUser(context.queryClient, location.href),
  component: SettingsPage,
})

function SettingsPage() {
  const user = Route.useRouteContext()
  const queryClient = useQueryClient()
  const profile = useMutation({
    mutationFn: async (display_name: string) =>
      (
        await api<Data<User>>("/api/session/user", {
          method: "PATCH",
          body: JSON.stringify({ user: { display_name } }),
        })
      ).data,
    onSuccess: (updated) => queryClient.setQueryData(["session"], updated),
  })
  const password = useMutation({
    mutationFn: (values: { password: string; password_confirmation: string }) =>
      api<Data<User>>("/api/session/password", {
        method: "PATCH",
        body: JSON.stringify(values),
      }),
  })

  function updateProfile(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    profile.mutate(formValue(event.currentTarget, "display_name"))
  }

  function updatePassword(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const form = event.currentTarget
    password.mutate(
      {
        password: formValue(form, "password"),
        password_confirmation: formValue(form, "password_confirmation"),
      },
      { onSuccess: () => form.reset() },
    )
  }

  return (
    <div className="mx-auto flex max-w-2xl flex-col gap-6">
      <div>
        <h1 className="text-3xl font-bold tracking-tight">Settings</h1>
        <p className="text-base-content/70 mt-1">Manage your profile and password.</p>
      </div>

      <form className="card bg-base-200 border-base-300 border" onSubmit={updateProfile}>
        <div className="card-body gap-4">
          <h2 className="card-title text-lg">
            <UserRound className="size-5" /> Profile
          </h2>
          <label className="fieldset">
            <span className="fieldset-legend">Username</span>
            <input className="input w-full" value={user.username} disabled />
          </label>
          <label className="fieldset">
            <span className="fieldset-legend">Display name</span>
            <input
              name="display_name"
              className="input w-full"
              defaultValue={user.display_name}
              required
            />
            {errorMessage(profile.error, "display_name") && (
              <span className="label text-error">
                {errorMessage(profile.error, "display_name")}
              </span>
            )}
          </label>
          <div className="card-actions items-center justify-end">
            {profile.isSuccess && <span className="text-success text-sm">Saved</span>}
            <button className="btn btn-primary btn-sm" disabled={profile.isPending}>
              Save profile
            </button>
          </div>
        </div>
      </form>

      <SudoPrompt error={password.error} onSuccess={() => password.reset()} />

      <form className="card bg-base-200 border-base-300 border" onSubmit={updatePassword}>
        <div className="card-body gap-4">
          <h2 className="card-title text-lg">
            <KeyRound className="size-5" /> Change password
          </h2>
          <label className="fieldset">
            <span className="fieldset-legend">New password</span>
            <input
              name="password"
              type="password"
              minLength={12}
              maxLength={72}
              autoComplete="new-password"
              className="input w-full"
              required
            />
            {errorMessage(password.error, "password") && (
              <span className="label text-error">{errorMessage(password.error, "password")}</span>
            )}
          </label>
          <label className="fieldset">
            <span className="fieldset-legend">Confirm new password</span>
            <input
              name="password_confirmation"
              type="password"
              minLength={12}
              maxLength={72}
              autoComplete="new-password"
              className="input w-full"
              required
            />
            {errorMessage(password.error, "password_confirmation") && (
              <span className="label text-error">
                {errorMessage(password.error, "password_confirmation")}
              </span>
            )}
          </label>
          <div className="card-actions items-center justify-end">
            {password.isSuccess && <span className="text-success text-sm">Password updated</span>}
            <button className="btn btn-primary btn-sm" disabled={password.isPending}>
              Update password
            </button>
          </div>
        </div>
      </form>
    </div>
  )
}
