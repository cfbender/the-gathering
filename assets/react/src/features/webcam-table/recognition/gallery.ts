import type { GalleryArt, GalleryPrinting } from "./pipeline"

/** The old format embeds siblings; new bundles fetch them only for explicit browsing. */
export function galleryPrintings(arts: GalleryArt[], url?: string) {
  let loading: Promise<GalleryArt[]> | undefined
  return () => {
    if (!url) return Promise.resolve(arts)
    loading ??= fetch(url, { credentials: "same-origin" })
      .then(async (response) => {
        if (!response.ok) throw new Error(`Printings: HTTP ${response.status}`)
        const siblings = (await response.json()) as Record<string, GalleryPrinting[]>
        return arts.map((art) => ({ ...art, printings: siblings[art.id] ?? art.printings }))
      })
      .catch((error: unknown) => {
        loading = undefined
        throw error
      })
    return loading
  }
}
