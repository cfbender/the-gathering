import { useCallback, useState } from "react"

/** Reads a device preference, or null when storage is unavailable. */
export function readStorage(key: string): string | null {
  try {
    return localStorage.getItem(key)
  } catch {
    return null
  }
}

/** Stores a device preference, removing the key for the default so only opt-outs persist. */
export function writeStorage(key: string, value: string, defaultValue: string) {
  try {
    if (value === defaultValue) localStorage.removeItem(key)
    else localStorage.setItem(key, value)
  } catch {
    // Storage may be unavailable; the in-memory value still applies.
  }
}

/** The stored value for `key` when it is valid, otherwise `defaultValue`. */
function storedChoice<T extends string>(
  key: string,
  defaultValue: T,
  isValid: (value: unknown) => value is T,
): T {
  const value = readStorage(key)
  return isValid(value) ? value : defaultValue
}

/**
 * A string choice remembered per browser under `key`, so a toggle keeps its value across
 * navigation and reloads. Invalid or missing stored values fall back to `defaultValue`.
 */
export function useStoredChoice<T extends string>(
  key: string,
  defaultValue: T,
  isValid: (value: unknown) => value is T,
) {
  const [value, setValue] = useState<T>(() => storedChoice(key, defaultValue, isValid))

  const setChoice = useCallback(
    (next: T) => {
      setValue(next)
      writeStorage(key, next, defaultValue)
    },
    [key, defaultValue],
  )

  return [value, setChoice] as const
}
