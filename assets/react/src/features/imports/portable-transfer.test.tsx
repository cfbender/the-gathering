import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react"
import { afterEach, expect, it, vi } from "vite-plus/test"
import { PortableTransfer } from "./portable-transfer"

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
  vi.restoreAllMocks()
})

function mount() {
  const client = new QueryClient({
    defaultOptions: { mutations: { retry: false }, queries: { staleTime: Infinity } },
  })
  client.setQueryData(["session"], null)
  return render(
    <QueryClientProvider client={client}>
      <PortableTransfer />
    </QueryClientProvider>,
  )
}

async function chooseFile(text: string) {
  const file = new File([text], "history.json", { type: "application/json" })
  Object.defineProperty(file, "text", { value: async () => text })
  fireEvent.change(screen.getByLabelText("The Gathering JSON file"), { target: { files: [file] } })
  await waitFor(() =>
    expect(
      (screen.getByRole("button", { name: "Preview transfer" }) as HTMLButtonElement).disabled,
    ).toBe(false),
  )
}

it("downloads the full document as a JSON file and reports export errors", async () => {
  const archive = { format: "the-gathering", version: 1, games: [{ notes: "Keep this", kills: 0 }] }
  const fetch = vi
    .fn()
    .mockResolvedValueOnce(new Response(JSON.stringify(archive), { status: 200 }))
    .mockResolvedValueOnce(
      new Response(JSON.stringify({ errors: { detail: "Unavailable" } }), { status: 503 }),
    )
  vi.stubGlobal("fetch", fetch)
  let blob: Blob | undefined
  const create = vi.fn((value: Blob | MediaSource) => {
    if (value instanceof Blob) blob = value
    return "blob:export"
  })
  vi.spyOn(URL, "createObjectURL").mockImplementation(create)
  const revoke = vi.spyOn(URL, "revokeObjectURL").mockImplementation(() => {})
  let filename = ""
  vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(function (
    this: HTMLAnchorElement,
  ) {
    filename = this.download
  })
  mount()
  fireEvent.click(screen.getByRole("button", { name: "Download JSON export" }))
  await screen.findByText(/Export prepared/)
  expect(filename).toMatch(/^the-gathering-\d{4}-\d{2}-\d{2}\.json$/)
  expect(blob?.type).toBe("application/json")
  const contents = await new Promise<string>((resolve) => {
    const reader = new FileReader()
    reader.onload = () => resolve(reader.result as string)
    reader.readAsText(blob!)
  })
  expect(JSON.parse(contents)).toEqual(archive)
  expect(revoke).toHaveBeenCalledWith("blob:export")
  fireEvent.click(screen.getByRole("button", { name: "Download JSON export" }))
  expect(await screen.findByRole("alert")).toBeTruthy()
})

it("previews counts, invalidates a preview when a file changes, and confirms the reviewed file", async () => {
  const counts = {
    players: { created: 2, reused: 1 },
    decks: { created: 4, reused: 3 },
    games: { created: 7, reused: 5 },
  }
  const fetch = vi.fn(async () => new Response(JSON.stringify({ data: counts }), { status: 200 }))
  vi.stubGlobal("fetch", fetch)
  mount()
  expect(screen.queryByRole("button", { name: "Confirm transfer" })).toBeNull()
  await chooseFile("first export")
  fireEvent.click(screen.getByRole("button", { name: "Preview transfer" }))
  await screen.findByText("Transfer preview")
  expect(within(screen.getByRole("row", { name: /games/i })).getByText("7")).toBeTruthy()
  await chooseFile("reviewed export")
  expect(screen.queryByRole("button", { name: "Confirm transfer" })).toBeNull()
  fireEvent.click(screen.getByRole("button", { name: "Preview transfer" }))
  fireEvent.click(await screen.findByRole("button", { name: "Confirm transfer" }))
  await screen.findByText("Transfer complete")
  expect(fetch).toHaveBeenLastCalledWith(
    "/api/imports/portable",
    expect.objectContaining({ body: JSON.stringify({ json: "reviewed export" }) }),
  )
})
