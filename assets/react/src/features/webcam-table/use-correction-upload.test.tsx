import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { fireEvent, render, screen, waitFor } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vite-plus/test"
import { api } from "@/lib/api"
import type { Identification } from "./recognition/messages"
import {
  CorrectionPreference,
  correctionPayload,
  useCorrectionUpload,
} from "./use-correction-upload"
import type { CapturedCard } from "./use-webcam-room"

vi.mock("@/lib/api", () => ({ api: vi.fn() }))

const capture: CapturedCard = {
  peerId: "owner",
  playerId: 1,
  image: "data:image/jpeg;base64,/9j/",
  nativeWidth: 1920,
  nativeHeight: 1080,
  cropSize: 640,
  clickX: 123,
  clickY: 456,
  inspect: true,
  private: false,
  shareCorrections: true,
}
const result: Identification = {
  quad: [
    [40, 20],
    [290, 20],
    [290, 370],
    [40, 370],
  ],
  upVote: 0.93,
  candidates: [
    { id: "wrong", name: "Wrong", set: "abc", frame: "modern", index: 0, score: 0.73 },
    { id: "right", name: "Right", set: "abc", frame: "modern", index: 1, score: 0.7 },
  ],
  timings: { detector: 1, embed: 2, search: 3, total: 6 },
}

beforeEach(() => {
  localStorage.clear()
  vi.mocked(api).mockReset()
})

describe("correction labels", () => {
  it("preserves a back-face gallery ID as the correction label", () => {
    const face = "b0a96416-9ee5-4202-a99f-e09db8794567-1"
    expect(correctionPayload("id", capture, result, face, "v3", true)?.label).toBe(face)
  })

  it("never uploads an automatic answer, but accepts explicit top-1 confirmations and search labels", () => {
    expect(correctionPayload("id", capture, result, "wrong", "v3", false)).toBeNull()
    expect(correctionPayload("id", capture, result, "wrong", "v3", true)?.label).toBe("wrong")
    const payload = correctionPayload("id", capture, result, "search-result", "v3", true)
    expect(payload).toMatchObject({
      label: "search-result",
      top1: "wrong",
      similarity: 0.73,
      click: [123, 456],
      quad: result.quad,
      up_vote: 0.93,
      bundle_version: "v3",
    })
    expect(payload?.margin).toBeCloseTo(0.03)
  })

  it("requires both clicker and camera owner consent, including older peers", () => {
    expect(
      correctionPayload("id", { ...capture, shareCorrections: false }, result, "right", "v3", true),
    ).toBeNull()
    expect(
      correctionPayload(
        "id",
        { ...capture, shareCorrections: undefined },
        result,
        "right",
        "v3",
        true,
      ),
    ).toBeNull()
    localStorage.setItem("the-gathering:share-card-corrections", "false")
    expect(correctionPayload("id", capture, result, "right", "v3", true)).toBeNull()
  })

  it("never uploads a crop captured during a private reveal", () => {
    expect(
      correctionPayload("id", { ...capture, private: true }, result, "right", "v3", true),
    ).toBeNull()
  })

  it("posts once, reports only acknowledged saves, and persists the opt-out", async () => {
    vi.mocked(api).mockResolvedValue({ data: { capture_id: "id" } })
    function Harness() {
      const upload = useCorrectionUpload()
      return (
        <>
          <CorrectionPreference upload={upload} />
          <button onClick={() => upload.save(capture, result, "right", "v3", true)}>Choose</button>
        </>
      )
    }
    render(
      <QueryClientProvider client={new QueryClient()}>
        <Harness />
      </QueryClientProvider>,
    )
    fireEvent.click(screen.getByText("Choose"))
    await screen.findByText("Correction saved for training.")
    expect(api).toHaveBeenCalledTimes(1)
    fireEvent.click(screen.getByRole("checkbox"))
    expect(localStorage.getItem("the-gathering:share-card-corrections")).toBe("false")
    fireEvent.click(screen.getByText("Choose"))
    await waitFor(() => expect(screen.getByRole("status").textContent).toBe(""))
    expect(api).toHaveBeenCalledTimes(1)
  })
})
