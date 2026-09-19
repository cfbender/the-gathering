/** Field errors as rendered by `TheGatheringWeb.ChangesetJSON`: `{ field: ["message"] }`. */
export type FieldErrors = Record<string, string | string[] | Record<string, unknown> | undefined>

/** Error body from the Phoenix API: `{ errors: { detail: "Not Found" } }` or field errors. */
export interface ApiErrorBody {
  errors: FieldErrors
}

export class ApiError extends Error {
  readonly errors: ApiErrorBody["errors"]

  constructor(
    readonly status: number,
    message: string,
    errors: ApiErrorBody["errors"] = {},
  ) {
    super(message)
    this.name = "ApiError"
    this.errors = errors
  }

  /** Messages for a single field, empty when the field is valid or unknown. */
  fieldErrors(field: string): string[] {
    const value = this.errors[field]
    return Array.isArray(value) ? value : []
  }

  /** The top-level `errors.detail` message, or `null` when the response only had field errors. */
  get detail(): string | null {
    return typeof this.errors.detail === "string" ? this.errors.detail : null
  }
}

function csrfToken(): string | null {
  return document.querySelector<HTMLMetaElement>('meta[name="csrf-token"]')?.content ?? null
}

async function readErrorBody(response: Response): Promise<ApiErrorBody["errors"]> {
  try {
    const body = (await response.json()) as Partial<ApiErrorBody>
    return body.errors ?? {}
  } catch {
    return {}
  }
}

/**
 * Thin fetch wrapper for the Phoenix JSON API: same-origin cookies, JSON
 * bodies, and the CSRF token from the SPA shell on mutating requests.
 *
 * Non-2xx responses reject with an `ApiError` carrying the parsed `errors`
 * object so forms can show per-field messages.
 */
export async function api<T>(path: string, init: RequestInit = {}): Promise<T> {
  const headers = new Headers(init.headers)
  headers.set("accept", "application/json")
  if (init.body !== undefined) headers.set("content-type", "application/json")

  const method = (init.method ?? "GET").toUpperCase()
  const token = csrfToken()
  if (token && method !== "GET" && method !== "HEAD") headers.set("x-csrf-token", token)

  const response = await fetch(path, { ...init, headers, credentials: "same-origin" })
  const refreshedToken = response.headers.get("x-csrf-token")
  const tokenMeta = document.querySelector<HTMLMetaElement>('meta[name="csrf-token"]')
  if (refreshedToken && tokenMeta) tokenMeta.content = refreshedToken

  if (!response.ok) {
    const errors = await readErrorBody(response)
    const detail = typeof errors.detail === "string" ? errors.detail : response.statusText
    throw new ApiError(response.status, `${method} ${path}: ${detail}`, errors)
  }
  if (response.status === 204) return undefined as T
  return (await response.json()) as T
}
