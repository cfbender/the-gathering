import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react"
import { afterEach, expect, it, vi } from "vite-plus/test"
import { RegistrationInvite } from "./registration-invite"

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
})

it("creates and copies a reusable link, and only rotates after confirmation", async () => {
  let rotations = 0
  vi.stubGlobal(
    "fetch",
    vi.fn((_url: string, init?: RequestInit) => {
      if (init?.method === "POST") {
        rotations++
        return Promise.resolve(Response.json({ data: { token: `invite-${rotations}` } }))
      }
      return Promise.resolve(Response.json({ data: { enabled: rotations > 0 } }))
    }),
  )
  const writeText = vi.fn().mockResolvedValue(undefined)
  vi.stubGlobal("navigator", { clipboard: { writeText } })
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  const page = render(
    <QueryClientProvider client={client}>
      <RegistrationInvite />
    </QueryClientProvider>,
  )

  await waitFor(() =>
    expect(screen.getByRole<HTMLButtonElement>("button", { name: "Create link" }).disabled).toBe(
      false,
    ),
  )
  expect(rotations).toBe(0)
  fireEvent.click(screen.getByRole("button", { name: "Create link" }))
  const input = await screen.findByRole<HTMLInputElement>("textbox", {
    name: "Shareable sign-up link",
  })
  expect(input.value).toBe(`${window.location.origin}/invite#token=invite-1`)
  fireEvent.click(screen.getByRole("button", { name: "Copy link" }))
  await screen.findByText("Link copied.")
  expect(writeText).toHaveBeenCalledWith(`${window.location.origin}/invite#token=invite-1`)

  fireEvent.click(await screen.findByRole("button", { name: "Rotate link" }))
  const dialog = await screen.findByRole("alertdialog", { name: "Rotate sign-up link?" })
  expect(rotations).toBe(1)
  fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }))
  expect(rotations).toBe(1)
  fireEvent.click(screen.getByRole("button", { name: "Rotate link" }))
  fireEvent.click(
    within(await screen.findByRole("alertdialog")).getByRole("button", { name: "Rotate link" }),
  )
  await waitFor(() =>
    expect(
      screen.getByRole<HTMLInputElement>("textbox", { name: "Shareable sign-up link" }).value,
    ).toBe(`${window.location.origin}/invite#token=invite-2`),
  )
  expect(screen.queryByText("Link copied.")).toBeNull()

  page.unmount()
  render(
    <QueryClientProvider client={client}>
      <RegistrationInvite />
    </QueryClientProvider>,
  )
  await screen.findByRole("button", { name: "Rotate link" })
  expect(screen.queryByRole("textbox", { name: "Shareable sign-up link" })).toBeNull()
  expect(rotations).toBe(2)
})
