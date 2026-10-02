import { afterEach, expect, it, vi } from "vite-plus/test"
import type { BundleInfo, WorkerRequest, WorkerResponse } from "./messages"

const runtime = vi.hoisted(() => ({
  create: vi.fn(),
  env: {
    wasm: {} as {
      initTimeout?: number
      wasmBinary?: Uint8Array
      wasmPaths?: { wasm: string; mjs: string }
    },
  },
}))
vi.mock("onnxruntime-web/wasm", () => ({
  env: runtime.env,
  Tensor: class {
    constructor(
      public type: string,
      public data: unknown,
      public dims: number[],
    ) {}
  },
  InferenceSession: { create: runtime.create },
}))

afterEach(() => {
  vi.unstubAllGlobals()
  vi.resetModules()
  vi.clearAllMocks()
})

it.each([
  ["modern", "old", "extended", "tall", "right", "left"],
  [
    "modern",
    "old",
    "extended",
    "tall",
    "right",
    "left",
    "room_0",
    "room_1",
    "split_0",
    "split_1",
    "aftermath_0",
    "aftermath_1",
    "flip_0",
    "flip_1",
  ],
  ["flip_1", "room_0", "modern"],
])("passes through the graph tensor with manifest frame order %j", async (...frameNames) => {
  const vectors = {
    data: new Float32Array(frameNames.length * 128),
    dims: [frameNames.length, 128],
  }
  const search = vi.fn().mockResolvedValue({
    indices: { data: BigInt64Array.from([1n, 0n]) },
    scores: { data: Float32Array.from([0.9, 0.7]) },
  })
  const detector = vi.fn().mockResolvedValue({
    quad: { data: Float32Array.from([40, 20, 200, 20, 200, 240, 40, 240]) },
    up: { data: Float32Array.from([0, -4]) },
    centre: { data: Float32Array.from([120, 130]) },
    short: { data: Float32Array.from([160]) },
  })
  runtime.create.mockImplementation(async (bytes: Uint8Array) => ({
    run:
      bytes[0] === 1
        ? detector
        : bytes[0] === 2
          ? vi.fn().mockResolvedValue({ embeddings: vectors })
          : search,
  }))
  const front = {
    id: "uuid",
    name: "Mirror Room",
    set: "dsk",
    face: 0,
    layout: "split",
    frame: "room_0",
  }
  const back = { ...front, id: "uuid-1", name: "Fractured Realm", face: 1, frame: "room_1" }
  vi.stubGlobal(
    "fetch",
    vi.fn(
      async (url: string) =>
        new Response(
          url.endsWith("arts.json")
            ? JSON.stringify([front, back])
            : Uint8Array.from([url.includes("detector") ? 1 : url.includes("embed") ? 2 : 3]),
        ),
    ),
  )
  const messages: WorkerResponse[] = []
  const worker: {
    postMessage: (message: WorkerResponse) => void
    onmessage?: (event: { data: WorkerRequest }) => Promise<void>
  } = {
    postMessage: (message) => messages.push(message),
  }
  vi.stubGlobal("self", worker)
  await import("./recognizer.worker")
  const bundle: BundleInfo = {
    version: "test",
    created: "test",
    constants: {
      scene: 640,
      det_input: 256,
      rotations: 4,
      refine_fill: 0.6,
      refine_min_side: 64,
      card_aspect: 1.4,
      frame_names: frameNames,
    },
    gallery: { arts: 2, dtype: "f32", embed_dim: 128, frame_penalty: 0.02, topk: 2 },
    files: {
      "manifest.json": "manifest.json",
      "arts.json": "arts.json",
      "detector.onnx": "detector.onnx",
      "embed.onnx": "embed.onnx",
      "search.onnx": "search.onnx",
    },
  }
  await worker.onmessage!({ data: { type: "load", bundle, threads: 1 } })
  expect(messages[0]?.type).toBe("ready")
  // The runtime binary is downloaded up front and the start-up itself is on a deadline, so a
  // blocked pthread worker script fails the load instead of hanging it.
  const wasmFetch = vi
    .mocked(fetch)
    .mock.calls.find(([url]) => typeof url === "string" && url.endsWith(".wasm"))
  expect(wasmFetch).toBeDefined()
  expect(runtime.env.wasm.wasmBinary).toBeInstanceOf(Uint8Array)
  expect(runtime.env.wasm.initTimeout).toBeGreaterThan(0)
  // The pthread worker script is keyed past copies cached before responses carried COEP.
  expect(runtime.env.wasm.wasmPaths?.mjs).toMatch(/ort-wasm-simd-threaded\.mjs\?coep=1$/)
  await worker.onmessage!({
    data: {
      type: "identify",
      id: 7,
      rgba: new ArrayBuffer(32 * 32 * 4),
      width: 32,
      height: 32,
      x: 16,
      y: 16,
    },
  })
  expect(search).toHaveBeenCalledTimes(2)
  expect(search.mock.calls[1]?.[0].embeddings).toBe(vectors)
  expect(messages[1]).toMatchObject({
    type: "identified",
    id: 7,
    result: { candidates: [back, front] },
  })
})

it("identifies inside a drawn outline without the detector, keeping the better upright reading", async () => {
  const detector = vi.fn().mockResolvedValue({
    quad: { data: Float32Array.from([40, 20, 200, 20, 200, 240, 40, 240]) },
    up: { data: Float32Array.from([0, -4]) },
    centre: { data: Float32Array.from([120, 130]) },
    short: { data: Float32Array.from([160]) },
  })
  const embed = vi
    .fn()
    .mockResolvedValue({ embeddings: { data: new Float32Array(128), dims: [1, 128] } })
  // The load warm-up searches once; then the upright reading scores 0.5, the turned one 0.8.
  const search = vi
    .fn()
    .mockResolvedValueOnce({
      indices: { data: BigInt64Array.from([0n]) },
      scores: { data: Float32Array.from([0.1]) },
    })
    .mockResolvedValueOnce({
      indices: { data: BigInt64Array.from([0n]) },
      scores: { data: Float32Array.from([0.5]) },
    })
    .mockResolvedValueOnce({
      indices: { data: BigInt64Array.from([1n]) },
      scores: { data: Float32Array.from([0.8]) },
    })
  runtime.create.mockImplementation(async (bytes: Uint8Array) => ({
    run: bytes[0] === 1 ? detector : bytes[0] === 2 ? embed : search,
  }))
  const upright = { id: "upright", name: "A", set: "abc", frame: "modern" }
  const turned = { id: "turned", name: "B", set: "abc", frame: "modern" }
  vi.stubGlobal(
    "fetch",
    vi.fn(
      async (url: string) =>
        new Response(
          url.endsWith("arts.json")
            ? JSON.stringify([upright, turned])
            : Uint8Array.from([url.includes("detector") ? 1 : url.includes("embed") ? 2 : 3]),
        ),
    ),
  )
  const messages: WorkerResponse[] = []
  const worker: {
    postMessage: (message: WorkerResponse) => void
    onmessage?: (event: { data: WorkerRequest }) => Promise<void>
  } = { postMessage: (message) => messages.push(message) }
  vi.stubGlobal("self", worker)
  await import("./recognizer.worker")
  await worker.onmessage!({
    data: {
      type: "load",
      bundle: {
        version: "test",
        created: "test",
        constants: {
          scene: 640,
          det_input: 256,
          rotations: 4,
          refine_fill: 0.6,
          refine_min_side: 64,
          card_aspect: 1.4,
          frame_names: ["modern"],
        },
        gallery: { arts: 2, dtype: "f32", embed_dim: 128, frame_penalty: 0.02, topk: 1 },
        files: {
          "manifest.json": "manifest.json",
          "arts.json": "arts.json",
          "detector.onnx": "detector.onnx",
          "embed.onnx": "embed.onnx",
          "search.onnx": "search.onnx",
        },
      },
      threads: 1,
    },
  })
  detector.mockClear()
  embed.mockClear()
  // Corners clicked in a scrambled order around a 250 x 350 card.
  await worker.onmessage!({
    data: {
      type: "identify",
      id: 3,
      rgba: new ArrayBuffer(32 * 32 * 4),
      width: 32,
      height: 32,
      x: 16,
      y: 16,
      quad: [
        [290, 370],
        [40, 20],
        [40, 370],
        [290, 20],
      ],
    },
  })
  expect(detector).not.toHaveBeenCalled()
  const quads = embed.mock.calls.map(([feeds]) => Array.from(feeds.quad.data as Float32Array))
  expect(quads).toEqual([
    [40, 20, 290, 20, 290, 370, 40, 370],
    [290, 370, 40, 370, 40, 20, 290, 20],
  ])
  expect(messages[1]).toMatchObject({
    type: "identified",
    id: 3,
    result: {
      quad: [
        [290, 370],
        [40, 370],
        [40, 20],
        [290, 20],
      ],
      upVote: null,
      candidates: [{ id: "turned", score: expect.closeTo(0.8, 5) }],
    },
  })
})
