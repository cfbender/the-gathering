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
