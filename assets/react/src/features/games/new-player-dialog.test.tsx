import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vite-plus/test"
import { NewPlayerButton } from "./new-player-dialog"

const navigate = vi.hoisted(() => vi.fn())

vi.mock("@tanstack/react-router", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@tanstack/react-router")>()),
  useNavigate: () => navigate,
}))

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
  navigate.mockReset()
})

let sent: unknown

function renderButton(respond: () => Response) {
  const fetch = vi.fn(async (_input: RequestInfo | URL, init?: RequestInit) => {
    if (typeof init?.body === "string") sent = JSON.parse(init.body)
    return respond()
  })
  vi.stubGlobal("fetch", fetch)
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  })
  render(
    <QueryClientProvider client={queryClient}>
      <NewPlayerButton />
    </QueryClientProvider>,
  )
  fireEvent.click(screen.getByRole("button", { name: "Add player" }))
  return fetch
}

const json = (status: number, body: unknown) =>
  new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } })

describe("new player dialog", () => {
  it("creates the player with a trimmed name and opens their page", async () => {
    const fetch = renderButton(() =>
      json(201, { data: { id: 12, name: "Cara", avatar_url: null, user_id: null } }),
    )
    fireEvent.change(screen.getByRole("textbox", { name: "Name" }), {
      target: { value: "  Cara  " },
    })
    fireEvent.click(screen.getAllByRole("button", { name: "Add player" }).at(-1)!)

    await waitFor(() =>
      expect(navigate).toHaveBeenCalledWith({
        to: "/players/$playerId",
        params: { playerId: "12" },
      }),
    )
    const [url, init] = fetch.mock.calls[0] ?? []
    expect(url).toBe("/api/players")
    expect(init?.method).toBe("POST")
    expect(sent).toEqual({ name: "Cara" })
  })

  it("shows the name error when the player already exists", async () => {
    renderButton(() => json(422, { errors: { name: ["has already been taken"] } }))
    fireEvent.change(screen.getByRole("textbox", { name: "Name" }), {
      target: { value: "Alice" },
    })
    fireEvent.click(screen.getAllByRole("button", { name: "Add player" }).at(-1)!)

    expect((await screen.findByRole("alert")).textContent).toBe("Name has already been taken")
    expect(navigate).not.toHaveBeenCalled()
  })
})
