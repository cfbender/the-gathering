/** Per-seat trackers a player keeps alongside life and counters: free-form counters ("Lands",
 * "Creatures in graveyard") and the anthems and combat buffs on their board, combined into
 * one total for every kind of creature. Shared ones ride presence like the other counters;
 * private ones stay in this browser. */

export const COUNTER_MAX = 100
export const PT_LIMIT = 99
export const MAX_COUNTERS = 20
export const MAX_EFFECTS = 30
/** Rows grow as 2^tags; past this many distinct conditions the rest are ignored. */
const MAX_TAGS = 10

export interface CustomCounter {
  id: string
  label: string
  value: number
  shared: boolean
}

export interface CombatEffect {
  id: string
  name: string
  power: number
  toughness: number
  /** Every condition must hold for a creature to get the buff ("Token" + "Attacking"). */
  conditions: string[]
  keywords: string[]
}

export interface SeatTrackers {
  counters: CustomCounter[]
  effects: CombatEffect[]
  shareEffects: boolean
}

export const EMPTY_TRACKERS: SeatTrackers = { counters: [], effects: [], shareEffects: false }

export type SharedCounter = Omit<CustomCounter, "shared">

/** What a seat publishes about its trackers; mirrors the channel's `update_status` fields. */
export interface SharedTrackers {
  custom_counters: SharedCounter[]
  combat_effects: CombatEffect[]
}

export const PRESET_CONDITIONS = [
  { id: "attacking", label: "Attacking", word: "attacking" },
  { id: "blocking", label: "Blocking", word: "blocking" },
  { id: "flying", label: "Flying", word: "flying" },
  { id: "token", label: "Tokens", word: "token" },
  { id: "nontoken", label: "Nontoken", word: "nontoken" },
] as const

export type PresetCondition = (typeof PRESET_CONDITIONS)[number]["id"]

const PRESET_IDS: readonly string[] = PRESET_CONDITIONS.map((preset) => preset.id)
/** Tokenness reads last ("attacking Elf token creatures"); custom types sit between. */
const TRAILING_PRESETS: readonly string[] = ["token", "nontoken"]

export function conditionKey(condition: string) {
  return condition.trim().toLowerCase()
}

export function isPresetCondition(condition: string): condition is PresetCondition {
  return PRESET_IDS.includes(conditionKey(condition))
}

export function newId() {
  return crypto.randomUUID().slice(0, 8)
}

export function clampCounter(value: number) {
  return Number.isFinite(value) ? Math.max(0, Math.min(COUNTER_MAX, Math.round(value))) : 0
}

export function clampBuff(value: number) {
  return Number.isFinite(value) ? Math.max(-PT_LIMIT, Math.min(PT_LIMIT, Math.round(value))) : 0
}

export function newCounter(index: number): CustomCounter {
  return { id: newId(), label: `Counter ${index}`, value: 0, shared: false }
}

export function newEffect(): CombatEffect {
  return { id: newId(), name: "", power: 1, toughness: 1, conditions: [], keywords: [] }
}

/** Splits a typed list ("vigilance, trample") into trimmed, case-insensitively unique items. */
export function parseList(text: string): string[] {
  const seen = new Set<string>()
  return text
    .split(",")
    .map((item) => item.trim())
    .filter((item) => {
      const key = item.toLowerCase()
      if (!item || seen.has(key)) return false
      seen.add(key)
      return true
    })
}

export function sharedTrackers(trackers: SeatTrackers): SharedTrackers {
  return {
    custom_counters: trackers.counters
      .filter((counter) => counter.shared)
      .map(({ id, label, value }) => ({ id, label, value })),
    combat_effects: trackers.shareEffects ? trackers.effects : [],
  }
}

/** Another seat's published trackers, as this seat's shape for the shared overlay. */
export function receivedTrackers(
  participant: Partial<SharedTrackers> | undefined | null,
): SeatTrackers {
  return {
    counters: (participant?.custom_counters ?? []).map((counter) => ({ ...counter, shared: true })),
    effects: participant?.combat_effects ?? [],
    shareEffects: true,
  }
}

export function adjustCounter(trackers: SeatTrackers, id: string, delta: number): SeatTrackers {
  return {
    ...trackers,
    counters: trackers.counters.map((counter) =>
      counter.id === id ? { ...counter, value: clampCounter(counter.value + delta) } : counter,
    ),
  }
}

export function formatBuff(power: number, toughness: number) {
  const signed = (value: number) => (value < 0 ? `${value}` : `+${value}`)
  return `${signed(power)}/${signed(toughness)}`
}

function orderConditions(conditions: string[]) {
  const rank = (condition: string) => {
    const key = conditionKey(condition)
    if (TRAILING_PRESETS.includes(key)) return 2 + TRAILING_PRESETS.indexOf(key)
    if (PRESET_IDS.includes(key)) return 0
    return 1
  }
  return [...conditions].sort((a, b) => {
    const byRank = rank(a) - rank(b)
    if (byRank !== 0) return byRank
    const presets = PRESET_IDS.indexOf(conditionKey(a)) - PRESET_IDS.indexOf(conditionKey(b))
    return rank(a) === 0 ? presets : a.localeCompare(b)
  })
}

/** "Attacking token creatures": presets read as adjectives, custom conditions as typed. */
export function describeConditions(conditions: string[]) {
  if (conditions.length === 0) return "All creatures"
  const words = orderConditions(conditions).map((condition) => {
    const preset = PRESET_CONDITIONS.find((item) => item.id === conditionKey(condition))
    return preset ? preset.word : condition.trim()
  })
  const [first = "", ...rest] = words
  return [first.charAt(0).toUpperCase() + first.slice(1), ...rest, "creatures"].join(" ")
}

export interface CombatRow {
  /** Stable across re-renders: the ordered condition keys. */
  key: string
  conditions: string[]
  label: string
  power: number
  toughness: number
  keywords: string[]
}

interface Total {
  power: number
  toughness: number
  keywords: string[]
}

function totalFor(effects: CombatEffect[], present: ReadonlySet<string>): Total {
  const total: Total = { power: 0, toughness: 0, keywords: [] }
  const seen = new Set<string>()
  for (const effect of effects) {
    if (!effect.conditions.every((condition) => present.has(conditionKey(condition)))) continue
    total.power += effect.power
    total.toughness += effect.toughness
    for (const keyword of effect.keywords) {
      const key = keyword.toLowerCase()
      if (seen.has(key)) continue
      seen.add(key)
      total.keywords.push(keyword)
    }
  }
  return total
}

function sameTotal(a: Total, b: Total) {
  return (
    a.power === b.power &&
    a.toughness === b.toughness &&
    a.keywords.length === b.keywords.length &&
    a.keywords.every((keyword, index) => keyword.toLowerCase() === b.keywords[index]?.toLowerCase())
  )
}

function isEmpty(total: Total) {
  return total.power === 0 && total.toughness === 0 && total.keywords.length === 0
}

/**
 * The combined buff for every kind of creature on the board. A creature matching a set of
 * conditions gets each effect whose conditions it satisfies, so "attacking tokens" sum the
 * token anthems and the attack triggers. Only combinations where every condition changes the
 * result are listed: dropping any one of them would read the same.
 */
export function combatRows(effects: CombatEffect[]): CombatRow[] {
  const tags = new Map<string, string>()
  for (const effect of effects)
    for (const condition of effect.conditions) {
      const key = conditionKey(condition)
      if (key && !tags.has(key) && tags.size < MAX_TAGS) tags.set(key, condition.trim())
    }
  const keys = [...tags.keys()]
  const totals = new Map<number, Total>()
  const totalOf = (mask: number) => {
    let total = totals.get(mask)
    if (!total) {
      total = totalFor(effects, new Set(keys.filter((_, index) => mask & (1 << index))))
      totals.set(mask, total)
    }
    return total
  }

  const rows: CombatRow[] = []
  for (let mask = 0; mask < 1 << keys.length; mask++) {
    const total = totalOf(mask)
    if (isEmpty(total)) continue
    const matters = keys.every(
      (_, index) => !(mask & (1 << index)) || !sameTotal(total, totalOf(mask & ~(1 << index))),
    )
    if (!matters) continue
    const conditions = orderConditions(
      keys.filter((_, index) => mask & (1 << index)).map((key) => tags.get(key)!),
    )
    rows.push({
      key: conditions.map(conditionKey).join("+") || "all",
      conditions,
      label: describeConditions(conditions),
      power: total.power,
      toughness: total.toughness,
      keywords: total.keywords,
    })
  }
  // Broad rows first, then the narrower combinations that build on them.
  return rows.sort(
    (a, b) => a.conditions.length - b.conditions.length || a.label.localeCompare(b.label),
  )
}

function isStringList(value: unknown): value is string[] {
  return Array.isArray(value) && value.every((item) => typeof item === "string")
}

function savedCounter(value: unknown): CustomCounter | null {
  if (!value || typeof value !== "object") return null
  const { id, label, shared, value: count } = value as Record<string, unknown>
  if (typeof id !== "string" || typeof label !== "string" || typeof count !== "number") return null
  return { id, label, value: clampCounter(count), shared: shared === true }
}

function savedEffect(value: unknown): CombatEffect | null {
  if (!value || typeof value !== "object") return null
  const { id, name, power, toughness, conditions, keywords } = value as Record<string, unknown>
  if (typeof id !== "string" || typeof name !== "string") return null
  if (typeof power !== "number" || typeof toughness !== "number") return null
  return {
    id,
    name,
    power: clampBuff(power),
    toughness: clampBuff(toughness),
    conditions: isStringList(conditions) ? conditions : [],
    keywords: isStringList(keywords) ? keywords : [],
  }
}

/** Restores trackers saved by this browser, dropping anything malformed. */
export function parseSavedTrackers(saved: unknown): SeatTrackers {
  if (!saved || typeof saved !== "object") return EMPTY_TRACKERS
  const { counters, effects, shareEffects } = saved as Record<string, unknown>
  return {
    counters: Array.isArray(counters)
      ? counters.map(savedCounter).filter((counter) => counter !== null)
      : [],
    effects: Array.isArray(effects)
      ? effects.map(savedEffect).filter((effect) => effect !== null)
      : [],
    shareEffects: shareEffects === true,
  }
}
