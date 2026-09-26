import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { createContext, useCallback, useContext, useEffect, useMemo, useState } from "react"
import type { ReactNode } from "react"
import { api } from "@/lib/api"
import { sessionQueryOptions } from "@/lib/auth"
import type { User } from "@/lib/auth"
import { readStorage, useStoredChoice, writeStorage } from "@/lib/stored-choice"

export type Theme = "light" | "dark"
export type ThemePreference = Theme | "system"
/** How surfaces render; orthogonal to the light/dark palette. */
export type ThemeStyle = "classic" | "glass"

/** Color palettes, each with a light and dark variant; tokens live in palettes.css. The ids
 * must match `@palettes` in `TheGathering.Accounts.User`. */
export const PALETTES = [
  { id: "claret", label: "Claret" },
  { id: "nord", label: "Nord" },
  { id: "catppuccin", label: "Catppuccin" },
  { id: "tokyonight", label: "Tokyo Night" },
  { id: "gruvbox", label: "Gruvbox" },
  { id: "everforest", label: "Everforest" },
  { id: "kanagawa", label: "Kanagawa" },
  { id: "nightowl", label: "Night Owl" },
  { id: "dracula", label: "Dracula" },
  { id: "rosepine", label: "Rosé Pine" },
  { id: "solarized", label: "Solarized" },
  { id: "monochrome", label: "Monochrome" },
] as const
export type Palette = (typeof PALETTES)[number]["id"]
const DEFAULT_PALETTE: Palette = "claret"
const DEFAULT_THEME_STYLE: ThemeStyle = "glass"

// Must match the inline script in the SPA shell (AppController) so the first
// paint already has the right theme, style, and palette.
const STORAGE_KEY = "the-gathering:theme"
const STYLE_STORAGE_KEY = "the-gathering:theme-style"
const PALETTE_STORAGE_KEY = "the-gathering:palette"

function systemTheme(): Theme {
  return matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light"
}

function isThemePreference(value: unknown): value is ThemePreference {
  return value === "light" || value === "dark" || value === "system"
}

function isThemeStyle(value: string | null | undefined): value is ThemeStyle {
  return value === "classic" || value === "glass"
}

function isPalette(value: string | null | undefined): value is Palette {
  return PALETTES.some((palette) => palette.id === value)
}

// The SPA shell has already resolved the account's (or this device's) style and palette onto
// <html>, so start from that to avoid flashing the device value before the session loads.
function initialThemeStyle(): ThemeStyle {
  const value = document.documentElement.dataset.themeStyle ?? readStorage(STYLE_STORAGE_KEY)
  return isThemeStyle(value) ? value : DEFAULT_THEME_STYLE
}

function initialPalette(): Palette {
  const value = document.documentElement.dataset.palette ?? readStorage(PALETTE_STORAGE_KEY)
  return isPalette(value) ? value : DEFAULT_PALETTE
}

type Appearance = Pick<User, "palette" | "theme_style">

interface ThemeContextValue {
  preference: ThemePreference
  resolved: Theme
  setPreference: (preference: ThemePreference) => void
  themeStyle: ThemeStyle
  setThemeStyle: (style: ThemeStyle) => void
  palette: Palette
  setPalette: (palette: Palette) => void
}

const ThemeContext = createContext<ThemeContextValue | null>(null)

/**
 * Light/dark follows this device. Palette and surface style belong to the signed-in account
 * (saved via `PATCH /api/session/appearance`); the device values are only the fallback for
 * signed-out pages and are kept in sync so the login screen matches the last choice.
 */
export function ThemeProvider({ children }: { children: ReactNode }) {
  const queryClient = useQueryClient()
  const user = useQuery(sessionQueryOptions).data
  const [preference, setPreference] = useStoredChoice(STORAGE_KEY, "system", isThemePreference)
  const [deviceThemeStyle, setDeviceThemeStyle] = useState<ThemeStyle>(initialThemeStyle)
  const [devicePalette, setDevicePalette] = useState<Palette>(initialPalette)
  const [system, setSystem] = useState<Theme>(systemTheme)

  const { mutate: saveAppearance } = useMutation({
    // Serialize saves so rapid picks reach the server in the order they were made.
    scope: { id: "appearance" },
    mutationFn: (changes: Partial<Appearance>) =>
      api<{ data: User }>("/api/session/appearance", {
        method: "PATCH",
        body: JSON.stringify({ user: changes }),
      }),
    // Restore the account's saved values if a save fails.
    onError: () => queryClient.invalidateQueries({ queryKey: sessionQueryOptions.queryKey }),
  })

  useEffect(() => {
    const query = matchMedia("(prefers-color-scheme: dark)")
    const onChange = () => setSystem(systemTheme())
    query.addEventListener("change", onChange)
    return () => query.removeEventListener("change", onChange)
  }, [])

  const resolved = preference === "system" ? system : preference
  const themeStyle = user?.theme_style ?? deviceThemeStyle
  const palette = user?.palette ?? devicePalette

  useEffect(() => {
    document.documentElement.dataset.theme = resolved
  }, [resolved])

  useEffect(() => {
    document.documentElement.dataset.themeStyle = themeStyle
  }, [themeStyle])

  useEffect(() => {
    document.documentElement.dataset.palette = palette
  }, [palette])

  const updateAppearance = useCallback(
    (changes: Partial<Appearance>) => {
      if (!queryClient.getQueryData(sessionQueryOptions.queryKey)) return
      // Apply immediately; the server save runs in the background.
      queryClient.setQueryData(sessionQueryOptions.queryKey, (current) =>
        current ? { ...current, ...changes } : current,
      )
      saveAppearance(changes)
    },
    [queryClient, saveAppearance],
  )

  const setThemeStyle = useCallback(
    (next: ThemeStyle) => {
      setDeviceThemeStyle(next)
      writeStorage(STYLE_STORAGE_KEY, next, DEFAULT_THEME_STYLE)
      updateAppearance({ theme_style: next })
    },
    [updateAppearance],
  )

  const setPalette = useCallback(
    (next: Palette) => {
      setDevicePalette(next)
      writeStorage(PALETTE_STORAGE_KEY, next, DEFAULT_PALETTE)
      updateAppearance({ palette: next })
    },
    [updateAppearance],
  )

  const value = useMemo(
    () => ({ preference, resolved, setPreference, themeStyle, setThemeStyle, palette, setPalette }),
    [preference, resolved, setPreference, themeStyle, setThemeStyle, palette, setPalette],
  )

  return <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>
}

export function useTheme(): ThemeContextValue {
  const context = useContext(ThemeContext)
  if (!context) throw new Error("useTheme must be used within ThemeProvider")
  return context
}
