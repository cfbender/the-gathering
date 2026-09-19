import { Link, createFileRoute, useNavigate } from "@tanstack/react-router"
import type { FormEvent } from "react"
import { LogIn } from "lucide-react"
import { errorMessage, safeReturnTo, useLogin } from "@/lib/auth"
import { formValue } from "@/lib/form"

export const Route = createFileRoute("/login")({
  validateSearch: (search: Record<string, unknown>) => ({
    returnTo: safeReturnTo(search.returnTo),
  }),
  component: LoginPage,
})

function LoginPage() {
  const login = useLogin()
  const navigate = useNavigate()
  const { returnTo } = Route.useSearch()

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
        <form className="card-body gap-5" onSubmit={submit}>
          <div>
            <span className="bg-primary text-primary-content mb-4 grid size-10 place-items-center rounded-lg">
              <LogIn className="size-5" aria-hidden="true" />
            </span>
            <h1 className="card-title text-2xl">Welcome back</h1>
            <p className="text-base-content/70 mt-1 text-sm">Sign in to your playgroup.</p>
          </div>

          {login.error && (
            <div className="alert alert-error text-sm">{errorMessage(login.error)}</div>
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
          <button type="submit" className="btn btn-primary" disabled={login.isPending}>
            {login.isPending ? "Signing in…" : "Sign in"}
          </button>
          <p className="text-base-content/70 text-center text-sm">
            Need an account?{" "}
            <Link to="/register" className="link link-primary">
              Register
            </Link>
          </p>
        </form>
      </div>
    </section>
  )
}
