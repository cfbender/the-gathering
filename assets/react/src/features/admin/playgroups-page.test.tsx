import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react"
import { afterEach, expect, it, vi } from "vite-plus/test"
import { PlaygroupsPage } from "./playgroups-page"

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
})

const user = {
  id: 1,
  username: "admin",
  display_name: "Admin",
  role: "admin",
  has_password: true,
  playgroup_id: 1,
}

function json(body: unknown, status = 200) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  })
}

type Member = { id: number; username: string; display_name: string; role: string }
type PlaygroupRow = {
  id: number
  name: string
  discord_guild_id: string | null
  invite_enabled: boolean
  members: Member[]
}
type Input = { name: string; discord_guild_id: string }

const admin: Member = { id: 1, username: "admin", display_name: "Admin", role: "admin" }
const pat: Member = { id: 2, username: "pat", display_name: "Pat", role: "member" }
const accounts = [admin, pat].map((member) => ({ ...member, disabled: false }))

const parseBody = <T = Input,>(init?: RequestInit) =>
  JSON.parse(typeof init?.body === "string" ? init.body : "{}") as T

/** Requests every render makes: the session, the account list, and each playgroup's invite. */
function shared(url: string, init?: RequestInit) {
  if (url === "/api/session") return json({ data: user })
  if (url === "/api/admin/users") return json({ data: accounts })
  if (/^\/api\/admin\/playgroups\/\d+\/registration-invite$/.test(url) && !init?.method) {
    return json({ data: { enabled: false } })
  }
  return null
}

function renderPage() {
  render(
    <QueryClientProvider
      client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}
    >
      <PlaygroupsPage />
    </QueryClientProvider>,
  )
}

it("creates a playgroup, shows a taken name inline, and deletes only after confirming", async () => {
  const playgroups: PlaygroupRow[] = [
    { id: 1, name: "Main", discord_guild_id: null, invite_enabled: false, members: [admin, pat] },
  ]
  const fetch = vi.fn(async (url: string, init?: RequestInit) => {
    const common = shared(url, init)
    if (common) return common
    if (url === "/api/admin/playgroups" && !init?.method) return json({ data: playgroups })
    if (url === "/api/admin/playgroups" && init?.method === "POST") {
      const input = parseBody(init)
      if (playgroups.some((playgroup) => playgroup.name === input.name)) {
        return json({ errors: { name: ["has already been taken"] } }, 422)
      }
      const created: PlaygroupRow = {
        id: playgroups.length + 1,
        name: input.name,
        discord_guild_id: "456",
        invite_enabled: false,
        members: [admin],
      }
      playgroups.push(created)
      return json({ data: created }, 201)
    }
    if (url === "/api/admin/playgroups/2" && init?.method === "DELETE") {
      playgroups.splice(1, 1)
      return new Response(null, { status: 204 })
    }
    throw new Error(`Unexpected request: ${url}`)
  })
  vi.stubGlobal("fetch", fetch)
  renderPage()

  const list = await screen.findByRole("list", { name: "Playgroups" })
  expect(within(list).getByText("Main")).toBeTruthy()
  // The last playgroup cannot be deleted.
  expect(screen.getByRole<HTMLButtonElement>("button", { name: "Delete Main" }).disabled).toBe(true)

  const name = screen.getByRole("textbox", { name: "Name" })
  fireEvent.change(name, { target: { value: "Main" } })
  fireEvent.click(screen.getByRole("button", { name: "Create playgroup" }))
  expect(await screen.findByText("has already been taken")).toBeTruthy()

  fireEvent.change(name, { target: { value: "Thursday crew" } })
  fireEvent.change(screen.getByRole("textbox", { name: "Discord server ID (optional)" }), {
    target: { value: "456" },
  })
  fireEvent.click(screen.getByRole("button", { name: "Create playgroup" }))
  expect(await within(list).findByText("Thursday crew")).toBeTruthy()
  expect(within(list).getByText("456")).toBeTruthy()
  const create = fetch.mock.calls.filter(
    ([url, init]) => url === "/api/admin/playgroups" && init?.method === "POST",
  )
  expect(parseBody(create[1]?.[1])).toEqual({
    name: "Thursday crew",
    discord_guild_id: "456",
  })
  expect(screen.getByRole<HTMLInputElement>("textbox", { name: "Name" }).value).toBe("")
  expect(screen.getByRole<HTMLButtonElement>("button", { name: "Delete Main" }).disabled).toBe(
    false,
  )

  fireEvent.click(screen.getByRole("button", { name: "Delete Thursday crew" }))
  const dialog = await screen.findByRole("alertdialog", { name: "Delete Thursday crew?" })
  fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }))
  expect(fetch.mock.calls.some(([, init]) => init?.method === "DELETE")).toBe(false)
  fireEvent.click(screen.getByRole("button", { name: "Delete Thursday crew" }))
  fireEvent.click(
    within(await screen.findByRole("alertdialog")).getByRole("button", {
      name: "Delete playgroup",
    }),
  )
  await screen.findByText("Deleted Thursday crew.")
  await waitFor(() => expect(within(list).queryByText("Thursday crew")).toBeNull())
})

it("edits a playgroup in place", async () => {
  let playgroup: PlaygroupRow = {
    id: 1,
    name: "Main",
    discord_guild_id: "123",
    invite_enabled: false,
    members: [admin],
  }
  const fetch = vi.fn(async (url: string, init?: RequestInit) => {
    const common = shared(url, init)
    if (common) return common
    if (url === "/api/admin/playgroups" && !init?.method) return json({ data: [playgroup] })
    if (url === "/api/admin/playgroups/1" && init?.method === "PATCH") {
      const input = parseBody(init)
      playgroup = {
        ...playgroup,
        name: input.name,
        discord_guild_id: input.discord_guild_id || null,
      }
      return json({ data: playgroup })
    }
    throw new Error(`Unexpected request: ${url}`)
  })
  vi.stubGlobal("fetch", fetch)
  renderPage()

  fireEvent.click(await screen.findByRole("button", { name: "Edit Main" }))
  const form = screen.getByRole("form", { name: "Edit Main" })
  const name = within(form).getByRole<HTMLInputElement>("textbox", { name: "Name" })
  expect(name.value).toBe("Main")
  fireEvent.change(name, { target: { value: "Kitchen table" } })
  fireEvent.change(within(form).getByRole("textbox", { name: "Discord server ID (optional)" }), {
    target: { value: "" },
  })
  fireEvent.click(within(form).getByRole("button", { name: "Save" }))

  const list = await screen.findByRole("list", { name: "Playgroups" })
  await within(list).findByText("Kitchen table")
  expect(within(list).getByText("Not linked")).toBeTruthy()
  expect(screen.queryByRole("form", { name: "Edit Main" })).toBeNull()
})

it("adds and removes members, showing who belongs to each playgroup", async () => {
  const crew: PlaygroupRow = {
    id: 2,
    name: "Thursday crew",
    discord_guild_id: null,
    invite_enabled: true,
    members: [pat],
  }
  const main: PlaygroupRow = {
    id: 1,
    name: "Main",
    discord_guild_id: null,
    invite_enabled: false,
    members: [admin, pat],
  }
  const fetch = vi.fn(async (url: string, init?: RequestInit) => {
    const common = shared(url, init)
    if (common) return common
    if (url === "/api/admin/playgroups" && !init?.method) return json({ data: [main, crew] })
    if (url === "/api/admin/playgroups/2/members" && init?.method === "POST") {
      const { user_id } = parseBody<{ user_id: number }>(init)
      const member = accounts.find((account) => account.id === user_id)
      if (!member) return json({ errors: { user_id: ["does not exist"] } }, 422)
      crew.members = [admin, pat]
      return json({ data: crew }, 201)
    }
    if (url === "/api/admin/playgroups/2/members/2" && init?.method === "DELETE") {
      crew.members = crew.members.filter((member) => member.id !== 2)
      return new Response(null, { status: 204 })
    }
    throw new Error(`Unexpected request: ${url}`)
  })
  vi.stubGlobal("fetch", fetch)
  renderPage()

  const members = await screen.findByRole("region", { name: "Members of Thursday crew" })
  expect(within(members).getByText("Pat")).toBeTruthy()
  expect(within(members).queryByText("Admin")).toBeNull()
  expect(screen.getByText("Invite link active")).toBeTruthy()
  // The admin is not a member, so there is nothing to view there yet.
  expect(screen.queryByRole("button", { name: "View" })).toBeNull()

  const picker = within(members).getByRole<HTMLSelectElement>("combobox", { name: "Add a member" })
  expect(Array.from(picker.options).map((option) => option.textContent)).toEqual([
    "Choose an account",
    "Admin (@admin)",
  ])
  fireEvent.change(picker, { target: { value: "1" } })
  fireEvent.click(within(members).getByRole("button", { name: "Add" }))
  await within(members).findByText("Admin")
  expect(
    parseBody<{ user_id: number }>(
      fetch.mock.calls.find(([url]) => url === "/api/admin/playgroups/2/members")?.[1],
    ),
  ).toEqual({ user_id: 1 })
  await screen.findByRole("button", { name: "View" })

  fireEvent.click(within(members).getByRole("button", { name: "Remove Pat from Thursday crew" }))
  const dialog = await screen.findByRole("alertdialog", { name: "Remove Pat from Thursday crew?" })
  fireEvent.click(within(dialog).getByRole("button", { name: "Remove member" }))
  await waitFor(() => expect(within(members).queryByText("Pat")).toBeNull())
  expect(
    fetch.mock.calls.some(
      ([url, init]) => url === "/api/admin/playgroups/2/members/2" && init?.method === "DELETE",
    ),
  ).toBe(true)
})
