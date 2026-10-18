import { describe, expect, it } from "vite-plus/test"
import { DECK_SCRIPT_EXAMPLES } from "./deck-script-examples"
import { tokenizeLua } from "./lua-highlight"

const kinds = (source: string) =>
  tokenizeLua(source)
    .filter((token) => token.text.trim() !== "")
    .map((token) => `${token.kind}:${token.text}`)

describe("Lua highlighting", () => {
  it("keeps every character, so the colored text lines up under the editor", () => {
    const sources = [
      ...DECK_SCRIPT_EXAMPLES.map((example) => example.source),
      'x = "unterminated\nlocal y = [==[ long ]] still ]==] .. --[[ open comment',
    ]
    for (const source of sources) {
      expect(
        tokenizeLua(source)
          .map((token) => token.text)
          .join(""),
      ).toBe(source)
    }
  })

  it("colors keywords, members, calls, literals, and comments", () => {
    expect(
      kinds(`local function rematch(deck, ctx) -- note
  if deck.last_game ~= nil then return string.format("%d", 0x1F) end
  --[[ long
  comment ]] return ctx.now.hour * 1.5e2, [[raw]]
end`),
    ).toEqual([
      "keyword:local",
      "keyword:function",
      "function:rematch",
      "operator:(",
      "global:deck",
      "operator:,",
      "global:ctx",
      "operator:)",
      "comment:-- note",
      "keyword:if",
      "global:deck",
      "operator:.",
      "field:last_game",
      "operator:~=",
      "constant:nil",
      "keyword:then",
      "keyword:return",
      "builtin:string",
      "operator:.",
      "function:format",
      "operator:(",
      'string:"%d"',
      "operator:,",
      "number:0x1F",
      "operator:)",
      "keyword:end",
      "comment:--[[ long\n  comment ]]",
      "keyword:return",
      "global:ctx",
      "operator:.",
      "field:now",
      "operator:.",
      "field:hour",
      "operator:*",
      "number:1.5e2",
      "operator:,",
      "string:[[raw]]",
      "keyword:end",
    ])
  })
})
