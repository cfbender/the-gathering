import { act, cleanup, fireEvent, render, screen } from "@testing-library/react"
import { StrictMode, type ReactNode } from "react"
import { afterEach, expect, it, vi } from "vite-plus/test"
import { ApiError } from "@/lib/api"
import { RouteError, errorMessage } from "./route-error"
import { ToastProvider } from "./ui/toast"

vi.mock("@tanstack/react-router", () => ({
  Link: ({ children }: { children: ReactNode }) => <a>{children}</a>,
}))

afterEach(() => {
  cleanup()
  vi.useRealTimers()
})

it("describes API failures by detail, status, or the crash message", () => {
  expect(errorMessage(new ApiError(404, "GET /x: Not Found", { detail: "Not Found" }))).toBe(
    "Not Found",
  )
  expect(errorMessage(new ApiError(422, "PATCH /x: Unprocessable", { seats: [{}] }))).toBe(
    "The server rejected the change. Check the form and try again.",
  )
  expect(errorMessage(new ApiError(500, "GET /x: Internal Server Error"))).toBe(
    "Request failed (500).",
  )
  expect(errorMessage(new TypeError("Objects are not valid as a React child"))).toBe(
    "Objects are not valid as a React child",
  )
  expect(errorMessage("boom")).toBe("Something went wrong.")
})

it("toasts the failure once, offers a retry, and the toast dismisses on its own", () => {
  vi.useFakeTimers()
  const reset = vi.fn()
  const error = new Error("Objects are not valid as a React child")
  render(
    <ToastProvider>
      <RouteError error={error} reset={reset} info={{ componentStack: "" }} />
    </ToastProvider>,
  )
  const alerts = screen.getAllByRole("alert")
  expect(alerts).toHaveLength(2)
  expect(alerts.every((alert) => alert.textContent?.includes(error.message))).toBe(true)
  expect(screen.getByRole("heading", { name: "Something went wrong" })).toBeTruthy()

  fireEvent.click(screen.getByRole("button", { name: "Try again" }))
  expect(reset).toHaveBeenCalledOnce()

  act(() => {
    vi.advanceTimersByTime(6_000)
  })
  expect(screen.getAllByRole("alert")).toHaveLength(1)
})

it("keeps a single toast under StrictMode's double mount", () => {
  render(
    <StrictMode>
      <ToastProvider>
        <RouteError error={new Error("twice?")} reset={() => {}} info={{ componentStack: "" }} />
      </ToastProvider>
    </StrictMode>,
  )
  // The panel itself is one alert; exactly one toast should join it.
  expect(screen.getAllByRole("alert")).toHaveLength(2)
})

it("lets the player dismiss a toast early", () => {
  render(
    <ToastProvider>
      <RouteError error={new Error("nope")} reset={() => {}} info={{ componentStack: "" }} />
    </ToastProvider>,
  )
  fireEvent.click(screen.getByRole("button", { name: "Dismiss" }))
  expect(screen.getAllByRole("alert")).toHaveLength(1)
})
