import { queryOptions, useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import type { QueryClient } from "@tanstack/react-query"
import { redirect } from "@tanstack/react-router"
import { ApiError, api } from "@/lib/api"
import type { Palette, ThemeStyle } from "@/lib/theme"

export interface User {
  id: number
  username: string
  display_name: string
  discord_id: string | null
  avatar_url: string | null
  moxfield_username: string | null
  archidekt_username: string | null
  manavault_url: string | null
  has_manavault_api_key: boolean
  has_password: boolean
  role: "admin" | "member"
  disabled: boolean
  palette: Palette
  theme_style: ThemeStyle
  inserted_at: string
}

interface Data<T> {
  data: T
}

export interface RegistrationStatus {
  allowed: boolean
  bootstrap: boolean
  discord_configured: boolean
}

export const sessionQueryOptions = queryOptions({
  queryKey: ["session"],
  queryFn: async () => (await api<Data<User>>("/api/session")).data,
  retry: false,
})

export const registrationQueryOptions = queryOptions({
  queryKey: ["registration"],
  queryFn: async () => (await api<Data<RegistrationStatus>>("/api/registration")).data,
})

export function useCurrentUser() {
  return useQuery(sessionQueryOptions)
}

export function safeReturnTo(value: unknown) {
  return typeof value === "string" && value.startsWith("/") && !value.startsWith("//") ? value : "/"
}

export async function requireUser(queryClient: QueryClient, returnTo: string) {
  try {
    return await queryClient.ensureQueryData(sessionQueryOptions)
  } catch (error) {
    if (error instanceof ApiError && error.status === 401) {
      throw redirect({ to: "/login", search: { returnTo, error: undefined } })
    }
    throw error
  }
}

export async function requireAdmin(queryClient: QueryClient, returnTo: string) {
  const user = await requireUser(queryClient, returnTo)
  if (user.role !== "admin") throw redirect({ to: "/" })
  return user
}

export function errorMessage(error: unknown, field?: string) {
  if (!(error instanceof ApiError)) return error instanceof Error ? error.message : null
  if (field) return error.fieldErrors(field)[0] ?? null
  if (error.status === 401) return "Username or password is incorrect."
  if (error.status === 429) return "Too many attempts. Wait a few minutes and try again."
  return error.detail
}

export function isSudoRequired(error: unknown) {
  return error instanceof ApiError && error.errors.code === "sudo_required"
}

export function useLogin() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (credentials: { username: string; password: string }) =>
      (
        await api<Data<User>>("/api/session", {
          method: "POST",
          body: JSON.stringify(credentials),
        })
      ).data,
    onSuccess: (user) => queryClient.setQueryData(["session"], user),
  })
}

export function useLogout() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: () => api<void>("/api/session", { method: "DELETE" }),
    onSuccess: () => queryClient.removeQueries({ queryKey: ["session"] }),
  })
}

export function useRegister() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (user: { username: string; display_name: string; password: string }) =>
      (
        await api<Data<User>>("/api/users", {
          method: "POST",
          body: JSON.stringify({ user }),
        })
      ).data,
    onSuccess: (user) => {
      queryClient.setQueryData(["session"], user)
      void queryClient.invalidateQueries({ queryKey: ["registration"] })
    },
  })
}
