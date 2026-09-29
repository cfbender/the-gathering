/**
 * Webcam table documents (`/table/*`) are served cross-origin isolated (COOP + COEP, see
 * `TheGatheringWeb.CrossOriginIsolation`) so the card recognizer can run on several WASM
 * threads. Nothing else is, because COEP blocks third-party images such as Discord avatars.
 * The headers belong to the document, so crossing that boundary needs a full page load: links
 * across it use `reloadDocument`, and the root route turns any other navigation across it into
 * one (`crossesIsolation`).
 */
export function isolatedPath(pathname: string): boolean {
  return pathname.startsWith("/table/")
}

// A single-page app keeps its document, so the path it booted on says whether it is isolated.
const documentIsolated = isolatedPath(window.location.pathname)

/** True when rendering `pathname` in this document would get its isolation wrong. */
export function crossesIsolation(pathname: string): boolean {
  return isolatedPath(pathname) !== documentIsolated
}
