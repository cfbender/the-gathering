import { api, ApiError } from "@/lib/api"
import { errorMessage } from "@/lib/auth"

interface Data<T> {
  data: T
}

/** `GET /api/ai-rulings`: whether rulings are set up, and the member's spending this month. */
export interface RulingStatus {
  enabled: boolean
  model: string | null
  rules_effective_on: string | null
  usage: { spent_usd: number; limit_usd: number | null; resets_on: string }
}

export interface CitedRule {
  number: string
  text: string
}

/** One answered question, as kept in the conversation. */
export interface RulingTurn {
  question: string
  answer: string
  /** Lets the server trust this turn when it comes back as history. */
  signature: string
  model: string
  rules: CitedRule[]
  cards: { name: string; id: string }[]
}

export const rulingStatusKey = ["ai-rulings"] as const

export async function getRulingStatus() {
  return (await api<Data<RulingStatus>>("/api/ai-rulings")).data
}

/** Asks a question; earlier turns of the conversation and the table's cards give it context. */
export async function askRuling(question: string, history: RulingTurn[], cards: string[]) {
  const { data } = await api<Data<RulingTurn>>("/api/ai-rulings", {
    method: "POST",
    body: JSON.stringify({
      question,
      history: history.map(({ question, answer, signature }) => ({ question, answer, signature })),
      cards,
    }),
  })
  return data
}

/** Why a question went unanswered; a spent budget explains when it resets. */
export function rulingErrorMessage(error: unknown) {
  if (error instanceof ApiError && error.errors.code === "ai_budget_exhausted") return error.detail
  return errorMessage(error)
}

export function formatUsd(amount: number) {
  return amount.toLocaleString(undefined, {
    style: "currency",
    currency: "USD",
    minimumFractionDigits: 2,
    maximumFractionDigits: amount > 0 && amount < 0.01 ? 4 : 2,
  })
}

/** `GET /api/admin/ai-settings`. */
export interface AiSettings {
  provider: "openrouter"
  model: string | null
  has_api_key: boolean
  monthly_limit_usd: number | null
  usage_month: string
  usage: { user_id: number; display_name: string; questions: number; spent_usd: number }[]
}

export type AiSettingsUpdate = Partial<{
  model: string | null
  api_key: string | null
  monthly_limit_usd: number | null
}>

export async function getAiSettings() {
  return (await api<Data<AiSettings>>("/api/admin/ai-settings")).data
}

export async function updateAiSettings(update: AiSettingsUpdate) {
  return (
    await api<Data<AiSettings>>("/api/admin/ai-settings", {
      method: "PATCH",
      body: JSON.stringify(update),
    })
  ).data
}

/** `GET /api/admin/rules`: the installed Comprehensive Rules and the last update check. */
export interface RulesStatus {
  source_url: string | null
  effective_on: string | null
  rule_count: number
  glossary_count: number
  installed_at: string | null
  checked_at: string | null
  last_error: string | null
}

export async function getRulesStatus() {
  return (await api<Data<RulesStatus>>("/api/admin/rules")).data
}

export async function syncRules() {
  return (
    await api<Data<{ outcome: "installed" | "current"; status: RulesStatus }>>(
      "/api/admin/rules/sync",
      { method: "POST" },
    )
  ).data
}

/** A date such as `2026-09-25`, shown in the reader's locale without shifting time zones. */
export function formatDate(date: string) {
  // Without a zone, a date-time string is local time, so the calendar day stays put.
  return new Date(`${date}T00:00:00`).toLocaleDateString(undefined, {
    year: "numeric",
    month: "long",
    day: "numeric",
  })
}
