import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react"
import type { ReactNode } from "react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test"
import type { User } from "@/lib/auth"
import { ThemeProvider } from "@/lib/theme"
import { AppearanceSection } from "./appearance-section"
import { ThemeToggle } from "./theme-toggle"

const STORAGE_KEY = "the-gathering:theme"
const STYLE_STORAGE_KEY = "the-gathering:theme-style"
const PALETTE_STORAGE_KEY = "the-gathering:palette"

function stubSystemTheme(theme: "light" | "dark") {
  vi.stubGlobal(
    "matchMedia",
    vi.fn().mockImplementation((query: string) => ({
      matches: theme === "dark" && query.includes("dark"),
      media: query,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
    })),
  )
}

const signedInUser: User = {
  id: 1,
  username: "cody",
  display_name: "Cody",
  discord_id: null,
  avatar_url: null,
  moxfield_username: null,
  archidekt_username: null,
  manavault_url: null,
  has_manavault_api_key: false,
  has_password: true,
  role: "admin",
  disabled: false,
  palette: "nord",
  theme_style: "classic",
  inserted_at: "2026-09-20T00:00:00Z",
}

function json(status: number, body: unknown) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  })
}

/** Stubs the API: a signed-out session unless `session` is given, echoing appearance saves. */
function stubApi({ session, saveStatus = 200 }: { session?: User; saveStatus?: number } = {}) {
  const fetchMock = vi.fn(async (path: string, init?: RequestInit) => {
    if (path === "/api/session/appearance") {
      if (saveStatus !== 200) return json(saveStatus, { errors: { palette: ["is invalid"] } })
      const { user } = JSON.parse(typeof init?.body === "string" ? init.body : "{}") as {
        user: Partial<User>
      }
      return json(200, { data: { ...session, ...user } })
    }
    return session
      ? json(200, { data: session })
      : json(401, { errors: { detail: "Unauthorized" } })
  })
  vi.stubGlobal("fetch", fetchMock)
  return fetchMock
}

function renderWithProviders(children: ReactNode, session?: User) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false, staleTime: Infinity }, mutations: { retry: false } },
  })
  if (session) queryClient.setQueryData(["session"], session)

  return render(
    <QueryClientProvider client={queryClient}>
      <ThemeProvider>{children}</ThemeProvider>
    </QueryClientProvider>,
  )
}

function renderToggle() {
  return renderWithProviders(<ThemeToggle />)
}

function renderStylePicker(session?: User) {
  return renderWithProviders(<AppearanceSection />, session)
}

describe("ThemeToggle", () => {
  beforeEach(() => {
    localStorage.clear()
    delete document.documentElement.dataset.theme
    delete document.documentElement.dataset.themeStyle
    delete document.documentElement.dataset.palette
    stubSystemTheme("dark")
    stubApi()
  })

  afterEach(() => {
    cleanup()
    vi.unstubAllGlobals()
  })

  it("defaults to the system preference and resolves it against prefers-color-scheme", () => {
    renderToggle()

    expect(
      screen.getByRole("radio", { name: "Match system theme" }).getAttribute("aria-checked"),
    ).toBe("true")
    expect(document.documentElement.dataset.theme).toBe("dark")
  })

  it("applies an explicit theme and persists it for the SPA shell's inline script", () => {
    renderToggle()

    fireEvent.click(screen.getByRole("radio", { name: "Light theme" }))

    expect(document.documentElement.dataset.theme).toBe("light")
    expect(localStorage.getItem(STORAGE_KEY)).toBe("light")
    expect(screen.getByRole("radio", { name: "Light theme" }).getAttribute("aria-checked")).toBe(
      "true",
    )
    expect(
      screen.getByRole("radio", { name: "Match system theme" }).getAttribute("aria-checked"),
    ).toBe("false")
  })

  it("defaults to the glass style and only persists the classic opt-out", () => {
    renderStylePicker()

    expect(document.documentElement.dataset.themeStyle).toBe("glass")
    expect(screen.getByRole("button", { name: /Classic/ }).getAttribute("aria-pressed")).toBe(
      "false",
    )

    fireEvent.click(screen.getByRole("button", { name: /Classic/ }))

    expect(document.documentElement.dataset.themeStyle).toBe("classic")
    expect(localStorage.getItem(STYLE_STORAGE_KEY)).toBe("classic")

    fireEvent.click(screen.getByRole("button", { name: /Liquid glass/ }))

    expect(document.documentElement.dataset.themeStyle).toBe("glass")
    expect(localStorage.getItem(STYLE_STORAGE_KEY)).toBeNull()
  })

  it("restores a stored classic style", () => {
    localStorage.setItem(STYLE_STORAGE_KEY, "classic")
    renderStylePicker()

    expect(document.documentElement.dataset.themeStyle).toBe("classic")
    expect(screen.getByRole("button", { name: /Classic/ }).getAttribute("aria-pressed")).toBe(
      "true",
    )
  })

  it("defaults to the claret palette and only persists other palettes", () => {
    renderStylePicker()

    expect(document.documentElement.dataset.palette).toBe("claret")
    expect(screen.getByRole("button", { name: "Claret" }).getAttribute("aria-pressed")).toBe("true")

    fireEvent.click(screen.getByRole("button", { name: "Gruvbox" }))

    expect(document.documentElement.dataset.palette).toBe("gruvbox")
    expect(localStorage.getItem(PALETTE_STORAGE_KEY)).toBe("gruvbox")
    expect(screen.getByRole("button", { name: "Gruvbox" }).getAttribute("aria-pressed")).toBe(
      "true",
    )

    fireEvent.click(screen.getByRole("button", { name: "Claret" }))

    expect(document.documentElement.dataset.palette).toBe("claret")
    expect(localStorage.getItem(PALETTE_STORAGE_KEY)).toBeNull()
  })

  it("restores a stored palette and ignores unknown ones", () => {
    localStorage.setItem(PALETTE_STORAGE_KEY, "nord")
    const { unmount } = renderStylePicker()

    expect(document.documentElement.dataset.palette).toBe("nord")
    unmount()
    delete document.documentElement.dataset.palette

    localStorage.setItem(PALETTE_STORAGE_KEY, "vaporwave")
    renderStylePicker()

    expect(document.documentElement.dataset.palette).toBe("claret")
  })

  it("starts from the palette and style the SPA shell rendered instead of device storage", () => {
    document.documentElement.dataset.palette = "kanagawa"
    document.documentElement.dataset.themeStyle = "classic"
    localStorage.setItem(PALETTE_STORAGE_KEY, "nord")
    renderStylePicker()

    expect(document.documentElement.dataset.palette).toBe("kanagawa")
    expect(document.documentElement.dataset.themeStyle).toBe("classic")
  })

  it("uses the signed-in account's appearance and saves changes to it", async () => {
    localStorage.setItem(PALETTE_STORAGE_KEY, "gruvbox")
    const fetchMock = stubApi({ session: signedInUser })
    renderStylePicker(signedInUser)

    expect(document.documentElement.dataset.palette).toBe("nord")
    expect(document.documentElement.dataset.themeStyle).toBe("classic")

    fireEvent.click(screen.getByRole("button", { name: "Catppuccin" }))
    fireEvent.click(screen.getByRole("button", { name: /Liquid glass/ }))

    expect(document.documentElement.dataset.palette).toBe("catppuccin")
    expect(document.documentElement.dataset.themeStyle).toBe("glass")
    // The device copy keeps signed-out pages in step with the last choice.
    expect(localStorage.getItem(PALETTE_STORAGE_KEY)).toBe("catppuccin")

    await waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(2))
    const saves = fetchMock.mock.calls.map(([path, init]) => [path, init?.method, init?.body])
    expect(saves).toEqual([
      ["/api/session/appearance", "PATCH", JSON.stringify({ user: { palette: "catppuccin" } })],
      ["/api/session/appearance", "PATCH", JSON.stringify({ user: { theme_style: "glass" } })],
    ])
  })

  it("restores the account's saved palette when a save fails", async () => {
    stubApi({ session: signedInUser, saveStatus: 422 })
    renderStylePicker(signedInUser)

    fireEvent.click(screen.getByRole("button", { name: "Everforest" }))
    expect(document.documentElement.dataset.palette).toBe("everforest")

    await waitFor(() => expect(document.documentElement.dataset.palette).toBe("nord"))
  })

  it("restores a stored preference and clears it when switching back to system", () => {
    localStorage.setItem(STORAGE_KEY, "light")
    renderToggle()

    expect(document.documentElement.dataset.theme).toBe("light")

    fireEvent.click(screen.getByRole("radio", { name: "Match system theme" }))

    expect(localStorage.getItem(STORAGE_KEY)).toBeNull()
    expect(document.documentElement.dataset.theme).toBe("dark")
  })
})
