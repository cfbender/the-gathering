import docs from "../../../../../docs/deck-chooser-scripts.md?raw"

export interface ReferenceField {
  name: string
  type: string
  description: string
}

export interface ReferenceSection {
  /** `deck`, `game`, `opponent`, `time`, or `ctx`. */
  name: string
  intro: string
  fields: ReferenceField[]
}

/** Reads the field reference tables out of the deck chooser script guide, so the editor and
 * `docs/deck-chooser-scripts.md` (which the server's tests check against the tables scripts
 * receive) never disagree. Inline Markdown stays in the strings; see `inlineParts`. */
export function parseReference(markdown: string): ReferenceSection[] {
  const start = markdown.indexOf("## Field reference")
  if (start < 0) return []
  const rest = markdown.slice(start + 1)
  const end = rest.search(/^## /m)
  const lines = (end < 0 ? rest : rest.slice(0, end)).split("\n")
  const sections: ReferenceSection[] = []
  let section: ReferenceSection | undefined
  for (const line of lines) {
    const heading = /^### `(\w+)`/.exec(line)
    if (heading?.[1]) {
      section = { name: heading[1], intro: "", fields: [] }
      sections.push(section)
      continue
    }
    if (!section) continue
    const row = /^\| (`[^`]+`) \| (.+?) \| (.+) \|$/.exec(line)
    if (row?.[1] && row[2] && row[3]) {
      section.fields.push({ name: row[1].slice(1, -1), type: row[2], description: row[3] })
    } else if (line.trim() && !line.startsWith("|") && section.fields.length === 0) {
      section.intro = section.intro ? `${section.intro} ${line.trim()}` : line.trim()
    }
  }
  return sections
}

/** Splits inline Markdown into text and code spans, keeping link text and dropping URLs. */
export function inlineParts(text: string): Array<{ code: boolean; text: string }> {
  return text
    .replace(/\[([^\]]+)\]\([^)]*\)/g, "$1")
    .split("`")
    .map((part, index) => ({ code: index % 2 === 1, text: part }))
    .filter((part) => part.text !== "")
}

export const DECK_SCRIPT_REFERENCE = parseReference(docs)
