import { CheckCircle2, Copy } from "lucide-react"
import { useState } from "react"
import { MYTHIC_TRACK_EXPORT_SNIPPET } from "@/features/imports/imports"

export function MythicTrackInstructions() {
  const [copied, setCopied] = useState(false)

  async function copySnippet() {
    await navigator.clipboard.writeText(MYTHIC_TRACK_EXPORT_SNIPPET)
    setCopied(true)
    window.setTimeout(() => setCopied(false), 2000)
  }

  return (
    <section className="card border-base-300 bg-base-200 mb-6 border">
      <div className="card-body gap-4 p-4 sm:p-6">
        <div>
          <h2 className="text-lg font-bold">Export your games from Mythic Track</h2>
          <p className="text-base-content/70 text-sm">
            Mythic Track has no export button, but its site loads your full game list from one
            request. This snippet saves that response as a file you can upload here.
          </p>
        </div>
        <ol className="list-inside list-decimal space-y-1 text-sm">
          <li>
            Sign in at{" "}
            <a
              className="link link-primary"
              href="https://www.mythictrack.com/"
              target="_blank"
              rel="noreferrer"
            >
              mythictrack.com
            </a>{" "}
            in a desktop browser.
          </li>
          <li>
            Open the developer console (<kbd className="kbd kbd-sm">F12</kbd> or{" "}
            <kbd className="kbd kbd-sm">⌥⌘J</kbd>), paste the snippet, and press Enter.
          </li>
          <li>
            Upload the downloaded <code className="font-mono">mythic-track-games.json</code> below.
            Every member of your playgroup can do the same; overlapping games are imported once.
          </li>
        </ol>
        <div className="flex flex-col gap-2">
          <div className="flex items-center justify-between gap-2">
            <span className="text-base-content/60 text-xs font-semibold uppercase tracking-wide">
              Console snippet
            </span>
            <button type="button" className="btn btn-sm btn-outline" onClick={copySnippet}>
              {copied ? <CheckCircle2 className="size-4" /> : <Copy className="size-4" />}
              {copied ? "Copied" : "Copy snippet"}
            </button>
          </div>
          <pre className="bg-base-100 border-base-300 max-h-56 overflow-auto rounded-lg border p-3 font-mono text-xs leading-relaxed">
            {MYTHIC_TRACK_EXPORT_SNIPPET}
          </pre>
        </div>
        <p className="text-base-content/60 text-xs">
          Only completed games are imported. Commanders keep their Scryfall IDs and colour identity;
          players with a linked Discord account merge with the same Discord user here.
        </p>
      </div>
    </section>
  )
}
