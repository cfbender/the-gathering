import { afterEach, expect, it, vi } from "vite-plus/test"
import { galleryPrintings } from "./gallery"
import { searchArts, type GalleryArt } from "./pipeline"

const art: GalleryArt = {
  id: "front",
  name: "Forest",
  set: "lea",
  frame: "1993",
  printing_count: 2,
}
const siblings = [
  { id: "front", name: "Forest", set: "lea", collector_number: "280", lang: "en" },
  { id: "back-1", name: "Wald", set: "fin", collector_number: "301", lang: "de", face: 1 },
]
afterEach(() => vi.unstubAllGlobals())

it("keeps siblings off the initial load, shares concurrent requests, preserves exact face metadata", async () => {
  const fetch = vi.fn().mockResolvedValue(new Response(JSON.stringify({ front: siblings })))
  vi.stubGlobal("fetch", fetch)
  const load = galleryPrintings([art], "/bundles/v2/printings.json")
  expect(fetch).not.toHaveBeenCalled()
  const first = load()
  expect(load()).toBe(first)
  const arts = await first
  expect(arts[0]?.id).toBe("front")
  expect(art.printings).toBeUndefined()
  expect(searchArts(arts, "wald set:fin #301 lang:de")).toEqual([{ ...siblings[1], frame: "1993" }])
  expect(await load()).toBe(arts)
  expect(fetch).toHaveBeenCalledOnce()
})

it("supports old embedded and representative-only bundles without another request", async () => {
  const fetch = vi.fn()
  vi.stubGlobal("fetch", fetch)
  const arts = [
    { ...art, printings: siblings },
    { ...art, id: "other" },
  ]
  expect(await galleryPrintings(arts)()).toBe(arts)
  expect(fetch).not.toHaveBeenCalled()
})

it("allows a failed optional metadata download to be retried", async () => {
  const fetch = vi
    .fn()
    .mockResolvedValueOnce(new Response("", { status: 503 }))
    .mockResolvedValueOnce(new Response(JSON.stringify({ front: siblings })))
  vi.stubGlobal("fetch", fetch)
  const load = galleryPrintings([art], "/printings.json")
  await expect(load()).rejects.toThrow("503")
  expect((await load())[0]?.printings).toEqual(siblings)
})
