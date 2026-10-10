import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react"
import { afterEach, expect, it, vi } from "vite-plus/test"
import { AuditLogPage } from "./audit-log-page"

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
})

function mount() {
  render(
    <QueryClientProvider
      client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}
    >
      <AuditLogPage />
    </QueryClientProvider>,
  )
}

it("shows ordered before/after changes and distinguishes deleted records from empty results", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string) => {
      const data = url.startsWith("/api/admin/audit/7")
        ? [
            {
              id: 10,
              entity: "players",
              entity_id: "5",
              before: { id: 5, name: "Old name" },
              after: { id: 5, name: "New name" },
            },
            {
              id: 11,
              entity: "decks",
              entity_id: "9",
              before: { id: 9, name: "Deleted deck" },
              after: null,
            },
          ]
        : [
            {
              id: 7,
              actor_id: 42,
              actor_name: "member",
              action: "PATCH /api/players/{id}",
              target: "/api/players/5",
              status: 200,
              inserted_at: "2026-10-10T12:00:00Z",
              change_count: 2,
            },
          ]
      return new Response(
        JSON.stringify({ data, pagination: { page: 1, per_page: 25, total: data.length } }),
      )
    }),
  )
  mount()
  const operation = await screen.findByRole("button", { name: /@member/ })
  expect(operation.getAttribute("aria-expanded")).toBe("false")
  fireEvent.click(operation)
  const player = await screen.findByRole("region", { name: "players 5" })
  expect(within(player).getByText(/"name": "Old name"/)).toBeTruthy()
  expect(within(player).getByText(/"name": "New name"/)).toBeTruthy()
  expect(within(player).getByText("Changed fields: name")).toBeTruthy()
  expect(
    within(screen.getByRole("region", { name: "decks 9" })).getByText("Record does not exist"),
  ).toBeTruthy()
  fireEvent.click(operation)
  expect(screen.queryByRole("region", { name: "players 5" })).toBeNull()
})

it("applies filters and pagination without showing old rows on an access error", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string) => {
      const query = new URL(url, "http://test").searchParams
      if (query.get("page") === "2")
        return new Response(JSON.stringify({ errors: { detail: "Forbidden" } }), { status: 403 })
      const failed = query.get("outcome") === "failed" && query.get("search") === "member"
      return new Response(
        JSON.stringify({
          data: failed
            ? []
            : [
                {
                  id: 7,
                  actor_id: 42,
                  actor_name: "member",
                  action: "DELETE /api/decks/{id}",
                  target: "/api/decks/9",
                  status: 204,
                  inserted_at: "2026-10-10T12:00:00Z",
                  change_count: 1,
                },
              ],
          pagination: { page: 1, per_page: 25, total: failed ? 0 : 26 },
        }),
      )
    }),
  )
  mount()
  fireEvent.click(await screen.findByRole("button", { name: "Next" }))
  expect(await screen.findByRole("alert")).toBeTruthy()
  expect(screen.queryByRole("button", { name: /@member/ })).toBeNull()
  fireEvent.change(screen.getByRole("searchbox"), { target: { value: "member" } })
  fireEvent.change(screen.getByRole("combobox", { name: "Outcome" }), {
    target: { value: "failed" },
  })
  fireEvent.click(screen.getByRole("button", { name: "Apply filters" }))
  expect(await screen.findByText("No operations match these filters.")).toBeTruthy()
})
