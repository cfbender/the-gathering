import { createFileRoute, useNavigate } from "@tanstack/react-router"
import { useEffect, useState, type FormEvent } from "react"
import { LogIn } from "lucide-react"
import { DiscordIcon } from "@/components/discord-icon"
import { errorMessage, registrationQueryOptions, safeReturnTo, useLogin } from "@/lib/auth"
import { formValue } from "@/lib/form"

export const Route = createFileRoute("/login")({
  validateSearch: (search: Record<string, unknown>) => ({
    returnTo: safeReturnTo(search.returnTo),
    error: typeof search.error === "string" ? search.error : undefined,
  }),
  loader: ({ context }) => context.queryClient.ensureQueryData(registrationQueryOptions),
  component: LoginPage,
})

function LoginPage() {
  const login = useLogin()
  const navigate = useNavigate()
  const { returnTo, error } = Route.useSearch()
  const registration = Route.useLoaderData()
  const oauthError = useOneShotDiscordError(error, returnTo)

  function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const form = event.currentTarget
    login.mutate(
      { username: formValue(form, "username"), password: formValue(form, "password") },
      { onSuccess: () => void navigate({ to: returnTo }) },
    )
  }

  return (
    <section className="mx-auto max-w-md py-8 sm:py-16">
      <div className="card bg-base-200 border-base-300 border shadow-sm">
        <div className="card-body gap-5">
          <div>
            <span className="bg-primary text-primary-content mb-4 grid size-10 place-items-center rounded-lg">
              <LogIn className="size-5" aria-hidden="true" />
            </span>
            <h1 className="card-title text-2xl">Welcome back</h1>
            <p className="text-base-content/70 mt-1 text-sm">Sign in to your playgroup.</p>
          </div>

          {oauthError && <div className="alert alert-error text-sm">{oauthError}</div>}
          {login.error && (
            <div className="alert alert-error text-sm">{errorMessage(login.error)}</div>
          )}

          {registration.discord_configured && !registration.bootstrap && (
            <a
              href={`/auth/discord?${new URLSearchParams({ returnTo }).toString()}`}
              className="btn bg-discord hover:bg-discord-hover border-discord hover:border-discord-hover w-full text-white shadow-sm"
            >
              <DiscordIcon className="size-5" />
              Continue with Discord
            </a>
          )}

          {!registration.discord_configured && !registration.bootstrap && (
            <div className="alert alert-warning text-sm">
              Discord sign-in is not configured. Ask your server administrator for help.
            </div>
          )}

          {registration.bootstrap && (
            <a href="/register" className="btn btn-primary w-full">
              Set up administrator account
            </a>
          )}

          <details className="collapse-arrow bg-base-100 border-base-300 collapse border">
            <summary className="collapse-title py-3 font-medium">Administrator sign in</summary>
            <form className="collapse-content grid gap-4" onSubmit={submit}>
              <label className="fieldset">
                <span className="fieldset-legend">Username</span>
                <input name="username" autoComplete="username" className="input w-full" required />
              </label>
              <label className="fieldset">
                <span className="fieldset-legend">Password</span>
                <input
                  name="password"
                  type="password"
                  autoComplete="current-password"
                  className="input w-full"
                  required
                />
              </label>
              <button type="submit" className="btn" disabled={login.isPending}>
                {login.isPending ? "Signing in…" : "Sign in as administrator"}
              </button>
            </form>
          </details>
        </div>
      </div>
    </section>
  )
}

/**
 * The OAuth callback reports failures through `?error=`. Keep the message for this
 * visit but drop it from the URL, so reloading or sharing the page after the
 * administrator has fixed the cause does not keep showing a stale error.
 */
function useOneShotDiscordError(error: string | undefined, returnTo: string) {
  const navigate = useNavigate()
  const [message, setMessage] = useState(() => discordError(error))

  useEffect(() => {
    if (!error) return
    setMessage(discordError(error))
    void navigate({ to: "/login", search: { returnTo, error: undefined }, replace: true })
  }, [error, returnTo, navigate])

  return message
}

function discordError(error?: string) {
  switch (error) {
    case "registration_closed":
      return "Registration is closed. Ask your administrator to enable new member registration, then try Continue with Discord again."
    case "account_disabled":
      return "This account is disabled. Ask your administrator for help."
    case "discord_sudo_mismatch":
      return "Reauthentication must use the Discord account already linked to this user."
    case "discord_sudo_unavailable":
      return "Discord reauthentication is not available for this account."
    case "discord_unavailable":
      return "Discord sign-in is not configured."
    case "discord_failed":
      return "Discord sign-in could not be completed. Please try again."
    default:
      return null
  }
}
