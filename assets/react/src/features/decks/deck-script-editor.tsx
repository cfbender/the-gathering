import { useImperativeHandle, useRef, type KeyboardEvent, type Ref } from "react"
import { cn } from "@/lib/cn"
import { LuaCode } from "./lua-highlight"

export interface DeckScriptEditorHandle {
  /** Focuses the editor with the caret at the start of a 1-based line. */
  goToLine: (line: number) => void
}

const INDENT = "  "

/** A Lua editor: a transparent textarea over syntax-colored text, with line numbers. Tab indents
 * and the error line is marked. */
export function DeckScriptEditor({
  value,
  onChange,
  errorLine,
  label,
  ref,
}: {
  value: string
  onChange: (value: string) => void
  errorLine?: number | null
  label: string
  ref?: Ref<DeckScriptEditorHandle>
}) {
  const textarea = useRef<HTMLTextAreaElement>(null)
  const gutter = useRef<HTMLDivElement>(null)
  const highlight = useRef<HTMLPreElement>(null)
  const lines = value.split("\n").length

  useImperativeHandle(ref, () => ({
    goToLine(line) {
      const element = textarea.current
      if (!element) return
      const offset = value
        .split("\n")
        .slice(0, Math.max(line - 1, 0))
        .reduce((total, text) => total + text.length + 1, 0)
      element.focus()
      element.setSelectionRange(offset, offset)
      const lineHeight = parseFloat(getComputedStyle(element).lineHeight) || 24
      element.scrollTop = Math.max((line - 4) * lineHeight, 0)
    },
  }))

  function handleKeyDown(event: KeyboardEvent<HTMLTextAreaElement>) {
    if (event.key !== "Tab" || event.altKey || event.ctrlKey || event.metaKey) return
    event.preventDefault()
    const element = event.currentTarget
    const { selectionStart: start, selectionEnd: end } = element
    if (event.shiftKey) {
      const lineStart = value.lastIndexOf("\n", start - 1) + 1
      if (!value.startsWith(INDENT, lineStart)) return
      onChange(value.slice(0, lineStart) + value.slice(lineStart + INDENT.length))
      requestAnimationFrame(() => {
        const caret = Math.max(start - INDENT.length, lineStart)
        element.setSelectionRange(caret, Math.max(end - INDENT.length, caret))
      })
      return
    }
    onChange(value.slice(0, start) + INDENT + value.slice(end))
    requestAnimationFrame(() =>
      element.setSelectionRange(start + INDENT.length, start + INDENT.length),
    )
  }

  return (
    <div className="border-base-300 bg-base-100 focus-within:border-primary focus-within:ring-primary/20 flex h-[min(70vh,44rem)] min-h-80 overflow-hidden rounded-box border font-mono text-xs leading-5 focus-within:ring-2">
      <div
        ref={gutter}
        aria-hidden="true"
        className="bg-base-200/70 text-base-content/40 border-base-300 shrink-0 overflow-hidden border-r pt-3 pb-8 text-right select-none"
      >
        {Array.from({ length: lines }, (_, index) => (
          <div
            key={index}
            className={cn(
              "px-2",
              errorLine === index + 1 && "bg-error text-error-content font-bold",
            )}
          >
            {index + 1}
          </div>
        ))}
      </div>
      <div className="relative min-w-0 flex-1">
        <pre
          ref={highlight}
          aria-hidden="true"
          className="pointer-events-none absolute inset-0 m-0 overflow-hidden pt-3 pr-8 pb-8 pl-2.5 whitespace-pre"
        >
          <LuaCode code={`${value}\n`} />
        </pre>
        <textarea
          ref={textarea}
          aria-label={label}
          value={value}
          onChange={(event) => onChange(event.target.value)}
          onKeyDown={handleKeyDown}
          onScroll={(event) => {
            const { scrollTop, scrollLeft } = event.currentTarget
            if (gutter.current) gutter.current.scrollTop = scrollTop
            if (highlight.current) {
              highlight.current.scrollTop = scrollTop
              highlight.current.scrollLeft = scrollLeft
            }
          }}
          spellCheck={false}
          autoCapitalize="off"
          autoComplete="off"
          autoCorrect="off"
          wrap="off"
          className="caret-base-content selection:bg-primary/25 absolute inset-0 size-full resize-none bg-transparent px-2.5 py-3 whitespace-pre text-transparent outline-none"
        />
      </div>
    </div>
  )
}
