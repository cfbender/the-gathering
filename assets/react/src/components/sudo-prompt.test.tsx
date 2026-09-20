import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vite-plus/test"
import { ApiError } from "@/lib/api"
import type { User } from "@/lib/auth"
import { SudoPrompt } from "./sudo-prompt"

const sudoRequired = new ApiError(403, "Reauthentication required", {
  code: "sudo_required",
})

function user(hasPassword: boolean): User {
  return {
    id: 1,
    username: "cody",
    display_name: "Cody",
    discord_id: hasPassword ? null : "123",
    avatar_url: null,
    moxfield_username: null,
    archidekt_username: null,
    manavault_url: null,
    has_manavault_api_key: false,
    has_password: hasPassword,
    role: "admin",
    disabled: false,
    inserted_at: "2026-09-20T00:00:00Z",
  }
}

function renderPrompt(currentUser: User, onSuccess = vi.fn()) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { staleTime: Infinity }, mutations: { retry: false } },
  })
  queryClient.setQueryData(["session"], currentUser)

  render(
    <QueryClientProvider client={queryClient}>
      <SudoPrompt error={sudoRequired} onSuccess={onSuccess} />
    </QueryClientProvider>,
  )

  return onSuccess
}

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
})

describe("SudoPrompt", () => {
  it("reauthenticates by password and invokes the intended-action retry", async () => {
    const fetch = vi
      .fn()
      .mockResolvedValue(
        new Response("{}", { status: 200, headers: { "content-type": "application/json" } }),
      )
    vi.stubGlobal("fetch", fetch)
    const retry = renderPrompt(user(true))

    fireEvent.change(screen.getByLabelText("Confirm your password"), {
      target: { value: "long-enough-password" },
    })
    fireEvent.click(screen.getByRole("button", { name: "Confirm password" }))

    await waitFor(() => expect(retry).toHaveBeenCalledOnce())
    expect(fetch).toHaveBeenCalledWith(
      "/api/session/sudo",
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({ password: "long-enough-password" }),
      }),
    )
  })

  it("keeps Discord reauthorization available for passwordless admins", () => {
    renderPrompt(user(false))

    const link = screen.getByRole("link", { name: "Continue with Discord" })
    expect(link.getAttribute("href")).toContain("sudo=1")
    expect(link.getAttribute("href")).toContain("returnTo=")
  })
})
