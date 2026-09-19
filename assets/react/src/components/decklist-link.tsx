import { ExternalLink, Link as LinkIcon } from "lucide-react"
import { decklistSourceLabels, detectDecklistSource } from "@/lib/decklists"

export function DecklistLink({ url }: { url: string }) {
  const source = detectDecklistSource(url) ?? "other"

  return (
    <a
      className="link link-hover inline-flex items-center gap-1.5"
      href={url}
      target="_blank"
      rel="noreferrer"
    >
      <LinkIcon className="size-3.5" aria-hidden="true" />
      {decklistSourceLabels[source]}
      <ExternalLink className="size-3" aria-hidden="true" />
    </a>
  )
}
