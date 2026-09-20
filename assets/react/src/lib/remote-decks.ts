import { queryOptions } from "@tanstack/react-query"
import { api } from "@/lib/api"
import type { User } from "@/lib/auth"
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

export interface RemoteDeckSyncResult {
  created: number
  updated: number
  errors: { source: RemoteDeckSource; error: string }[]
}

/** Folds the user's hosted decks into their player's deck list (see SyncRemoteDecks). */
export function syncRemoteDecks() {
  return api<{ data: RemoteDeckSyncResult }>("/api/session/remote-decks/sync", {
    method: "POST",
  }).then((body) => body.data)
}

/** Whether the user has any deck host to sync from; mirrors the server's check. */
export function hasDeckHost(user: User | undefined): boolean {
  return Boolean(
    user &&
    (user.moxfield_username ||
      user.archidekt_username ||
      (user.manavault_url && user.has_manavault_api_key)),
  )
}

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
