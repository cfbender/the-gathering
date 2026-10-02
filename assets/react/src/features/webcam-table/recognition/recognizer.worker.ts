/// <reference lib="webworker" />
/**
 * Runs the published card-recognition bundle with onnxruntime-web off the main thread.
 *
 * Mirrors Oracle's `cardid/bundle.py` step for step (see `pipeline.ts` for the maths): two detector
 * passes, one embed pass, one gallery search. Uses the WASM backend so results match the
 * Python parity check bit-for-bit modulo float rounding. WebGPU was measured and is not used:
 * the default (JSEP) build fails on the detector, and the native EP (`onnxruntime-web/webgpu`)
 * lacks Round/Mod/Or and was far slower than WASM on Firefox/Linux. See docs/webcam-table.md,
 * "Browser inference in the clicking browser".
 */
import * as ort from "onnxruntime-web/wasm"
import mjsUrl from "onnxruntime-web/ort-wasm-simd-threaded.mjs?url"
import wasmUrl from "onnxruntime-web/ort-wasm-simd-threaded.wasm?url"
import { galleryPrintings } from "./gallery"
import type { BundleInfo, Identification, WorkerRequest, WorkerResponse } from "./messages"
import {
  fromWindow,
  portraitQuad,
  refineSide,
  resampleWindow,
  searchArts,
  turnedHalf,
  upVote,
  type BundleConstants,
  type Candidate,
  type GalleryArt,
  type Point,
  type Quad,
  type RgbaImage,
} from "./pipeline"

// Same-origin copies of the runtime (Vite emits them as assets). The standalone `.mjs` gives
// onnxruntime a real script URL to start its pthread workers from. The `.wasm` is fetched by
// `load` below and handed over as `wasmBinary`, so the download is not on the init clock.
ort.env.wasm.wasmPaths = { wasm: wasmUrl, mjs: mjsUrl }
ort.env.logLevel = "warning"

// Budget for compiling the runtime and starting its pthread workers once the binary is in
// hand. A worker script the browser blocks (an extension or policy on the `.mjs` URL) never
// reports back, and Firefox does not fire `error` for it either, so the threaded start would
// otherwise hang forever. Failing here lets useRecognizer retry on one thread, or show
// "failed" instead of "Loading…".
const INIT_TIMEOUT_MS = 20_000

interface Loaded {
  version: string
  constants: BundleConstants
  arts: GalleryArt[]
  printings: () => Promise<GalleryArt[]>
  detector: ort.InferenceSession
  embed: ort.InferenceSession
  search: ort.InferenceSession
}

let loaded: Loaded | null = null

function reply(message: WorkerResponse) {
  self.postMessage(message)
}

async function fetchBytes(url: string): Promise<Uint8Array> {
  const response = await fetch(url, { credentials: "same-origin" })
  if (!response.ok) throw new Error(`${url}: HTTP ${response.status}`)
  return new Uint8Array(await response.arrayBuffer())
}

function session(model: Uint8Array): Promise<ort.InferenceSession> {
  return ort.InferenceSession.create(model, {
    executionProviders: ["wasm"],
    graphOptimizationLevel: "all",
  })
}

async function load(bundle: BundleInfo, threads: number) {
  const started = performance.now()
  // WASM threads need SharedArrayBuffer, which only a cross-origin-isolated page (the webcam
  // table, see TheGatheringWeb.CrossOriginIsolation) provides. 0 lets onnxruntime pick
  // min(4, ceil(cores / 2)). The runtime initializes once, on the first session below.
  ort.env.wasm.numThreads = self.crossOriginIsolated ? threads : 1
  ort.env.wasm.initTimeout = INIT_TIMEOUT_MS
  const [wasmBinary, detectorBytes, embedBytes, searchBytes, artsBytes] = await Promise.all([
    fetchBytes(wasmUrl),
    fetchBytes(bundle.files["detector.onnx"]),
    fetchBytes(bundle.files["embed.onnx"]),
    fetchBytes(bundle.files["search.onnx"]),
    fetchBytes(bundle.files["arts.json"]),
  ])
  ort.env.wasm.wasmBinary = wasmBinary
  const [detector, embed, search] = await Promise.all([
    session(detectorBytes),
    session(embedBytes),
    session(searchBytes),
  ])
  const arts = JSON.parse(new TextDecoder().decode(artsBytes)) as GalleryArt[]
  loaded = {
    version: bundle.version,
    constants: bundle.constants,
    arts,
    detector,
    embed,
    search,
    printings: galleryPrintings(arts, bundle.files["printings.json"]),
  }
  // The first run of each graph pays for kernel setup; do it now, not on the first click.
  const size = bundle.constants.det_input
  const blank: RgbaImage = {
    data: new Uint8ClampedArray(size * size * 4).fill(255),
    width: size,
    height: size,
  }
  await identify(blank, size / 2, size / 2)
  reply({
    type: "ready",
    version: bundle.version,
    arts: arts.length,
    ms: performance.now() - started,
    threads: ort.env.wasm.numThreads,
  })
}

async function detect(
  image: RgbaImage,
  cx: number,
  cy: number,
  side: number,
): Promise<{ quad: Quad; up: [number, number]; centre: Point; short: number }> {
  if (!loaded) throw new Error("bundle not loaded")
  const size = loaded.constants.det_input
  const { window, scale } = resampleWindow(image, cx, cy, side, size)
  const feeds = { window: new ort.Tensor("uint8", new Uint8Array(window.buffer), [size, size, 4]) }
  const out = await loaded.detector.run(feeds)
  const quad = out.quad?.data as Float32Array
  const up = out.up?.data as Float32Array
  const centre = out.centre?.data as Float32Array
  const short = out.short?.data as Float32Array
  const corners = [0, 1, 2, 3].map((k) =>
    fromWindow([quad[k * 2] ?? 0, quad[k * 2 + 1] ?? 0], cx, cy, scale, size),
  ) as Quad
  return {
    quad: corners,
    up: [up[0] ?? 0, up[1] ?? 0],
    centre: fromWindow([centre[0] ?? 0, centre[1] ?? 0], cx, cy, scale, size),
    short: (short[0] ?? 0) / scale,
  }
}

/** Gallery candidates for the card at `quad` (image px, printed order), with stage timings. */
async function embedAndSearch(image: RgbaImage, quad: Quad) {
  if (!loaded) throw new Error("bundle not loaded")
  const { embed, search, arts } = loaded
  const started = performance.now()
  const embeddings = await embed.run({
    scene: new ort.Tensor("uint8", new Uint8Array(image.data.buffer), [
      image.height,
      image.width,
      4,
    ]),
    quad: new ort.Tensor("float32", Float32Array.from(quad.flat()), [4, 2]),
  })
  const embedded = performance.now()
  const vectors = Object.values(embeddings)[0]
  if (!vectors) throw new Error("embed graph returned nothing")
  const ranked = await search.run({ embeddings: vectors })
  const finished = performance.now()

  const indices = ranked.indices?.data as BigInt64Array | Int32Array
  const scores = ranked.scores?.data as Float32Array
  const candidates: Candidate[] = []
  for (let k = 0; k < indices.length; k += 1) {
    const index = Number(indices[k])
    const art = arts[index]
    if (art) candidates.push({ ...art, index, score: scores[k] ?? 0 })
  }
  return { candidates, embed: embedded - started, search: finished - embedded }
}

async function identify(image: RgbaImage, x: number, y: number): Promise<Identification> {
  if (!loaded) throw new Error("bundle not loaded")
  const { constants } = loaded
  const started = performance.now()
  const coarse = await detect(image, x, y, constants.scene)
  const fine = await detect(
    image,
    coarse.centre[0],
    coarse.centre[1],
    refineSide(coarse.short, constants),
  )
  const detected = performance.now()
  const { candidates, embed, search } = await embedAndSearch(image, fine.quad)
  return {
    quad: fine.quad,
    upVote: upVote(fine.up, constants),
    candidates,
    timings: {
      detector: detected - started,
      embed,
      search,
      total: performance.now() - started,
    },
  }
}

/**
 * The card inside an outline the user drew (Shift+click corners, any order): no detector.
 * Which short edge is the top is unknown, so both upright readings are searched and the one
 * with the better top match wins; its quad is returned in printed order for the correction.
 */
async function identifyOutline(image: RgbaImage, corners: Quad): Promise<Identification> {
  const started = performance.now()
  const upright = portraitQuad(corners)
  const readings = []
  for (const quad of [upright, turnedHalf(upright)]) {
    readings.push({ quad, ...(await embedAndSearch(image, quad)) })
  }
  const [first, second] = readings as [(typeof readings)[0], (typeof readings)[0]]
  const best =
    (second.candidates[0]?.score ?? -Infinity) > (first.candidates[0]?.score ?? -Infinity)
      ? second
      : first
  return {
    quad: best.quad,
    upVote: null,
    candidates: best.candidates,
    timings: {
      detector: 0,
      embed: first.embed + second.embed,
      search: first.search + second.search,
      total: performance.now() - started,
    },
  }
}

self.onmessage = async (event: MessageEvent<WorkerRequest>) => {
  const request = event.data
  try {
    if (request.type === "load") {
      await load(request.bundle, request.threads)
    } else if (request.type === "identify") {
      const image: RgbaImage = {
        data: new Uint8ClampedArray(request.rgba),
        width: request.width,
        height: request.height,
      }
      reply({
        type: "identified",
        id: request.id,
        result: request.quad
          ? await identifyOutline(image, request.quad)
          : await identify(image, request.x, request.y),
      })
    } else if (request.type === "search") {
      reply({
        type: "matches",
        id: request.id,
        arts: loaded ? searchArts(await loaded.printings(), request.query) : [],
      })
    } else if (request.type === "locate") {
      const wanted = new Set(request.printingIds)
      const arts = (await loaded?.printings()) ?? []
      reply({
        type: "matches",
        id: request.id,
        arts: arts.flatMap((art) => {
          const hits = (art.printings ?? [art]).filter((printing) => wanted.has(printing.id))
          return hits.length > 0 ? [{ ...art, printings: hits }] : []
        }),
      })
    } else if (request.type === "printings") {
      const art = (await loaded?.printings())?.find((art) => art.id === request.artId)
      reply({
        type: "matches",
        id: request.id,
        arts: art
          ? (art.printings ?? [art]).map((printing) => ({ ...printing, frame: art.frame }))
          : [],
      })
    }
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error)
    if (request.type === "load") reply({ type: "load_failed", message })
    else reply({ type: "identify_failed", id: request.id, message })
  }
}
