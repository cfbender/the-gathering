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
    shiftKey: false,
    repeat: false,
    isComposing: false,
    defaultPrevented: false,
    ...overrides,
  }
}

describe("table hotkeys", () => {
  it("dispatches life, camera, panels, cycling and help without reserving picker keys", () => {
    for (const [input, action] of [
      ["ArrowUp", "gainLife"],
      ["ArrowDown", "loseLife"],
      [" ", "passTurn"],
      ["C", "camera"],
      ["b", "panel"],
      ["t", "table"],
      ["d", "decks"],
      ["a", "cards"],
      ["l", "log"],
      ["s", "settings"],
      ["[", "loseTax"],
      ["]", "gainTax"],
      [",", "previous"],
      [".", "next"],
      ["?", "help"],
      ["H", "help"],
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
    expect(tableHotkeyAction(key("ArrowUp", { target: child }), context)).toBeNull()
    for (const role of ["textbox", "combobox", "slider", "separator"]) {
      const widget = document.createElement("div")
      widget.setAttribute("role", role)
      expect(tableHotkeyAction(key("ArrowDown", { target: widget }), context)).toBeNull()
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
      for (const input of ["ArrowUp", " ", "]"])
        expect(tableHotkeyAction(key(input, { [flag]: true }), context)).toBeNull()
    }
    for (const state of [{ enabled: false }, { pickerOpen: true }, { overlayOpen: true }]) {
      for (const input of ["ArrowUp", " ", "]"])
        expect(tableHotkeyAction(key(input), { ...context, ...state })).toBeNull()
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

  it("distinguishes one and ten life, pass and un-pass, and never fires them while searching", () => {
    expect(tableHotkeyAction(key(" ", { shiftKey: true }), context)).toBe("unpassTurn")
    expect(tableHotkeyAction(key("ArrowUp", { shiftKey: true }), context)).toBe("gainTenLife")
    expect(tableHotkeyAction(key("ArrowDown", { shiftKey: true }), context)).toBe("loseTenLife")
    const target = document.createElement("input")
    target.type = "search"
    for (const input of ["ArrowUp", "ArrowDown", " "]) {
      for (const shiftKey of [false, true]) {
        expect(tableHotkeyAction(key(input, { target, shiftKey }), context)).toBeNull()
        expect(
          tableHotkeyAction(key(input, { shiftKey }), { ...context, pickerOpen: true }),
        ).toBeNull()
      }
    }
  })

  it("preserves native Space activation for buttons and links, including nested children", () => {
    for (const tag of ["button", "a", "summary"]) {
      const control = document.createElement(tag)
      const child = control.appendChild(document.createElement("span"))
      expect(tableHotkeyAction(key(" ", { target: child }), context)).toBeNull()
      expect(tableHotkeyAction(key(" ", { target: child, shiftKey: true }), context)).toBeNull()
    }
  })

  it("lets controls left focused by a click give up keys, but never text entry", () => {
    const clicked = { ...context, pointerFocused: true }
    expect(tableHotkeyAction(key(" ", { target: document.createElement("button") }), clicked)).toBe(
      "passTurn",
    )
    for (const role of ["combobox", "slider", "separator"]) {
      const widget = document.createElement("div")
      widget.setAttribute("role", role)
      expect(tableHotkeyAction(key("ArrowUp", { target: widget }), clicked)).toBe("gainLife")
      expect(tableHotkeyAction(key(" ", { target: widget }), clicked)).toBe("passTurn")
    }
    for (const tag of ["input", "textarea", "select"]) {
      expect(
        tableHotkeyAction(key("ArrowUp", { target: document.createElement(tag) }), clicked),
      ).toBeNull()
    }
  })
})
