import { createFileRoute } from "@tanstack/react-router"
import { DiscordIcon } from "@/components/discord-icon"
import { api } from "@/lib/api"
import type { RegistrationStatus } from "@/lib/auth"

export const Route = createFileRoute("/invite")({
  loader: async () => {
    // Fragments never reach the server or Referer headers. Remove the secret from
    // browser history before exchanging it for a signed, session-bound digest.
    const token = new URLSearchParams(window.location.hash.slice(1)).get("token")
    window.history.replaceState(window.history.state, "", "/invite")
    const [invite, registration] = await Promise.all([
      api<{ data: { valid: boolean } }>(
        "/api/registration-invite",
        token === null
          ? {}
          : {
              method: "POST",
              body: JSON.stringify({ token }),
            },
      ),
      api<{ data: RegistrationStatus }>("/api/registration"),
    ])
    return { valid: invite.data.valid, registration: registration.data }
  },
  component: InvitePage,
})

function InvitePage() {
  const { valid, registration } = Route.useLoaderData()
  const canJoin = valid && !registration.bootstrap

  return (
    <section className="mx-auto max-w-md py-8 sm:py-16">
      <div className="card bg-base-200 border-base-300 border shadow-sm">
        <div className="card-body gap-5">
          <h1 className="card-title text-2xl">
            {canJoin ? "You're invited" : "Invitation unavailable"}
          </h1>
          <p className="text-base-content/70 text-sm">
            {canJoin
              ? "Join this playgroup with your Discord account. This invitation lets you sign up even when open registration is off."
              : "This link is invalid or has been replaced. Ask the administrator for a new sign-up link."}
          </p>
          {canJoin &&
            (registration.discord_configured ? (
              <a
                href="/auth/discord"
                className="btn bg-discord hover:bg-discord-hover border-discord w-full text-white"
              >
                <DiscordIcon className="size-5" /> Join with Discord
              </a>
            ) : (
              <p className="alert alert-warning text-sm">
                Discord sign-in is not configured. Ask your server administrator for help.
              </p>
            ))}
          <a href="/login" className="link text-sm">
            Already a member? Sign in
          </a>
        </div>
      </div>
    </section>
  )
}
