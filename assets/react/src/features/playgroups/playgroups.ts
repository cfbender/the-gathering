import { queryOptions, useMutation, useQueryClient } from "@tanstack/react-query"
import type { QueryClient } from "@tanstack/react-query"
import { api } from "@/lib/api"
import type { User } from "@/lib/auth"

export interface Playgroup {
  id: number
  name: string
  discord_guild_id: string | null
  /** Whether a sign-up invite link is active for this playgroup. */
  invite_enabled: boolean
  inserted_at: string
  updated_at: string
}

export interface PlaygroupMember {
  id: number
  username: string
  display_name: string
  role: "admin" | "member"
}

/** A playgroup as administrators see it: every one on the server, with its members. */
export interface AdminPlaygroup extends Playgroup {
  members: PlaygroupMember[]
}

export interface PlaygroupInput {
  name: string
  discord_guild_id: string
}

/** The playgroups the signed-in member belongs to. */
export const playgroupsQueryOptions = queryOptions({
  queryKey: ["playgroups"],
  queryFn: async () => (await api<{ data: Playgroup[] }>("/api/playgroups")).data,
})

export const adminPlaygroupsQueryOptions = queryOptions({
  queryKey: ["admin", "playgroups"],
  queryFn: async () => (await api<{ data: AdminPlaygroup[] }>("/api/admin/playgroups")).data,
})

/** Memberships changed: both lists, and the session when the member's own may have moved. */
export function invalidatePlaygroups(queryClient: QueryClient) {
  void queryClient.invalidateQueries({ queryKey: ["playgroups"] })
  void queryClient.invalidateQueries({ queryKey: ["admin", "playgroups"] })
}

/** Switches the signed-in member's active playgroup. Every resource query is scoped to it, so
 * the whole cache is invalidated once the session reflects the switch. */
export function useSwitchPlaygroup() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (playgroupId: number) =>
      (
        await api<{ data: User }>("/api/session/playgroup", {
          method: "PATCH",
          body: JSON.stringify({ playgroup_id: playgroupId }),
        })
      ).data,
    onSuccess: (user) => {
      queryClient.setQueryData(["session"], user)
      void queryClient.invalidateQueries({
        predicate: (query) => query.queryKey[0] !== "session" && query.queryKey[0] !== "playgroups",
      })
    },
  })
}

export const createPlaygroup = async (input: PlaygroupInput) =>
  (
    await api<{ data: AdminPlaygroup }>("/api/admin/playgroups", {
      method: "POST",
      body: JSON.stringify(input),
    })
  ).data

export const updatePlaygroup = async (id: number, input: PlaygroupInput) =>
  (
    await api<{ data: AdminPlaygroup }>(`/api/admin/playgroups/${id}`, {
      method: "PATCH",
      body: JSON.stringify(input),
    })
  ).data

export const deletePlaygroup = (id: number) =>
  api<void>(`/api/admin/playgroups/${id}`, { method: "DELETE" })

export const addPlaygroupMember = async (id: number, userId: number) =>
  (
    await api<{ data: AdminPlaygroup }>(`/api/admin/playgroups/${id}/members`, {
      method: "POST",
      body: JSON.stringify({ user_id: userId }),
    })
  ).data

export const removePlaygroupMember = (id: number, userId: number) =>
  api<void>(`/api/admin/playgroups/${id}/members/${userId}`, { method: "DELETE" })

export const playgroupInviteQueryOptions = (id: number) =>
  queryOptions({
    queryKey: ["admin", "playgroups", id, "registration-invite"],
    queryFn: async () =>
      (await api<{ data: { enabled: boolean } }>(`/api/admin/playgroups/${id}/registration-invite`))
        .data,
  })

/** Rotates a playgroup's reusable invite link and returns the new secret, shown once. */
export const rotatePlaygroupInvite = async (id: number) =>
  (
    await api<{ data: { token: string } }>(`/api/admin/playgroups/${id}/registration-invite`, {
      method: "POST",
    })
  ).data
