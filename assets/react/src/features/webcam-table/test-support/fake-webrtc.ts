// Minimal RTCPeerConnection and RTCDataChannel stand-ins; jsdom has neither. Install with
// `installFakeWebRtc()` in a test's beforeEach and inspect `FakePeerConnection.instances`.
import { vi } from "vite-plus/test"

export class FakeDataChannel {
  readyState: RTCDataChannelState = "open"
  readonly sent: string[] = []
  onmessage: ((event: { data: unknown }) => void) | null = null
  onopen: (() => void) | null = null

  constructor(readonly label: string) {}

  send(data: string) {
    this.sent.push(data)
  }

  /** Delivers a message from the remote peer. */
  deliver(data: unknown) {
    this.onmessage?.({ data })
  }

  /** Parsed messages this side sent. */
  messages() {
    return this.sent.map((data) => JSON.parse(data) as { type: string } & Record<string, unknown>)
  }
}

export class FakePeerConnection {
  static instances: FakePeerConnection[] = []
  /** Lets a test script a connection's behaviour as soon as the room creates it. */
  static onCreate: ((connection: FakePeerConnection) => void) | null = null

  connectionState: RTCPeerConnectionState = "new"
  signalingState: RTCSignalingState = "stable"
  remoteDescription: RTCSessionDescriptionInit | null = null
  onicecandidate: ((event: { candidate: null }) => void) | null = null
  ontrack: ((event: { streams: MediaStream[] }) => void) | null = null
  ondatachannel: ((event: { channel: FakeDataChannel }) => void) | null = null
  onconnectionstatechange: (() => void) | null = null
  readonly channels: FakeDataChannel[] = []

  createOffer = vi.fn(async (): Promise<RTCSessionDescriptionInit> => ({ type: "offer", sdp: "o" }))
  createAnswer = vi.fn(async (): Promise<RTCSessionDescriptionInit> => ({
    type: "answer",
    sdp: "a",
  }))
  setLocalDescription = vi.fn(async (_description: RTCSessionDescriptionInit) => {})
  setRemoteDescription = vi.fn(async (description: RTCSessionDescriptionInit) => {
    this.remoteDescription = description
  })
  addIceCandidate = vi.fn(async (_candidate: RTCIceCandidateInit) => {})
  restartIce = vi.fn()

  constructor(readonly config: RTCConfiguration) {
    FakePeerConnection.instances.push(this)
    FakePeerConnection.onCreate?.(this)
  }

  addTrack() {
    return {
      replaceTrack: async () => {},
      getParameters: () => ({ encodings: [] }),
      setParameters: async () => {},
    }
  }

  createDataChannel(label: string) {
    const channel = new FakeDataChannel(label)
    this.channels.push(channel)
    return channel
  }

  /** Announces a channel the remote peer opened. */
  announceChannel(label = "table") {
    const channel = new FakeDataChannel(label)
    this.channels.push(channel)
    this.ondatachannel?.({ channel })
    return channel
  }

  changeState(state: RTCPeerConnectionState) {
    this.connectionState = state
    this.onconnectionstatechange?.()
  }

  async getStats() {
    return new Map()
  }

  close() {
    this.connectionState = "closed"
    this.signalingState = "closed"
  }
}

export function installFakeWebRtc() {
  FakePeerConnection.instances = []
  FakePeerConnection.onCreate = null
  vi.stubGlobal("RTCPeerConnection", FakePeerConnection)
}

interface FakeTrack {
  enabled: boolean
  stop: () => void
  getSettings: () => { height: number }
  clone: () => FakeTrack
}

function fakeTrack(): FakeTrack {
  return {
    enabled: true,
    stop: vi.fn(),
    getSettings: () => ({ height: 1080 }),
    clone: fakeTrack,
  }
}

/** A camera-like stream whose video track can be cloned for each peer. */
export function fakeMedia() {
  const track = fakeTrack()
  return { track, media: { getTracks: () => [track], getVideoTracks: () => [track] } }
}

/** Stubs the browser media APIs the room touches: the placeholder canvas stream, the hidden
 * capture video's play(), and canvas drawing. Returns the placeholder's media. */
export function installFakeMedia() {
  const placeholder = fakeMedia()
  vi.spyOn(HTMLMediaElement.prototype, "play").mockResolvedValue()
  vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(null)
  vi.spyOn(HTMLCanvasElement.prototype, "toDataURL").mockReturnValue(
    "data:image/jpeg;base64,/9j/4AAQ",
  )
  Object.defineProperty(HTMLCanvasElement.prototype, "captureStream", {
    configurable: true,
    value: () => placeholder.media,
  })
  return placeholder
}

/** Answers the table config request; every other request fails like an offline server. */
export function serveTableConfig(socketToken = "fresh-token") {
  return vi.spyOn(globalThis, "fetch").mockImplementation(async (input) =>
    (input instanceof Request ? input.url : input.toString()).includes("/api/webcam-table/config")
      ? new Response(
          JSON.stringify({
            data: { socket_token: socketToken, ice_servers: [], max_players: 10 },
          }),
        )
      : new Response("{}", { status: 503 }),
  )
}
