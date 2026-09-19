import { Link, createFileRoute, useNavigate } from "@tanstack/react-router"
import type { FormEvent } from "react"
import { UserPlus } from "lucide-react"
import { errorMessage, registrationQueryOptions, useRegister } from "@/lib/auth"
import { formValue } from "@/lib/form"

export const Route = createFileRoute("/register")({
  loader: ({ context }) => context.queryClient.ensureQueryData(registrationQueryOptions),
  component: RegisterPage,
})

function RegisterPage() {
  const status = Route.useLoaderData()
  const register = useRegister()
  const navigate = useNavigate()

  function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const form = event.currentTarget
    register.mutate(
      {
        username: formValue(form, "username"),
        display_name: formValue(form, "display_name"),
        password: formValue(form, "password"),
      },
      { onSuccess: () => void navigate({ to: "/" }) },
    )
  }

  if (!status.allowed) {
    return (
      <section className="mx-auto max-w-md py-16 text-center">
        <h1 className="text-2xl font-bold">Registration is closed</h1>
        <p className="text-base-content/70 mt-2">
          Ask your server admin to create an account for you.
        </p>
        <Link to="/login" search={{ returnTo: "/" }} className="btn btn-primary mt-6">
          Back to sign in
        </Link>
      </section>
    )
  }

  return (
    <section className="mx-auto max-w-md py-8 sm:py-12">
      <div className="card bg-base-200 border-base-300 border shadow-sm">
        <form className="card-body gap-4" onSubmit={submit}>
          <div>
            <span className="bg-primary text-primary-content mb-4 grid size-10 place-items-center rounded-lg">
              <UserPlus className="size-5" aria-hidden="true" />
            </span>
            <h1 className="card-title text-2xl">
              {status.bootstrap ? "Set up your server" : "Create your account"}
            </h1>
            <p className="text-base-content/70 mt-1 text-sm">
              {status.bootstrap
                ? "This first account will be the server administrator."
                : "Join your playgroup on The Gathering."}
            </p>
          </div>

          {register.error && !errorMessage(register.error, "username") && (
            <div className="alert alert-error text-sm">{errorMessage(register.error)}</div>
          )}

          <label className="fieldset">
            <span className="fieldset-legend">Username</span>
            <input
              name="username"
              autoComplete="username"
              className="input w-full"
              required
              autoFocus
            />
            {errorMessage(register.error, "username") && (
              <span className="label text-error">{errorMessage(register.error, "username")}</span>
            )}
          </label>
          <label className="fieldset">
            <span className="fieldset-legend">Display name</span>
            <input name="display_name" autoComplete="name" className="input w-full" required />
            {errorMessage(register.error, "display_name") && (
              <span className="label text-error">
                {errorMessage(register.error, "display_name")}
              </span>
            )}
          </label>
          <label className="fieldset">
            <span className="fieldset-legend">Password</span>
            <input
              name="password"
              type="password"
              minLength={12}
              maxLength={72}
              autoComplete="new-password"
              className="input w-full"
              required
            />
            <span className="label">At least 12 characters</span>
            {errorMessage(register.error, "password") && (
              <span className="label text-error">{errorMessage(register.error, "password")}</span>
            )}
          </label>
          <button type="submit" className="btn btn-primary" disabled={register.isPending}>
            {register.isPending ? "Creating account…" : "Create account"}
          </button>
        </form>
      </div>
    </section>
  )
}
