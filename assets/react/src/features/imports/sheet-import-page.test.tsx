import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import {
  createMemoryHistory,
  createRootRoute,
  createRouter,
  RouterProvider,
} from "@tanstack/react-router"
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react"
import { afterEach, expect, it, vi } from "vite-plus/test"
import { SheetImportPage } from "./sheet-import-page"
import type { SheetInput, SheetPreview } from "./sheet-import"

const preview: SheetPreview = {
  revision: "revision-1",
  valid: false,
  players: [{ id: 1, name: "Daniel" }],
  decks: [],
  rows: [
    {
      key: "row-key",
      line: 2,
      date: "2025-03-17",
      winner: "Daniel",
      notes: "Win con: Swing Out",
      seats: [
        {
          player: "Daniel",
          deck: "Edgar",
          player_id: 1,
          deck_id: null,
          deck_key: '["Daniel","Edgar"]',
          kills: 1,
          result: "win",
        },
      ],
      kill_counts: [{ player: "Dan", kills: 1 }],
      errors: [],
      warnings: [],
      action: 42,
      status: "changed",
      match_reason: "Matched by date and players",
      changes: [
        { field: "kills", player: "Daniel", before: null, after: 1 },
        { field: "notes", player: null, before: "Existing note", after: "Win con: Swing Out" },
      ],
      imported_id: null,
      candidates: [
        {
          id: 42,
          played_at: "2025-03-17T12:00:00Z",
          notes: "Existing note",
          seats: [
            { player_id: 1, player: "Daniel", deck: "Cleaned deck", result: "win", kills: null },
          ],
        },
      ],
      target: null,
    },
  ],
}

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
})

it("requires refreshed selections, shows the existing game, and commits the reviewed revision", async () => {
  const fetch = vi.fn(async (url: string, init: RequestInit) => {
    if (typeof init.body !== "string") throw new Error("Expected JSON request body")
    const input = JSON.parse(init.body) as SheetInput
    const action = input.actions["row-key"] ?? 42
    const data = url.endsWith("preview")
      ? {
          ...preview,
          revision: `revision-${action}`,
          valid: action === 42,
          rows: preview.rows.map((row) => ({ ...row, action, target: row.candidates[0] })),
        }
      : { created: 0, updated: 1, skipped: 0, game_ids: [] }
    return new Response(JSON.stringify({ data }), {
      status: 200,
      headers: { "content-type": "application/json" },
    })
  })
  vi.stubGlobal("fetch", fetch)
  const router = createRouter({
    routeTree: createRootRoute({ component: SheetImportPage }),
    history: createMemoryHistory(),
  })
  render(
    <QueryClientProvider client={new QueryClient()}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  )
  fireEvent.change(await screen.findByRole("textbox", { name: "Sheet contents" }), {
    target: { value: "sheet data" },
  })
  fireEvent.click(screen.getByRole("button", { name: "Read sheet" }))
  await screen.findByText("Row 2 · 2025-03-17 · Daniel")
  const confirm = screen.getByRole("button", {
    name: "Confirm selected changes",
  }) as HTMLButtonElement
  expect(confirm.disabled).toBe(false)
  const changes = screen.getByLabelText("Changes for row 2")
  expect(within(changes).getByText("Existing note")).toBeTruthy()
  expect(within(changes).getByText("Unknown")).toBeTruthy()
  fireEvent.change(screen.getByRole("combobox", { name: "Action for row 2" }), {
    target: { value: "skip" },
  })
  expect(confirm.disabled).toBe(true)
  expect(screen.queryByLabelText("Changes for row 2")).toBeNull()
  fireEvent.click(screen.getAllByRole("button", { name: "Refresh preview" })[0]!)
  await waitFor(() =>
    expect(screen.queryByText("Refresh preview to recalculate changes.")).toBeNull(),
  )
  expect(confirm.disabled).toBe(true)
  fireEvent.change(screen.getByRole("combobox", { name: "Action for row 2" }), {
    target: { value: "42" },
  })
  expect(confirm.disabled).toBe(true)
  fireEvent.click(screen.getAllByRole("button", { name: "Refresh preview" })[0]!)
  await waitFor(() => expect(confirm.disabled).toBe(false))
  fireEvent.click(screen.getByText(/Player mappings/))
  fireEvent.change(screen.getByRole("combobox", { name: /^Dan$/ }), {
    target: { value: "1" },
  })
  expect(confirm.disabled).toBe(true)
  fireEvent.click(screen.getAllByRole("button", { name: "Refresh preview" })[0]!)
  await waitFor(() => expect(confirm.disabled).toBe(false))
  fireEvent.click(confirm)
  await screen.findByText(/Reconciled: 1 updated/)
  const commitCall = fetch.mock.calls.find(([url]) => url === "/api/imports/sheet")
  const body = commitCall?.[1].body
  if (typeof body !== "string") throw new Error("Expected import JSON request body")
  expect(JSON.parse(body)).toMatchObject({
    revision: "revision-42",
    players: { Dan: 1 },
    actions: { "row-key": 42 },
  })
})

it("separates result corrections, kills and notes, unchanged rows and unresolved matches", async () => {
  const base = preview.rows[0]!
  const data: SheetPreview = {
    ...preview,
    valid: true,
    rows: [
      { ...base, target: base.candidates[0]! },
      {
        ...base,
        key: "corrected",
        line: 3,
        changes: [{ field: "result", player: "Daniel", before: "loss", after: "win" }],
      },
      { ...base, key: "same", line: 4, status: "unchanged", action: "skip", changes: [] },
      {
        ...base,
        key: "ambiguous",
        line: 5,
        status: "review",
        action: "skip",
        changes: [],
        match_reason: "Multiple games match",
      },
    ],
  }
  vi.stubGlobal(
    "fetch",
    vi.fn(async () => new Response(JSON.stringify({ data }), { status: 200 })),
  )
  const router = createRouter({
    routeTree: createRootRoute({ component: SheetImportPage }),
    history: createMemoryHistory(),
  })
  render(
    <QueryClientProvider client={new QueryClient()}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  )
  fireEvent.change(await screen.findByRole("textbox", { name: "Sheet contents" }), {
    target: { value: "sheet" },
  })
  fireEvent.click(screen.getByRole("button", { name: "Read sheet" }))
  await screen.findByText("Row 2 · 2025-03-17 · Daniel")
  expect(screen.getAllByRole("article")).toHaveLength(2)
  const filter = screen.getByRole("combobox", { name: "Show rows" })
  fireEvent.change(filter, { target: { value: "corrections" } })
  expect(screen.getAllByRole("article")).toHaveLength(1)
  expect(screen.getByText("Row 3 · 2025-03-17 · Daniel")).toBeTruthy()
  fireEvent.change(filter, { target: { value: "unchanged" } })
  expect(screen.getByText("No stored values would change. Skipped.")).toBeTruthy()
  fireEvent.change(filter, { target: { value: "review" } })
  expect(screen.getByText("Multiple games match")).toBeTruthy()
  expect(
    (screen.getByRole("button", { name: "Confirm selected changes" }) as HTMLButtonElement)
      .disabled,
  ).toBe(false)
})
