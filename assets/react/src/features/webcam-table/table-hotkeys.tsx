import { useEffect } from "react"
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"

export const TABLE_HOTKEYS = [
  { keys: ["+", "="], action: "gainLife", label: "Gain 1 life (your seat)" },
  { keys: ["-"], action: "loseLife", label: "Lose 1 life (your seat)" },
  { keys: ["c"], action: "camera", label: "Toggle your camera" },
  { keys: ["b"], action: "panel", label: "Collapse / expand side panel" },
  { keys: ["t"], action: "table", label: "Table panel" },
  { keys: ["d"], action: "decks", label: "Decks panel" },
  { keys: ["a"], action: "cards", label: "Cards panel" },
  { keys: ["l"], action: "log", label: "Log panel" },
  { keys: ["s"], action: "settings", label: "Settings panel" },
  { keys: ["["], action: "previous", label: "Previous board (pins selection)" },
  { keys: ["]"], action: "next", label: "Next board (pins selection)" },
  { keys: ["?"], action: "help", label: "Keyboard shortcuts" },
] as const

export type TableAction = (typeof TABLE_HOTKEYS)[number]["action"] | "dismiss"

export function isTypingTarget(target: EventTarget | null): boolean {
  return (
    target instanceof Element &&
    !!target.closest(
      'input, textarea, select, [contenteditable]:not([contenteditable="false"]), [role="textbox"], [role="combobox"], [role="slider"], [role="separator"]',
    )
  )
}

/** Existing overlays keep their own keyboard handling; Escape can dismiss the card picker
 * even with shortcuts disabled or its search field focused. Never consume its 1–5 or /. */
export function tableHotkeyAction(
  event: Pick<
    KeyboardEvent,
    | "key"
    | "target"
    | "altKey"
    | "ctrlKey"
    | "metaKey"
    | "repeat"
    | "isComposing"
    | "defaultPrevented"
  >,
  {
    enabled,
    pickerOpen,
    overlayOpen,
  }: { enabled: boolean; pickerOpen: boolean; overlayOpen: boolean },
): TableAction | null {
  if (
    event.defaultPrevented ||
    event.isComposing ||
    event.repeat ||
    event.altKey ||
    event.ctrlKey ||
    event.metaKey ||
    overlayOpen
  )
    return null
  if (event.key === "Escape" && pickerOpen) return "dismiss"
  if (!enabled || pickerOpen || isTypingTarget(event.target)) return null
  return (
    TABLE_HOTKEYS.find(({ keys }) => (keys as readonly string[]).includes(event.key.toLowerCase()))
      ?.action ?? null
  )
}

export function useTableHotkeys(
  enabled: boolean,
  pickerOpen: boolean,
  onAction: (action: TableAction) => void,
) {
  useEffect(() => {
    const handle = (event: KeyboardEvent) => {
      const overlayOpen = !!document.querySelector(
        '[role="dialog"], [role="menu"], [role="listbox"]',
      )
      const action = tableHotkeyAction(event, { enabled, pickerOpen, overlayOpen })
      if (!action) return
      event.preventDefault()
      onAction(action)
    }
    window.addEventListener("keydown", handle)
    return () => window.removeEventListener("keydown", handle)
  }, [enabled, pickerOpen, onAction])
}

export function HotkeyHelp({
  open,
  onOpenChange,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
}) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>Keyboard shortcuts</DialogTitle>
          <DialogClose onClose={() => onOpenChange(false)} />
        </DialogHeader>
        <dl className="grid grid-cols-[1fr_auto] gap-x-4 gap-y-2 p-5 text-sm">
          {TABLE_HOTKEYS.map(({ keys, action, label }) => (
            <div key={action} className="contents">
              <dt>{label}</dt>
              <dd>
                <kbd className="kbd kbd-sm">{keys.join(" / ")}</kbd>
              </dd>
            </div>
          ))}
          <dt>Close overlay</dt>
          <dd>
            <kbd className="kbd kbd-sm">Escape</kbd>
          </dd>
        </dl>
        <p className="px-5 pb-5 text-xs text-base-content/60">
          Shortcuts pause while typing or using an overlay. The card picker keeps 1–5 and / for
          selection and search. Toggle table shortcuts in Settings.
        </p>
      </DialogContent>
    </Dialog>
  )
}
