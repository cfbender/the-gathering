import { act, fireEvent, render, renderHook, screen } from "@testing-library/react"
import { expect, it, vi } from "vite-plus/test"
import { HotkeyHelp, useTableHotkeys } from "./table-hotkeys"

it("the installed Space listener follows preference changes and real dialog/input guards", () => {
  const onAction = vi.fn()
  const hook = renderHook(({ enabled }) => useTableHotkeys(enabled, false, onAction), {
    initialProps: { enabled: false },
  })
  fireEvent.keyDown(window, { key: " " })
  expect(onAction).not.toHaveBeenCalled()
  hook.rerender({ enabled: true })
  fireEvent.keyDown(window, { key: " " })
  expect(onAction).toHaveBeenLastCalledWith("passTurn")
  onAction.mockClear()
  const input = document.body.appendChild(document.createElement("input"))
  fireEvent.keyDown(input, { key: "ArrowUp", shiftKey: true })
  expect(onAction).not.toHaveBeenCalled()
  input.remove()
  const dialog = document.body.appendChild(document.createElement("div"))
  dialog.setAttribute("role", "alertdialog")
  fireEvent.keyDown(window, { key: " " })
  expect(onAction).not.toHaveBeenCalled()
  dialog.remove()
  hook.unmount()
  fireEvent.keyDown(window, { key: " " })
  expect(onAction).not.toHaveBeenCalled()
})

it("hover previews do not pause shortcuts, but a dialog holding focus does", () => {
  const onAction = vi.fn()
  const hook = renderHook(() => useTableHotkeys(true, false, onAction))
  const preview = document.body.appendChild(document.createElement("div"))
  preview.setAttribute("role", "dialog")
  fireEvent.keyDown(window, { key: "ArrowUp" })
  expect(onAction).toHaveBeenLastCalledWith("gainLife")
  onAction.mockClear()
  const inner = preview.appendChild(document.createElement("button"))
  inner.focus()
  fireEvent.keyDown(inner, { key: "ArrowUp" })
  expect(onAction).not.toHaveBeenCalled()
  preview.remove()
  hook.unmount()
})

it("clicked controls release the keyboard; keyboard-focused ones keep theirs", () => {
  const onAction = vi.fn()
  const hook = renderHook(() => useTableHotkeys(true, false, onAction))
  const button = document.body.appendChild(document.createElement("button"))
  // A focused control claiming the key in its own handler must not beat the table.
  button.addEventListener("keydown", (event) => event.preventDefault())

  fireEvent.pointerDown(button)
  button.focus()
  fireEvent.keyDown(button, { key: " " })
  expect(onAction).toHaveBeenLastCalledWith("passTurn")
  expect(document.activeElement).toBe(document.body)

  onAction.mockClear()
  fireEvent.keyDown(document.body, { key: "Tab" })
  button.focus()
  fireEvent.keyDown(button, { key: " " })
  expect(onAction).not.toHaveBeenCalled()
  expect(document.activeElement).toBe(button)

  const handle = document.body.appendChild(document.createElement("div"))
  handle.setAttribute("role", "separator")
  handle.tabIndex = 0
  fireEvent.pointerDown(handle)
  handle.focus()
  fireEvent.keyDown(handle, { key: "ArrowDown" })
  expect(onAction).toHaveBeenLastCalledWith("loseLife")

  button.remove()
  handle.remove()
  hook.unmount()
})

it("H closes help without leaking the key through to table actions", () => {
  const close = vi.fn()
  const onAction = vi.fn()
  const hook = renderHook(() => useTableHotkeys(true, false, onAction))
  const help = render(<HotkeyHelp open onOpenChange={close} />)
  act(() => {
    fireEvent.keyDown(screen.getByRole("dialog"), { key: "H" })
  })
  expect(close).toHaveBeenCalledWith(false)
  expect(onAction).not.toHaveBeenCalled()
  help.unmount()
  hook.unmount()
})
