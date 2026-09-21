import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react"
import { afterEach, expect, it, vi } from "vite-plus/test"
import { FileImportPage } from "./import-page"

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
})

function mount() {
  const client = new QueryClient({ defaultOptions: { mutations: { retry: false } } })
  client.setQueryData(["session"], null)
  render(
    <QueryClientProvider client={client}>
      <FileImportPage />
    </QueryClientProvider>,
  )
}

const preview = {
  valid: true,
  revision: "revision-7",
  games: [],
  players: { create: [], matched: [] },
  decks: { create: [], matched: [] },
  errors: [],
  warnings: [],
  review: [],
}

it("commits the reviewed CSV and revision", async () => {
  const fetch = vi.fn((url: string | URL | Request) =>
    Promise.resolve(
      new Response(
        JSON.stringify({
          data:
            url === "/api/imports/csv/preview"
              ? preview
              : { created: 0, updated: 1, skipped: 0, game_ids: [] },
        }),
        { status: 200 },
      ),
    ),
  )
  vi.stubGlobal("fetch", fetch)
  mount()
  fireEvent.change(screen.getByLabelText("CSV contents"), { target: { value: "reviewed csv" } })
  fireEvent.click(screen.getByRole("button", { name: "Preview import" }))
  fireEvent.click(await screen.findByRole("button", { name: "Confirm import" }))
  await screen.findByText("Import complete")
  expect(fetch).toHaveBeenLastCalledWith(
    "/api/imports/csv",
    expect.objectContaining({
      body: JSON.stringify({ csv: "reviewed csv", revision: "revision-7" }),
    }),
  )
})

it("prevents confirmation when input changes while its preview is in flight", async () => {
  let resolve!: (response: Response) => void
  vi.stubGlobal(
    "fetch",
    vi.fn(
      () =>
        new Promise<Response>((done) => {
          resolve = done
        }),
    ),
  )
  mount()
  const input = screen.getByLabelText("CSV contents")
  fireEvent.change(input, { target: { value: "old csv" } })
  fireEvent.click(screen.getByRole("button", { name: "Preview import" }))
  fireEvent.change(input, { target: { value: "new csv" } })
  resolve(new Response(JSON.stringify({ data: preview }), { status: 200 }))
  await waitFor(() => expect(screen.queryByRole("button", { name: "Confirm import" })).toBeNull())
})
