import { queryOptions } from "@tanstack/react-query"
import { api } from "@/lib/api"
import type { Decklist } from "@/lib/decklists"

export type RemoteDeckSource = "moxfield" | "archidekt" | "manavault"

export interface RemoteDeck {
  name: string
  commanders: string[]
  color_identity: string[]
  url: string
  source: RemoteDeckSource
  updated_at: string | null
}

export interface RemoteDeckSourceStatus {
  source: RemoteDeckSource
  configured: boolean
  error: string | null
}

export interface RemoteDecksResult {
  decks: RemoteDeck[]
  sources: RemoteDeckSourceStatus[]
}

export const remoteDecksQueryOptions = queryOptions({
  queryKey: ["remote-decks"],
  queryFn: async () => (await api<{ data: RemoteDecksResult }>("/api/session/remote-decks")).data,
  staleTime: 5 * 60 * 1000,
})

export const remoteDeckSourceLabels: Record<RemoteDeckSource, string> = {
  moxfield: "Moxfield",
  archidekt: "Archidekt",
  manavault: "ManaVault",
}

/**
 * ManaVault decks come from the owner's authenticated API listing, so they may be
 * private and cannot be re-resolved through the public share endpoint. The listing
 * already carries everything the deck form needs, so build the deck list locally.
 */
export function decklistFromRemoteDeck(deck: RemoteDeck): Decklist {
  return {
    source: deck.source,
    id: deck.url,
    url: deck.url,
    name: deck.name,
    commanders: deck.commanders.map((name) => ({ name })),
    color_identity: deck.color_identity,
    fetched_at: deck.updated_at ?? new Date().toISOString(),
  }
}
