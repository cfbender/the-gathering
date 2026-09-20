import { cleanup, fireEvent, render, screen } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vite-plus/test"
import { ThemeProvider } from "@/lib/theme"
import { AppearanceSection } from "./appearance-section"
import { ThemeToggle } from "./theme-toggle"

const STORAGE_KEY = "the-gathering:theme"
const STYLE_STORAGE_KEY = "the-gathering:theme-style"

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

function renderToggle() {
  return render(
    <ThemeProvider>
      <ThemeToggle />
    </ThemeProvider>,
  )
}

function renderStylePicker() {
  return render(
    <ThemeProvider>
      <AppearanceSection />
    </ThemeProvider>,
  )
}

describe("ThemeToggle", () => {
  beforeEach(() => {
    localStorage.clear()
    delete document.documentElement.dataset.theme
    delete document.documentElement.dataset.themeStyle
    stubSystemTheme("dark")
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

  it("restores a stored preference and clears it when switching back to system", () => {
    localStorage.setItem(STORAGE_KEY, "light")
    renderToggle()

    expect(document.documentElement.dataset.theme).toBe("light")

    fireEvent.click(screen.getByRole("radio", { name: "Match system theme" }))

    expect(localStorage.getItem(STORAGE_KEY)).toBeNull()
    expect(document.documentElement.dataset.theme).toBe("dark")
  })
})
