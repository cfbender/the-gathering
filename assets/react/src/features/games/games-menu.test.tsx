import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react"
import { afterEach, expect, it, vi } from "vite-plus/test"
import { ToastProvider } from "@/components/ui/toast"
import { GamesMenu } from "./games-menu"

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
  vi.restoreAllMocks()
})

function mount() {
  const client = new QueryClient({ defaultOptions: { mutations: { retry: false } } })
  return render(
    <QueryClientProvider client={client}>
      <ToastProvider>
        <GamesMenu />
      </ToastProvider>
    </QueryClientProvider>,
  )
}

async function chooseExport() {
  const trigger = screen.getByRole("button", { name: "More game actions" })
  act(() => trigger.focus())
  fireEvent.keyDown(trigger, { key: "ArrowDown" })
  fireEvent.click(await screen.findByRole("menuitem", { name: "Export history (JSON)" }))
}

it("downloads the full history as a JSON file and reports export errors", async () => {
  const archive = { format: "the-gathering", version: 1, games: [{ notes: "Keep this", kills: 0 }] }
  const fetch = vi
    .fn()
    .mockResolvedValueOnce(new Response(JSON.stringify(archive), { status: 200 }))
    .mockResolvedValueOnce(
      new Response(JSON.stringify({ errors: { detail: "Unavailable" } }), { status: 503 }),
    )
  vi.stubGlobal("fetch", fetch)
  let blob: Blob | undefined
  vi.spyOn(URL, "createObjectURL").mockImplementation((value: Blob | MediaSource) => {
    if (value instanceof Blob) blob = value
    return "blob:export"
  })
  const revoke = vi.spyOn(URL, "revokeObjectURL").mockImplementation(() => {})
  let filename = ""
  vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(function (
    this: HTMLAnchorElement,
  ) {
    filename = this.download
  })
  mount()
  await chooseExport()
  await waitFor(() => expect(revoke).toHaveBeenCalledWith("blob:export"))
  expect(fetch).toHaveBeenCalledWith("/api/exports/portable", expect.anything())
  expect(filename).toMatch(/^the-gathering-\d{4}-\d{2}-\d{2}\.json$/)
  expect(blob?.type).toBe("application/json")
  expect(JSON.parse(await blob!.text())).toEqual(archive)

  await chooseExport()
  expect(await screen.findByText("Unavailable")).toBeTruthy()
})
