import Markdown, { type Components } from "react-markdown"
import remarkGfm from "remark-gfm"
import { CardHover } from "@/components/card-hover"
import type { RulingTurn } from "./rulings"

const CARD_LINK = "#card="

function cardLink(name: string) {
  const target = encodeURIComponent(name).replace(/\(/g, "%28").replace(/\)/g, "%29")
  return `[${name}](${CARD_LINK}${target})`
}

function escapeRegExp(text: string) {
  return text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")
}

/** `[[Card Name]]` and the written-out names of the answer's catalog cards become links the
 * renderer turns into card hovers. */
export function linkCardReferences(markdown: string, cards: { name: string }[] = []) {
  const names = cards.map((card) => card.name).sort((a, b) => b.length - a.length)
  const plain = names.map((name) => `(?<![\\w[])${escapeRegExp(name)}(?![\\w\\]])`)
  const pattern = new RegExp([String.raw`\[\[([^[\]\n]{1,160})\]\]`, ...plain].join("|"), "g")
  return markdown.replace(pattern, (match, bracketed?: string) =>
    cardLink((bracketed ?? match).trim()),
  )
}

const MARKDOWN_CLASSES = [
  "text-xs leading-relaxed break-words",
  "[&_p]:my-1.5 [&_strong]:font-semibold [&_em]:italic",
  "[&_ul]:my-1.5 [&_ul]:list-disc [&_ul]:pl-4 [&_ol]:my-1.5 [&_ol]:list-decimal [&_ol]:pl-4 [&_li]:my-0.5",
  "[&_h1]:mt-2 [&_h1]:font-bold [&_h2]:mt-2 [&_h2]:font-bold [&_h3]:mt-2 [&_h3]:font-semibold",
  "[&_code]:rounded [&_code]:bg-white/10 [&_code]:px-1",
  "[&_table]:my-2 [&_table]:w-full [&_td]:border [&_td]:border-white/10 [&_td]:px-1.5 [&_th]:border [&_th]:border-white/10 [&_th]:px-1.5",
].join(" ")

/** A ruling's Markdown with hoverable card names, then the rules it cites. */
export function RulingAnswer({ turn }: { turn: RulingTurn }) {
  const components: Components = {
    a({ href, children }) {
      if (href?.startsWith(CARD_LINK)) {
        const name = decodeURIComponent(href.slice(CARD_LINK.length))
        const card = turn.cards.find((card) => card.name.toLowerCase() === name.toLowerCase())
        return (
          <CardHover id={card?.id ?? null} name={name}>
            <span className="text-primary font-semibold underline decoration-dotted underline-offset-2">
              {children}
            </span>
          </CardHover>
        )
      }
      return (
        <a href={href} target="_blank" rel="noreferrer" className="link">
          {children}
        </a>
      )
    },
  }
  return (
    <div>
      <div className={MARKDOWN_CLASSES}>
        <Markdown remarkPlugins={[remarkGfm]} skipHtml components={components}>
          {linkCardReferences(turn.answer, turn.cards)}
        </Markdown>
      </div>
      {turn.rules.length > 0 && (
        <details className="mt-1.5 rounded-field border border-white/10 bg-black/20 text-xs">
          <summary className="cursor-pointer px-2 py-1 font-semibold text-base-content/70">
            Rules cited ({turn.rules.length})
          </summary>
          <dl className="grid gap-2 px-2 pb-2">
            {turn.rules.map((rule) => (
              <div key={rule.number}>
                <dt className="font-semibold">{rule.number}</dt>
                <dd className="text-base-content/75 whitespace-pre-line">{rule.text}</dd>
              </div>
            ))}
          </dl>
        </details>
      )}
    </div>
  )
}
