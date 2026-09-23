import type { BundleConstants, Candidate, GalleryArt, Quad } from "./pipeline"

/** `GET /api/cardid/bundle`: the published bundle and where to fetch its files. */
export interface BundleInfo {
  version: string
  created: string
  gallery: { arts: number; dtype: string; embed_dim: number; frame_penalty: number; topk: number }
  constants: BundleConstants
  files: Record<
    "manifest.json" | "arts.json" | "detector.onnx" | "embed.onnx" | "search.onnx",
    string
  > & { "printings.json"?: string }
}

export type WorkerRequest =
  | { type: "load"; bundle: BundleInfo }
  | {
      type: "identify"
      id: number
      /** RGBA pixels of the crop, transferred (not copied) to the worker. */
      rgba: ArrayBuffer
      width: number
      height: number
      /** The click, in crop pixels. */
      x: number
      y: number
    }
  | { type: "search"; id: number; query: string }
  | { type: "printings"; id: number; artId: string }

export interface Identification {
  quad: Quad
  /** Share of detector views that agreed the card is upright, 0–1. */
  upVote: number
  candidates: Candidate[]
  /** Milliseconds per stage, for the Connection panel and telemetry. */
  timings: { detector: number; embed: number; search: number; total: number }
}

export type WorkerResponse =
  | { type: "ready"; version: string; arts: number; ms: number }
  | { type: "load_failed"; message: string }
  | { type: "identified"; id: number; result: Identification }
  | { type: "identify_failed"; id: number; message: string }
  | { type: "matches"; id: number; arts: GalleryArt[] }
