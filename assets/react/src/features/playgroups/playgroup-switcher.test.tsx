import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import {
  createMemoryHistory,
  createRootRoute,
  createRouter,
  RouterProvider,
} from "@tanstack/react-router"
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react"
import { afterEach, expect, it, vi } from "vite-plus/test"
import { PlaygroupSwitcher } from "./playgroup-switcher"

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
})

const user = {
  id: 42,
  username: "member",
  display_name: "Member",
  role: "member",
  has_password: true,
  playgroup_id: 1,
}

const playgroups = [
  { id: 1, name: "Main", discord_guild_id: null },
  { id: 2, name: "Thursday crew", discord_guild_id: "123" },
]

function json(body: unknown, status = 200) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  })
}

it("names the active playgroup, switches it, and refreshes scoped data", async () => {
  let active = 1
  const fetch = vi.fn(async (url: string, init?: RequestInit) => {
    if (url === "/api/session/playgroup" && init?.method === "PATCH") {
      active = (
        JSON.parse(typeof init.body === "string" ? init.body : "{}") as { playgroup_id: number }
      ).playgroup_id
      return json({ data: { ...user, playgroup_id: active } })
    }
    if (url === "/api/session") return json({ data: { ...user, playgroup_id: active } })
    if (url === "/api/playgroups") return json({ data: playgroups })
    throw new Error(`Unexpected request: ${url}`)
  })
  vi.stubGlobal("fetch", fetch)
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  client.setQueryData(["games", "list"], "stale games")
  const router = createRouter({
    routeTree: createRootRoute({ component: PlaygroupSwitcher }),
    history: createMemoryHistory(),
  })
  render(
    <QueryClientProvider client={client}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  )

  const trigger = await screen.findByRole("button", { name: "Playgroup: Main" })
  fireEvent.pointerDown(trigger, { button: 0, ctrlKey: false })
  fireEvent.click(trigger)
  const option = await screen.findByRole("menuitemradio", { name: "Thursday crew" })
  expect(screen.getByRole("menuitemradio", { name: "Main" }).getAttribute("aria-checked")).toBe(
    "true",
  )
  // Members do not manage playgroups.
  expect(screen.queryByRole("menuitem", { name: "Manage playgroups" })).toBeNull()
  fireEvent.click(option)

  await screen.findByRole("button", { name: "Playgroup: Thursday crew" })
  expect(
    fetch.mock.calls.some(
      ([url, init]) => url === "/api/session/playgroup" && init?.method === "PATCH",
    ),
  ).toBe(true)
  await waitFor(() => expect(client.getQueryState(["games", "list"])?.isInvalidated).toBe(true))
  expect(client.getQueryState(["playgroups"])?.isInvalidated).toBe(false)
})
