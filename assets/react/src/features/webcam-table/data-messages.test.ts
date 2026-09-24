import { describe, expect, it } from "vite-plus/test"
import { MAX_DATA_MESSAGE_LENGTH, parseDataMessage } from "./data-messages"

const response = {
  type: "capture_response",
  requestId: "request",
  image: "data:image/jpeg;base64,/9j/4AAQ",
  nativeWidth: 1920,
  nativeHeight: 1080,
  cropSize: 640,
  clickX: 320,
  clickY: 100,
  private: false,
  shareCorrections: true,
}

const send = (message: unknown) => parseDataMessage(JSON.stringify(message))

describe("data channel messages", () => {
  it("accepts capture requests with a click inside the frame", () => {
    expect(send({ type: "capture_request", requestId: "r", x: 0.25, y: 1 })).toEqual({
      type: "capture_request",
      requestId: "r",
      x: 0.25,
      y: 1,
    })
    for (const bad of [
      { x: -0.1, y: 0.5 },
      { x: 0.5, y: 1.5 },
      { x: "0.5", y: 0.5 },
      { x: null, y: 0.5 },
    ])
      expect(send({ type: "capture_request", requestId: "r", ...bad })).toBeNull()
    expect(send({ type: "capture_request", requestId: 7, x: 0.5, y: 0.5 })).toBeNull()
    expect(send({ type: "capture_request", requestId: "", x: 0.5, y: 0.5 })).toBeNull()
  })

  it("accepts only JPEG crops under the size cap and drops unknown fields", () => {
    expect(send({ ...response, extra: "ignored" })).toEqual(response)
    expect(send({ ...response, image: "data:image/png;base64,iVBOR" })).toBeNull()
    expect(send({ ...response, image: "javascript:alert(1)" })).toBeNull()
    expect(send({ ...response, image: "data:image/jpeg;base64,<svg>" })).toBeNull()
    expect(
      send({ ...response, image: `data:image/jpeg;base64,${"A".repeat(MAX_DATA_MESSAGE_LENGTH)}` }),
    ).toBeNull()
    expect(send({ ...response, cropSize: 2000 })).toBeNull()
    expect(send({ ...response, clickX: 641 })).toBeNull()
    expect(send({ ...response, private: "no" })).toBeNull()
    expect(send({ ...response, shareCorrections: undefined })).toBeNull()
  })

  it("ignores malformed JSON, non-strings, and other message types", () => {
    expect(parseDataMessage("{not json")).toBeNull()
    expect(parseDataMessage(new ArrayBuffer(4))).toBeNull()
    expect(parseDataMessage("null")).toBeNull()
    expect(parseDataMessage("[1,2]")).toBeNull()
    expect(send({ type: "card_identified", entry: {} })).toBeNull()
    expect(send({ type: "cards_cleared", ownerPeerId: "someone" })).toBeNull()
  })
})
