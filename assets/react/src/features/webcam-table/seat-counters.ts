import type { DeckSummary } from "@/features/decks/decks"

export interface SeatCounters {
  poison: number
  rad: number
  commander_casts: Record<string, number>
  /** Player id, then commander name: identical commanders at two seats stay separate. */
  commander_damage: Record<string, Record<string, number>>
}

export const EMPTY_COUNTERS: SeatCounters = {
  poison: 0,
  rad: 0,
  commander_casts: {},
  commander_damage: {},
}

export type Counter =
  | { kind: "poison" | "rad" }
  | { kind: "casts"; commander: string }
  | { kind: "damage"; playerId: number; commander: string }

export function commanderNames(
  deck: Pick<DeckSummary, "commander_name" | "partner_name"> | undefined,
) {
  return deck
    ? [deck.commander_name, deck.partner_name].filter((name): name is string => !!name)
    : []
}

export function counterValue(state: SeatCounters, counter: Counter): number {
  switch (counter.kind) {
    case "poison":
    case "rad":
      return state[counter.kind]
    case "casts":
      return state.commander_casts[counter.commander] ?? 0
    case "damage":
      return state.commander_damage[counter.playerId]?.[counter.commander] ?? 0
  }
}

export function changeCounter(state: SeatCounters, counter: Counter, delta: number): SeatCounters {
  const value = Math.max(0, Math.min(999, counterValue(state, counter) + delta))
  switch (counter.kind) {
    case "poison":
    case "rad":
      return { ...state, [counter.kind]: value }
    case "casts":
      return { ...state, commander_casts: { ...state.commander_casts, [counter.commander]: value } }
    case "damage":
      return {
        ...state,
        commander_damage: {
          ...state.commander_damage,
          [counter.playerId]: {
            ...state.commander_damage[counter.playerId],
            [counter.commander]: value,
          },
        },
      }
  }
}

export function counterWarning(state: SeatCounters) {
  return (
    state.poison >= 10 ||
    Object.values(state.commander_damage).some((counts) =>
      Object.values(counts).some((damage) => damage >= 21),
    )
  )
}
