import { describe, expect, it } from "vite-plus/test"
import { tableHotkeyAction } from "./table-hotkeys"

const context = { enabled: true, pickerOpen: false, overlayOpen: false }
function key(key: string, overrides: Partial<KeyboardEvent> = {}) {
  return {
    key,
    target: document.body,
    altKey: false,
    ctrlKey: false,
    metaKey: false,
    repeat: false,
    isComposing: false,
    defaultPrevented: false,
    ...overrides,
  }
}

describe("table hotkeys", () => {
  it("dispatches life, camera, panels, cycling and help without reserving picker keys", () => {
    for (const [input, action] of [
      ["+", "gainLife"],
      ["=", "gainLife"],
      ["-", "loseLife"],
      ["C", "camera"],
      ["b", "panel"],
      ["t", "table"],
      ["d", "decks"],
      ["a", "cards"],
      ["l", "log"],
      ["s", "settings"],
      ["[", "previous"],
      ["]", "next"],
      ["?", "help"],
    ]) {
      expect(tableHotkeyAction(key(input!), context)).toBe(action)
    }
    for (const input of ["1", "2", "3", "4", "5", "/", "Escape", "Enter"]) {
      expect(tableHotkeyAction(key(input), context)).toBeNull()
      expect(tableHotkeyAction(key(input), { ...context, pickerOpen: true })).toBe(
        input === "Escape" ? "dismiss" : null,
      )
    }
  })

  it("ignores typing, nested editable content and keyboard-operated widgets", () => {
    for (const tag of ["input", "textarea", "select"]) {
      expect(
        tableHotkeyAction(key("c", { target: document.createElement(tag) }), context),
      ).toBeNull()
    }
    const editable = document.createElement("div")
    editable.setAttribute("contenteditable", "true")
    const child = editable.appendChild(document.createElement("span"))
    expect(tableHotkeyAction(key("+", { target: child }), context)).toBeNull()
    for (const role of ["textbox", "combobox", "slider", "separator"]) {
      const widget = document.createElement("div")
      widget.setAttribute("role", role)
      expect(tableHotkeyAction(key("-", { target: widget }), context)).toBeNull()
    }
    expect(tableHotkeyAction(key("c", { target: document.createElement("button") }), context)).toBe(
      "camera",
    )
  })

  it("respects disabled shortcuts, overlays, modifiers, repeats, IME and claimed events", () => {
    for (const flag of [
      "altKey",
      "ctrlKey",
      "metaKey",
      "repeat",
      "isComposing",
      "defaultPrevented",
    ]) {
      expect(tableHotkeyAction(key("+", { [flag]: true }), context)).toBeNull()
    }
    for (const state of [{ enabled: false }, { pickerOpen: true }, { overlayOpen: true }]) {
      expect(tableHotkeyAction(key("+"), { ...context, ...state })).toBeNull()
    }
    expect(
      tableHotkeyAction(key("Escape", { target: document.createElement("input") }), {
        ...context,
        enabled: false,
        pickerOpen: true,
      }),
    ).toBe("dismiss")
    expect(
      tableHotkeyAction(key("Escape"), { ...context, pickerOpen: true, overlayOpen: true }),
    ).toBeNull()
  })
})
