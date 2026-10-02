// Minimal RTCPeerConnection stand-ins; jsdom has none. Install with `installFakeWebRtc()`
// in a test's beforeEach and inspect `FakePeerConnection.instances`.
import { vi } from "vite-plus/test"

/** A video sender that keeps its track and encodings, like a negotiated RTCRtpSender. */
export class FakeSender {
  private encodings: RTCRtpEncodingParameters[]

  constructor(
    public track: unknown,
    encodings: RTCRtpEncodingParameters[] = [{}],
  ) {
    this.encodings = encodings
  }

  replaceTrack = vi.fn(async (track: unknown) => {
    this.track = track
  })
  getParameters = vi.fn(() => ({ encodings: this.encodings.map((encoding) => ({ ...encoding })) }))
  setParameters = vi.fn(async (parameters: { encodings: RTCRtpEncodingParameters[] }) => {
    this.encodings = parameters.encodings
  })
}

export class FakeReceiver {
  track: unknown = null
  getStats = vi.fn(async () => new Map())
}

/** A transceiver as the room sees it: its sender, receiver, negotiated mid, and the codec
 * order the room asked for. */
export class FakeTransceiver {
  codecPreferences: RTCRtpCodec[] | null = null
  mid: string | null = null
  readonly receiver = new FakeReceiver()

  constructor(
    readonly sender: FakeSender,
    readonly direction: RTCRtpTransceiverDirection,
  ) {}

  setCodecPreferences = vi.fn((codecs: RTCRtpCodec[]) => {
    this.codecPreferences = codecs
  })
}

/** What Chrome reports for a camera track: VP8 first, H.264 in the middle, repair codecs last. */
export const VIDEO_CAPABILITIES: RTCRtpCodec[] = [
  { mimeType: "video/VP8", clockRate: 90000 },
  { mimeType: "video/rtx", clockRate: 90000, sdpFmtpLine: "apt=96" },
  { mimeType: "video/H264", clockRate: 90000, sdpFmtpLine: "profile-level-id=42001f" },
  { mimeType: "video/VP9", clockRate: 90000, sdpFmtpLine: "profile-id=0" },
  { mimeType: "video/H264", clockRate: 90000, sdpFmtpLine: "profile-level-id=42e01f" },
  { mimeType: "video/red", clockRate: 90000 },
  { mimeType: "video/ulpfec", clockRate: 90000 },
]

export class FakePeerConnection {
  static instances: FakePeerConnection[] = []
  /** Lets a test script a connection's behaviour as soon as the room creates it. */
  static onCreate: ((connection: FakePeerConnection) => void) | null = null

  connectionState: RTCPeerConnectionState = "new"
  signalingState: RTCSignalingState = "stable"
  remoteDescription: RTCSessionDescriptionInit | null = null
  onicecandidate: ((event: { candidate: null }) => void) | null = null
  ontrack:
    | ((event: { transceiver: FakeTransceiver; streams: MediaStream[]; track: unknown }) => void)
    | null = null
  onconnectionstatechange: (() => void) | null = null

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

  constructor(readonly config: RTCConfiguration) {
    FakePeerConnection.instances.push(this)
    FakePeerConnection.onCreate?.(this)
  }

  readonly transceivers: FakeTransceiver[] = []
  addTransceiver = vi.fn((trackOrKind: unknown, init?: RTCRtpTransceiverInit) => {
    const track = typeof trackOrKind === "string" ? null : trackOrKind
    const sender = new FakeSender(
      track,
      init?.sendEncodings?.map((encoding) => ({ ...encoding })) ?? [{}],
    )
    const transceiver = new FakeTransceiver(sender, init?.direction ?? "sendrecv")
    transceiver.mid = String(this.transceivers.length)
    this.transceivers.push(transceiver)
    return transceiver
  })

  /** This seat's camera sender, if it publishes. */
  get sender() {
    return this.transceivers.find((transceiver) => transceiver.direction === "sendonly")?.sender
  }

  getTransceivers() {
    return this.transceivers
  }

  /** Delivers a board the server added under `mid`, as the server's offer named it. */
  arrive(mid: string, stream: MediaStream) {
    let transceiver = this.transceivers.find((entry) => entry.mid === mid)
    if (!transceiver) {
      transceiver = new FakeTransceiver(new FakeSender(null), "recvonly")
      transceiver.mid = mid
      this.transceivers.push(transceiver)
    }
    transceiver.receiver.track = { kind: "video" }
    this.ontrack?.({ transceiver, streams: [stream], track: transceiver.receiver.track })
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

/** Stands in for ResizeObserver: tests fire sizes for the elements the room observes. */
export class FakeResizeObserver {
  static instances: FakeResizeObserver[] = []
  readonly observed = new Set<Element>()

  constructor(
    readonly callback: (entries: { target: Element; contentRect: DOMRectReadOnly }[]) => void,
  ) {
    FakeResizeObserver.instances.push(this)
  }

  observe = vi.fn((element: Element) => {
    this.observed.add(element)
  })
  unobserve = vi.fn((element: Element) => {
    this.observed.delete(element)
  })
  disconnect = vi.fn()

  /** Reports `element` drawn at `width` × `height` CSS pixels. */
  resize(element: Element, width: number, height: number) {
    this.callback([{ target: element, contentRect: { width, height } as DOMRectReadOnly }])
  }
}

export function installFakeWebRtc() {
  FakePeerConnection.instances = []
  FakePeerConnection.onCreate = null
  FakeResizeObserver.instances = []
  vi.stubGlobal("RTCPeerConnection", FakePeerConnection)
  vi.stubGlobal("ResizeObserver", FakeResizeObserver)
  vi.stubGlobal("RTCRtpReceiver", {
    getCapabilities: (kind: string) => (kind === "video" ? { codecs: VIDEO_CAPABILITIES } : null),
  })
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
            data: {
              socket_token: socketToken,
              ice_servers: [],
              max_players: 10,
              sfu: { transport: "direct" },
            },
          }),
        )
      : new Response("{}", { status: 503 }),
  )
}
