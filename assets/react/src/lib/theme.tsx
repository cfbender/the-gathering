import { createContext, useCallback, useContext, useEffect, useMemo, useState } from "react"
import type { ReactNode } from "react"

export type Theme = "light" | "dark"
export type ThemePreference = Theme | "system"
/** How surfaces render; orthogonal to the light/dark palette. */
export type ThemeStyle = "classic" | "glass"

// Must match the inline script in the SPA shell (AppController) so the first
// paint already has the right theme and style.
const STORAGE_KEY = "the-gathering:theme"
const STYLE_STORAGE_KEY = "the-gathering:theme-style"

function systemTheme(): Theme {
  return matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light"
}

function storedPreference(): ThemePreference {
  try {
    const value = localStorage.getItem(STORAGE_KEY)
    return value === "light" || value === "dark" ? value : "system"
  } catch {
    return "system"
  }
}

function storedThemeStyle(): ThemeStyle {
  try {
    return localStorage.getItem(STYLE_STORAGE_KEY) === "classic" ? "classic" : "glass"
  } catch {
    return "glass"
  }
}

interface ThemeContextValue {
  preference: ThemePreference
  resolved: Theme
  setPreference: (preference: ThemePreference) => void
  themeStyle: ThemeStyle
  setThemeStyle: (style: ThemeStyle) => void
}

const ThemeContext = createContext<ThemeContextValue | null>(null)

export function ThemeProvider({ children }: { children: ReactNode }) {
  const [preference, setPreferenceState] = useState<ThemePreference>(storedPreference)
  const [themeStyle, setThemeStyleState] = useState<ThemeStyle>(storedThemeStyle)
  const [system, setSystem] = useState<Theme>(systemTheme)

  useEffect(() => {
    const query = matchMedia("(prefers-color-scheme: dark)")
    const onChange = () => setSystem(systemTheme())
    query.addEventListener("change", onChange)
    return () => query.removeEventListener("change", onChange)
  }, [])

  const resolved = preference === "system" ? system : preference

  useEffect(() => {
    document.documentElement.dataset.theme = resolved
  }, [resolved])

  useEffect(() => {
    document.documentElement.dataset.themeStyle = themeStyle
  }, [themeStyle])

  const setPreference = useCallback((next: ThemePreference) => {
    setPreferenceState(next)
    try {
      if (next === "system") localStorage.removeItem(STORAGE_KEY)
      else localStorage.setItem(STORAGE_KEY, next)
    } catch {
      // Storage may be unavailable; the in-memory preference still applies.
    }
  }, [])

  const setThemeStyle = useCallback((next: ThemeStyle) => {
    setThemeStyleState(next)
    try {
      // Glass is the default, so only the opt-out is persisted.
      if (next === "glass") localStorage.removeItem(STYLE_STORAGE_KEY)
      else localStorage.setItem(STYLE_STORAGE_KEY, next)
    } catch {
      // Storage may be unavailable; the in-memory style still applies.
    }
  }, [])

  const value = useMemo(
    () => ({ preference, resolved, setPreference, themeStyle, setThemeStyle }),
    [preference, resolved, setPreference, themeStyle, setThemeStyle],
  )

  return <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>
}

export function useTheme(): ThemeContextValue {
  const context = useContext(ThemeContext)
  if (!context) throw new Error("useTheme must be used within ThemeProvider")
  return context
}
