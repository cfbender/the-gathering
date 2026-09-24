import { useMutation, useQueryClient } from "@tanstack/react-query"
import { createFileRoute } from "@tanstack/react-router"
import type { FormEvent } from "react"
import { KeyRound, Library, UserRound } from "lucide-react"
import { PageHeader } from "@/components/app-shell"
import { AppearanceSection } from "@/components/appearance-section"
import { SudoPrompt } from "@/components/sudo-prompt"
import { invalidateGameRelated } from "@/features/games/games"
import { api } from "@/lib/api"
import { errorMessage, requireUser, useCurrentUser } from "@/lib/auth"
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
  const routeUser = Route.useRouteContext()
  // Prefer the live session so saves (for example storing an API key) update the form state.
  const user = useCurrentUser().data ?? routeUser
  const queryClient = useQueryClient()
  const profile = useMutation({
    mutationFn: async (values: {
      display_name: string
      moxfield_username: string
      archidekt_username: string
      manavault_url: string
      manavault_api_key?: string | null
    }) =>
      (
        await api<Data<User>>("/api/session/user", {
          method: "PATCH",
          body: JSON.stringify({ user: values }),
        })
      ).data,
    onSuccess: (updated) => {
      queryClient.setQueryData(["session"], updated)
      void queryClient.invalidateQueries({ queryKey: ["remote-decks"] })
      // A display name change also renames the user's linked player.
      void invalidateGameRelated(queryClient)
    },
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
    const form = event.currentTarget
    const apiKeyInput = form.elements.namedItem("manavault_api_key") as HTMLInputElement | null
    profile.mutate(
      {
        display_name: formValue(form, "display_name"),
        moxfield_username: formValue(form, "moxfield_username"),
        archidekt_username: formValue(form, "archidekt_username"),
        manavault_url: formValue(form, "manavault_url"),
        manavault_api_key: formValue(form, "manavault_api_key"),
      },
      {
        onSuccess: () => {
          if (apiKeyInput) apiKeyInput.value = ""
        },
      },
    )
  }

  function removeApiKey() {
    profile.mutate({
      display_name: user.display_name,
      moxfield_username: user.moxfield_username ?? "",
      archidekt_username: user.archidekt_username ?? "",
      manavault_url: user.manavault_url ?? "",
      manavault_api_key: null,
    })
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
    <div className="mx-auto flex min-w-0 max-w-2xl flex-col gap-6">
      <PageHeader
        eyebrow="Account"
        title="Settings"
        description={`Manage your profile${user.has_password ? ", password," : ""} and appearance.`}
      />

      <AppearanceSection />

      <form className="card bg-base-200 border-base-300 min-w-0 border" onSubmit={updateProfile}>
        <div className="card-body min-w-0 gap-4">
          <h2 className="card-title text-lg">
            <UserRound className="size-5" /> Profile
          </h2>
          <label className="fieldset min-w-0">
            <span className="fieldset-legend">Username</span>
            <input className="input min-w-0 w-full" value={user.username} disabled />
          </label>
          <label className="fieldset min-w-0">
            <span className="fieldset-legend">Display name</span>
            <input
              name="display_name"
              className="input min-w-0 w-full"
              defaultValue={user.display_name}
              required
            />
            {errorMessage(profile.error, "display_name") && (
              <span className="label text-error">
                {errorMessage(profile.error, "display_name")}
              </span>
            )}
          </label>
          <div className="divider my-0" />
          <div>
            <h3 className="flex items-center gap-2 font-semibold">
              <Library className="size-4" /> Deck hosts
            </h3>
            <p className="text-base-content/60 mt-1 text-sm">
              Add your public deck identities for quick access while logging games.
            </p>
          </div>
          <label className="fieldset min-w-0">
            <span className="fieldset-legend">Moxfield username</span>
            <input
              name="moxfield_username"
              className="input min-w-0 w-full"
              defaultValue={user.moxfield_username ?? ""}
              placeholder="your-username"
            />
            {errorMessage(profile.error, "moxfield_username") && (
              <span className="label text-error">
                {errorMessage(profile.error, "moxfield_username")}
              </span>
            )}
          </label>
          <label className="fieldset min-w-0">
            <span className="fieldset-legend">Archidekt username</span>
            <input
              name="archidekt_username"
              className="input min-w-0 w-full"
              defaultValue={user.archidekt_username ?? ""}
              placeholder="your-username"
            />
            {errorMessage(profile.error, "archidekt_username") && (
              <span className="label text-error">
                {errorMessage(profile.error, "archidekt_username")}
              </span>
            )}
          </label>
          <label className="fieldset min-w-0">
            <span className="fieldset-legend">ManaVault instance URL</span>
            <input
              name="manavault_url"
              type="url"
              className="input min-w-0 w-full"
              defaultValue={user.manavault_url ?? ""}
              placeholder="https://vault.example.com"
            />
            {errorMessage(profile.error, "manavault_url") && (
              <span className="label text-error">
                {errorMessage(profile.error, "manavault_url")}
              </span>
            )}
            <span className="label text-base-content/60 whitespace-normal">
              Enter only the instance origin (for example, https://vault.example.com). Private
              network hosts must be allowed by the server operator.
            </span>
          </label>
          <label className="fieldset min-w-0">
            <span className="fieldset-legend">ManaVault API key</span>
            <div className="flex min-w-0 gap-2">
              <input
                name="manavault_api_key"
                type="password"
                autoComplete="off"
                className="input min-w-0 flex-1"
                placeholder={
                  user.has_manavault_api_key ? "Saved — enter a new key to replace it" : "mv_…"
                }
              />
              {user.has_manavault_api_key && (
                <button
                  type="button"
                  className="btn btn-ghost btn-sm self-center"
                  onClick={removeApiKey}
                  disabled={profile.isPending}
                >
                  Remove key
                </button>
              )}
            </div>
            <span className="label text-base-content/60 whitespace-normal">
              Create a personal API key in your ManaVault account settings to list your decks here.
              Leave blank to keep the saved key.
            </span>
            {errorMessage(profile.error, "manavault_api_key") && (
              <span className="label text-error">
                {errorMessage(profile.error, "manavault_api_key")}
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

      {user.has_password && (
        <SudoPrompt error={password.error} onSuccess={() => password.reset()} />
      )}

      {user.has_password && (
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
      )}
    </div>
  )
}
