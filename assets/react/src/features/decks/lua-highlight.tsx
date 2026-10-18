export type LuaTokenKind =
  | "comment"
  | "string"
  | "number"
  | "keyword"
  | "constant"
  | "builtin"
  | "global"
  | "function"
  | "field"
  | "operator"
  | "plain"

export interface LuaToken {
  kind: LuaTokenKind
  text: string
}

const KEYWORDS = new Set([
  "and",
  "break",
  "do",
  "else",
  "elseif",
  "end",
  "for",
  "function",
  "goto",
  "if",
  "in",
  "local",
  "not",
  "or",
  "repeat",
  "return",
  "then",
  "until",
  "while",
])
const CONSTANTS = new Set(["true", "false", "nil"])
/** The globals a deck chooser script can reach. */
const BUILTINS = new Set([
  "assert",
  "error",
  "getmetatable",
  "ipairs",
  "math",
  "next",
  "pairs",
  "pcall",
  "print",
  "rawequal",
  "rawget",
  "rawlen",
  "rawset",
  "select",
  "setmetatable",
  "string",
  "table",
  "tonumber",
  "tostring",
  "type",
  "utf8",
  "xpcall",
])
const GLOBALS = new Set(["deck", "ctx"])

const LONG_BRACKET = /(--)?\[(=*)\[/y
const LEXEMES: Array<[LuaTokenKind | "space" | "name", RegExp]> = [
  ["space", /\s+/y],
  ["comment", /--[^\n]*/y],
  ["number", /0[xX][0-9a-fA-F]+|(?:\d+\.?\d*|\.\d+)(?:[eE][+-]?\d+)?/y],
  ["string", /"(?:\\[\s\S]|[^"\\\n])*"?|'(?:\\[\s\S]|[^'\\\n])*'?/y],
  ["name", /[A-Za-z_]\w*/y],
  ["operator", /\.\.\.?|[=~<>]=|::|[-+*/%^#&|~<>=(){}[\];:,.]/y],
]

const NEXT_CHARACTER = /\s*(\S)/y

function match(pattern: RegExp, source: string, index: number) {
  pattern.lastIndex = index
  return pattern.exec(source)
}

/** Splits Lua source into highlightable tokens; unterminated strings and comments run to the
 * end of their line (or, for long brackets, the source) so partial edits still color sensibly. */
export function tokenizeLua(source: string): LuaToken[] {
  const tokens: LuaToken[] = []
  let previous: LuaToken | undefined
  let index = 0
  while (index < source.length) {
    const long = match(LONG_BRACKET, source, index)
    let token: LuaToken | undefined
    if (long) {
      const close = `]${long[2] ?? ""}]`
      const end = source.indexOf(close, index + long[0].length)
      const stop = end < 0 ? source.length : end + close.length
      token = { kind: long[1] ? "comment" : "string", text: source.slice(index, stop) }
    } else {
      for (const [kind, pattern] of LEXEMES) {
        const text = match(pattern, source, index)?.[0]
        if (!text) continue
        if (kind === "space") token = { kind: "plain", text }
        else if (kind === "name") {
          const next = match(NEXT_CHARACTER, source, index + text.length)?.[1]
          token = { kind: classifyName(text, previous, next), text }
        } else token = { kind, text }
        break
      }
    }
    token ??= { kind: "plain", text: source.charAt(index) }
    tokens.push(token)
    if (token.kind !== "plain" || token.text.trim() !== "") previous = token
    index += token.text.length
  }
  return tokens
}

function classifyName(
  name: string,
  previous: LuaToken | undefined,
  next: string | undefined,
): LuaTokenKind {
  if (KEYWORDS.has(name)) return "keyword"
  if (CONSTANTS.has(name)) return "constant"
  if (next === "(" || (previous?.kind === "keyword" && previous.text === "function")) {
    return "function"
  }
  if (previous?.kind === "operator" && (previous.text === "." || previous.text === ":")) {
    return "field"
  }
  if (GLOBALS.has(name)) return "global"
  if (BUILTINS.has(name)) return "builtin"
  return "plain"
}

/** Lua source colored with the active palette (see `.lua-*` in app.css). */
export function LuaCode({ code }: { code: string }) {
  return (
    <>
      {tokenizeLua(code).map((token, index) =>
        token.kind === "plain" ? (
          token.text
        ) : (
          <span key={index} className={`lua-${token.kind}`}>
            {token.text}
          </span>
        ),
      )}
    </>
  )
}
