export class ApiError extends Error {
  constructor(
    readonly status: number,
    message: string,
  ) {
    super(message)
    this.name = "ApiError"
  }
}

function csrfToken(): string | null {
  return document.querySelector<HTMLMetaElement>('meta[name="csrf-token"]')?.content ?? null
}

/**
 * Thin fetch wrapper for the Phoenix JSON API: same-origin cookies, JSON
 * bodies, and the CSRF token from the SPA shell on mutating requests.
 */
export async function api<T>(path: string, init: RequestInit = {}): Promise<T> {
  const headers = new Headers(init.headers)
  headers.set("accept", "application/json")
  if (init.body !== undefined) headers.set("content-type", "application/json")

  const method = (init.method ?? "GET").toUpperCase()
  const token = csrfToken()
  if (token && method !== "GET" && method !== "HEAD") headers.set("x-csrf-token", token)

  const response = await fetch(path, { ...init, headers, credentials: "same-origin" })
  if (!response.ok) {
    throw new ApiError(response.status, `${method} ${path} failed with ${response.status}`)
  }
  return (await response.json()) as T
}
