import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react"
import { afterEach, expect, it, vi } from "vite-plus/test"
import { RulingsTab } from "./rulings-tab"

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
  sessionStorage.clear()
})

const status = {
  enabled: true,
  model: "anthropic/claude-sonnet-5.5",
  rules_effective_on: "2026-09-25",
  usage: { spent_usd: 0.25, limit_usd: 2, resets_on: "2026-11-01" },
}

function renderTab(client: QueryClient) {
  return render(
    <QueryClientProvider client={client}>
      <RulingsTab roomId="room-1" tableCards={["Questing Beast", "Sol Ring"]} />
    </QueryClientProvider>,
  )
}

function ask(question: string) {
  const box = screen.getByRole("textbox", { name: "Rules question" })
  fireEvent.change(box, { target: { value: question } })
  fireEvent.keyDown(box, { key: "Enter" })
}

it("asks with the table's cards and earlier turns, and keeps the conversation", async () => {
  const bodies: unknown[] = []
  let budgetSpent = false
  vi.stubGlobal(
    "fetch",
    vi.fn((url: string, init?: RequestInit) => {
      if (url === "/api/ai-rulings" && init?.method === "POST") {
        const body = JSON.parse(init.body as string) as { question: string }
        bodies.push(body)
        if (budgetSpent)
          return Promise.resolve(
            Response.json(
              {
                errors: {
                  code: "ai_budget_exhausted",
                  detail: "You have used this month's $2.00 of AI rulings.",
                },
              },
              { status: 429 },
            ),
          )
        return Promise.resolve(
          Response.json({
            data: {
              question: body.question,
              signature: `signature-${bodies.length}`,
              answer: `**Answer ${bodies.length}.** [[Questing Beast]] has deathtouch; Sol Ring does not (CR 702.2c).`,
              model: "anthropic/claude-sonnet-5.5",
              rules: [{ number: "702.2c", text: "Any nonzero amount of combat damage…" }],
              cards: [
                { name: "Questing Beast", id: "card-1" },
                { name: "Sol Ring", id: "card-2" },
              ],
            },
          }),
        )
      }
      if (url === "/api/ai-rulings") return Promise.resolve(Response.json({ data: status }))
      return Promise.resolve(Response.json({ data: null }))
    }),
  )
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  const tab = renderTab(client)
  await screen.findByText(/\$0\.25 of \$2\.00 used this month/)

  // The table's cards are sent only after opting in.
  ask("Deathtouch and trample?")
  expect((await screen.findByText("Answer 1.")).tagName).toBe("STRONG")
  expect(bodies[0]).toEqual({ question: "Deathtouch and trample?", history: [], cards: [] })
  // Card names render as hover targets, bracketed or not, without literal brackets.
  expect(screen.getByText("Questing Beast").className).toContain("underline")
  expect(screen.getByText("Sol Ring").className).toContain("underline")
  expect(screen.queryByText(/\[\[/)).toBeNull()
  expect(screen.getByText("Rules cited (1)")).toBeTruthy()

  // Switching tabs remounts the panel; the conversation and its context carry over.
  tab.unmount()
  renderTab(client)
  await screen.findByText("Answer 1.")
  const include = screen.getByRole<HTMLInputElement>("checkbox", {
    name: /Include the 2 cards in play/,
  })
  expect(include.checked).toBe(false)
  fireEvent.click(include)
  ask("And with first strike?")
  await screen.findByText("Answer 2.")
  expect(bodies[1]).toMatchObject({
    question: "And with first strike?",
    history: [
      {
        question: "Deathtouch and trample?",
        answer: expect.stringContaining("Answer 1"),
        signature: "signature-1",
      },
    ],
    cards: ["Questing Beast", "Sol Ring"],
  })

  budgetSpent = true
  ask("One more?")
  expect((await screen.findByRole("alert")).textContent).toBe(
    "You have used this month's $2.00 of AI rulings.",
  )

  fireEvent.click(screen.getByRole("button", { name: /New conversation/ }))
  await waitFor(() => expect(screen.queryByText("Answer 1.")).toBeNull())
  expect(sessionStorage.getItem("the-gathering:rulings:room-1")).toBeNull()
})

it("explains when rulings are not set up", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn((url: string) =>
      Promise.resolve(
        Response.json({ data: url === "/api/ai-rulings" ? { ...status, enabled: false } : null }),
      ),
    ),
  )
  renderTab(new QueryClient({ defaultOptions: { queries: { retry: false } } }))
  await screen.findByText(/AI rulings are not set up on this server/)
  expect(screen.queryByRole("textbox", { name: "Rules question" })).toBeNull()
})
