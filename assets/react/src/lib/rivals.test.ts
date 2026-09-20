import { describe, expect, it } from "vite-plus/test"
import { favoritePrey, favoriteVictim, nemesisCommander, nemesisPlayer } from "./rivals"

const players = [
  { id: 1, name: "Zelda", games: 14, wins: 6, losses: 4, draws: 4 },
  { id: 2, name: "Bea", games: 11, wins: 7, losses: 5, draws: 0 },
  { id: 3, name: "Ari", games: 16, wins: 7, losses: 5, draws: 4 },
  { id: 4, name: "Cal", games: 16, wins: 3, losses: 5, draws: 8 },
]

const commanders = [
  { id: "z", name: "Zimone", faced: 9, beat_me: 3, beaten: 4 },
  { id: "b", name: "Blex", faced: 12, beat_me: 4, beaten: 5 },
  { id: "a", name: "Alela", faced: 15, beat_me: 4, beaten: 5 },
  { id: "c", name: "Chainer", faced: 15, beat_me: 2, beaten: 5 },
]

describe("player rival selection", () => {
  it("picks the most losses, then shared games, then name for a nemesis", () => {
    expect(nemesisPlayer(players)?.name).toBe("Ari")
  })

  it("picks the most wins, then shared games, then name for a favorite victim", () => {
    expect(favoriteVictim(players)?.name).toBe("Ari")
  })

  it("returns undefined without an opponent", () => {
    expect(nemesisPlayer([])).toBeUndefined()
  })
})

describe("commander rival selection", () => {
  it("picks the most losses to a commander, then appearances, then name", () => {
    expect(nemesisCommander(commanders)?.name).toBe("Alela")
  })

  it("picks the most commanders beaten, then appearances, then name", () => {
    expect(favoritePrey(commanders)?.name).toBe("Alela")
  })
})
