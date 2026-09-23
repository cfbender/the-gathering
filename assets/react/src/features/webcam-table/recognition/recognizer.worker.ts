/// <reference lib="webworker" />
/**
 * Runs the published card-recognition bundle with onnxruntime-web off the main thread.
 *
 * Mirrors `ml/cardid/bundle.py` step for step (see `pipeline.ts` for the maths): two detector
 * passes, one embed pass, one gallery search. Uses the WASM backend so results match the
 * Python parity check bit-for-bit modulo float rounding; WebGPU is a later switch
 * (`onnxruntime-web/webgpu` + the jsep wasm pair) once its per-op coverage is measured.
 */
import * as ort from "onnxruntime-web/wasm"
import mjsUrl from "onnxruntime-web/ort-wasm-simd-threaded.mjs?url"
import wasmUrl from "onnxruntime-web/ort-wasm-simd-threaded.wasm?url"
import { galleryPrintings } from "./gallery"
import type { BundleInfo, Identification, WorkerRequest, WorkerResponse } from "./messages"
import {
  fromWindow,
  refineSide,
  resampleWindow,
  searchArts,
  upVote,
  type BundleConstants,
  type Candidate,
  type GalleryArt,
  type Point,
  type Quad,
  type RgbaImage,
} from "./pipeline"

// Same-origin copies of the runtime (Vite emits them as assets); the page is not
// cross-origin isolated, so a single wasm thread.
ort.env.wasm.wasmPaths = { wasm: wasmUrl, mjs: mjsUrl }
ort.env.wasm.numThreads = 1
ort.env.logLevel = "warning"

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

async function session(url: string): Promise<ort.InferenceSession> {
  return ort.InferenceSession.create(await fetchBytes(url), {
    executionProviders: ["wasm"],
    graphOptimizationLevel: "all",
  })
}

async function load(bundle: BundleInfo) {
  const started = performance.now()
  const [detector, embed, search, artsBytes] = await Promise.all([
    session(bundle.files["detector.onnx"]),
    session(bundle.files["embed.onnx"]),
    session(bundle.files["search.onnx"]),
    fetchBytes(bundle.files["arts.json"]),
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

async function identify(image: RgbaImage, x: number, y: number): Promise<Identification> {
  if (!loaded) throw new Error("bundle not loaded")
  const { constants, embed, search, arts } = loaded
  const started = performance.now()
  const coarse = await detect(image, x, y, constants.scene)
  const fine = await detect(
    image,
    coarse.centre[0],
    coarse.centre[1],
    refineSide(coarse.short, constants),
  )
  const detected = performance.now()

  const embeddings = await embed.run({
    scene: new ort.Tensor("uint8", new Uint8Array(image.data.buffer), [
      image.height,
      image.width,
      4,
    ]),
    quad: new ort.Tensor("float32", Float32Array.from(fine.quad.flat()), [4, 2]),
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
  return {
    quad: fine.quad,
    upVote: upVote(fine.up, constants),
    candidates,
    timings: {
      detector: detected - started,
      embed: embedded - detected,
      search: finished - embedded,
      total: finished - started,
    },
  }
}

self.onmessage = async (event: MessageEvent<WorkerRequest>) => {
  const request = event.data
  try {
    if (request.type === "load") {
      await load(request.bundle)
    } else if (request.type === "identify") {
      const image: RgbaImage = {
        data: new Uint8ClampedArray(request.rgba),
        width: request.width,
        height: request.height,
      }
      reply({
        type: "identified",
        id: request.id,
        result: await identify(image, request.x, request.y),
      })
    } else if (request.type === "search") {
      reply({
        type: "matches",
        id: request.id,
        arts: loaded ? searchArts(await loaded.printings(), request.query) : [],
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
