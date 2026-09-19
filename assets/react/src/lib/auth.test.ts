import { describe, expect, it } from "vite-plus/test"
import { ApiError } from "@/lib/api"
import { errorMessage, isSudoRequired, safeReturnTo } from "./auth"

describe("authentication error messages", () => {
  it("uses field errors for registration forms", () => {
    const error = new ApiError(422, "invalid", { username: ["has already been taken"] })
    expect(errorMessage(error, "username")).toBe("has already been taken")
    expect(errorMessage(error, "password")).toBeNull()
  })

  it("does not expose the generic Unauthorized response on failed login", () => {
    const error = new ApiError(401, "Unauthorized", { detail: "Unauthorized" })
    expect(errorMessage(error)).toBe("Username or password is incorrect.")
  })

  it("only accepts same-origin return paths", () => {
    expect(safeReturnTo("/settings")).toBe("/settings")
    expect(safeReturnTo("//example.com/account")).toBe("/")
    expect(safeReturnTo("https://example.com/account")).toBe("/")
  })

  it("recognizes the structured sudo-mode error", () => {
    expect(
      isSudoRequired(new ApiError(403, "Reauthentication required", { code: "sudo_required" })),
    ).toBe(true)
    expect(isSudoRequired(new ApiError(403, "Forbidden", { detail: "Forbidden" }))).toBe(false)
  })
})
