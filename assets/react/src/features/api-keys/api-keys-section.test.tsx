import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react"
import { afterEach, expect, it, vi } from "vite-plus/test"
import { ApiKeysSection, type ApiKey } from "./api-keys-section"

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
})

it("creates a key, shows its secret once, and revokes only after confirmation", async () => {
  let keys: ApiKey[] = []
  const requests: Array<{ method: string; url: string; body?: unknown }> = []
  vi.stubGlobal(
    "fetch",
    vi.fn((url: string, init?: RequestInit) => {
      const method = init?.method ?? "GET"
      requests.push({
        method,
        url,
        body: init?.body ? JSON.parse(init.body as string) : undefined,
      })
      if (method === "POST") {
        const key = {
          id: 7,
          name: "Spreadsheet",
          prefix: "tg_abcdefg",
          last_used_at: null,
          inserted_at: "2026-09-27T12:00:00Z",
        }
        keys = [key]
        return Promise.resolve(
          Response.json({ data: { ...key, token: "tg_abcdefgsecret" } }, { status: 201 }),
        )
      }
      if (method === "DELETE") {
        keys = []
        return Promise.resolve(new Response(null, { status: 204 }))
      }
      return Promise.resolve(Response.json({ data: keys }))
    }),
  )
  const writeText = vi.fn().mockResolvedValue(undefined)
  vi.stubGlobal("navigator", { clipboard: { writeText } })
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  const page = render(
    <QueryClientProvider client={client}>
      <ApiKeysSection />
    </QueryClientProvider>,
  )

  await screen.findByText("You have no API keys.")
  fireEvent.change(screen.getByRole("textbox", { name: "Key name" }), {
    target: { value: "Spreadsheet" },
  })
  fireEvent.click(screen.getByRole("button", { name: "Create key" }))

  const secret = await screen.findByRole<HTMLInputElement>("textbox", {
    name: "New key for Spreadsheet",
  })
  expect(secret.value).toBe("tg_abcdefgsecret")
  expect(requests).toContainEqual({
    method: "POST",
    url: "/api/session/api-keys",
    body: { api_key: { name: "Spreadsheet" } },
  })
  fireEvent.click(screen.getByRole("button", { name: "Copy key" }))
  await screen.findByText("Key copied.")
  expect(writeText).toHaveBeenCalledWith("tg_abcdefgsecret")
  await screen.findByText(/Never used/)

  fireEvent.click(screen.getByRole("button", { name: "Revoke Spreadsheet" }))
  const dialog = await screen.findByRole("alertdialog", { name: "Revoke Spreadsheet?" })
  fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }))
  expect(requests.some((request) => request.method === "DELETE")).toBe(false)

  fireEvent.click(screen.getByRole("button", { name: "Revoke Spreadsheet" }))
  fireEvent.click(
    within(await screen.findByRole("alertdialog")).getByRole("button", { name: "Revoke key" }),
  )
  await screen.findByText("You have no API keys.")
  expect(requests).toContainEqual({
    method: "DELETE",
    url: "/api/session/api-keys/7",
    body: undefined,
  })
  await waitFor(() => expect(screen.queryByRole("textbox", { name: /New key/ })).toBeNull())

  page.unmount()
})
