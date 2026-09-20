import { queryOptions } from "@tanstack/react-query"
import { api } from "@/lib/api"

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
