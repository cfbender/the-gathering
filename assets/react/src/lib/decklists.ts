import { useMutation } from "@tanstack/react-query"
import { api } from "@/lib/api"

export type DecklistSource = "moxfield" | "archidekt" | "manavault" | "other"

export interface Decklist {
  source: Exclude<DecklistSource, "other">
  id: string
  url: string
  name: string
  commanders: Array<{ name: string }>
  color_identity?: string[]
  author?: string
  card_count?: number
  fetched_at: string
}

/**
 * Hostname of the self-hosted ManaVault instance the server was configured with
 * (`MANAVAULT_URL`), embedded by the SPA shell as `<meta name="manavault-url">`.
 */
export function manavaultHost(): string | null {
  const content = document.querySelector<HTMLMetaElement>('meta[name="manavault-url"]')?.content
  if (!content) return null
  try {
    return new URL(content).hostname.toLowerCase()
  } catch {
    return null
  }
}

export function detectDecklistSource(value: string): DecklistSource | null {
  try {
    const url = new URL(value)
    if (url.protocol !== "http:" && url.protocol !== "https:") return null

    const host = url.hostname.replace(/^www\./, "").toLowerCase()
    if (host === "moxfield.com" && /^\/decks\/[^/]+/.test(url.pathname)) return "moxfield"
    if (host === "archidekt.com" && /^\/decks\/\d+/.test(url.pathname)) return "archidekt"
    const manavault = manavaultHost()
    if (manavault && host === manavault && /^\/share\/decks\/[^/]+/.test(url.pathname)) {
      return "manavault"
    }
    return "other"
  } catch {
    return value.trim() === "" ? null : "other"
  }
}

export function useResolveDecklist() {
  return useMutation({
    mutationFn: async (url: string) => {
      const response = await api<{ data: Decklist }>("/api/decklists/resolve", {
        method: "POST",
        body: JSON.stringify({ url }),
      })
      return response.data
    },
  })
}

export const decklistSourceLabels: Record<DecklistSource, string> = {
  moxfield: "Moxfield",
  archidekt: "Archidekt",
  manavault: "ManaVault",
  other: "Deck link",
}
