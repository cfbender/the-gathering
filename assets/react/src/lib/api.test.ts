import { describe, expect, it } from "vite-plus/test"
import { ApiError } from "./api"

describe("ApiError.fieldErrors", () => {
  it("returns plain message lists unchanged", () => {
    const error = new ApiError(422, "invalid", { seats: ["must contain between 2 and 10 players"] })
    expect(error.fieldErrors("seats")).toEqual(["must contain between 2 and 10 players"])
    expect(error.fieldErrors("notes")).toEqual([])
  })

  it("flattens nested cast_assoc row errors into readable strings", () => {
    const error = new ApiError(422, "invalid", {
      seats: [{}, { seat: ["has already been taken"] }, {}, { deck_id: ["does not exist"] }],
    })
    expect(error.fieldErrors("seats")).toEqual([
      "Seat 2: seat has already been taken",
      "Seat 4: deck does not exist",
    ])
  })
})
