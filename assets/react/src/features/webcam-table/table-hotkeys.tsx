import { useEffect, useEffectEvent } from "react"
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"

export const TABLE_HOTKEYS = [
  {
    keys: [" "],
    chips: ["Space"],
    action: "passTurn",
    section: "Turn & counters",
    title: "Next player turn",
    description: "Advance the turn once the match has started.",
  },
  {
    keys: ["shift+ "],
    chips: ["Shift + Space"],
    action: "unpassTurn",
    section: "Turn & counters",
    title: "Un-pass turn",
    description: "Hand the turn back to the previous player after a mistaken pass.",
  },
  {
    keys: ["arrowup"],
    chips: ["↑"],
    action: "gainLife",
    section: "Turn & counters",
    title: "Gain life",
    description: "Increase your life total by 1.",
  },
  {
    keys: ["arrowdown"],
    chips: ["↓"],
    action: "loseLife",
    section: "Turn & counters",
    title: "Lose life",
    description: "Decrease your life total by 1.",
  },
  {
    keys: ["shift+arrowup"],
    chips: ["Shift + ↑"],
    action: "gainTenLife",
    section: "Turn & counters",
    title: "Gain 10 life",
    description: "Increase your life total by 10.",
  },
  {
    keys: ["shift+arrowdown"],
    chips: ["Shift + ↓"],
    action: "loseTenLife",
    section: "Turn & counters",
    title: "Lose 10 life",
    description: "Decrease your life total by 10.",
  },
  {
    keys: ["["],
    chips: ["["],
    action: "loseTax",
    section: "Turn & counters",
    title: "Decrease commander tax",
    description: "Subtract 2 tax from your primary commander.",
  },
  {
    keys: ["]"],
    chips: ["]"],
    action: "gainTax",
    section: "Turn & counters",
    title: "Increase commander tax",
    description: "Add 2 tax to your primary commander.",
  },
  {
    keys: ["c"],
    chips: ["C"],
    action: "camera",
    section: "Video",
    title: "Toggle camera",
    description: "Enable or disable your camera.",
  },
  {
    keys: ["b"],
    chips: ["B"],
    action: "panel",
    section: "View & panels",
    title: "Side panel",
    description: "Collapse or expand the side panel.",
  },
  {
    keys: ["t"],
    chips: ["T"],
    action: "table",
    section: "View & panels",
    title: "Table",
    description: "Open table controls.",
  },
  {
    keys: ["d"],
    chips: ["D"],
    action: "decks",
    section: "View & panels",
    title: "Decks",
    description: "Open your decks.",
  },
  {
    keys: ["a"],
    chips: ["A"],
    action: "cards",
    section: "View & panels",
    title: "Cards",
    description: "Open identified cards and gallery search.",
  },
  {
    keys: ["l"],
    chips: ["L"],
    action: "log",
    section: "View & panels",
    title: "Log",
    description: "Open the table log.",
  },
  {
    keys: ["s"],
    chips: ["S"],
    action: "settings",
    section: "View & panels",
    title: "Settings",
    description: "Open table settings.",
  },
  {
    keys: [","],
    chips: [","],
    action: "previous",
    section: "View & panels",
    title: "Previous board",
    description: "Show the previous player's board.",
  },
  {
    keys: ["."],
    chips: ["."],
    action: "next",
    section: "View & panels",
    title: "Next board",
    description: "Show the next player's board.",
  },
  {
    keys: ["g"],
    chips: ["G"],
    action: "grid",
    section: "View & panels",
    title: "Grid view",
    description: "Switch between all cameras and following the active turn.",
  },
  {
    keys: ["?", "h"],
    chips: ["?", "H"],
    action: "help",
    section: "View & panels",
    title: "Keyboard shortcuts",
    description: "Toggle this menu.",
  },
] as const

export type TableAction = (typeof TABLE_HOTKEYS)[number]["action"] | "dismiss"

const TEXT_ENTRY =
  'input, textarea, select, [contenteditable]:not([contenteditable="false"]), [role="textbox"]'
const KEYBOARD_WIDGET = '[role="combobox"], [role="slider"], [role="separator"]'
const ACTIVATABLE = 'button, a, [role="button"], summary'
const MODAL_OVERLAY =
  '[aria-modal="true"], [role="alertdialog"], dialog[open], [role="menu"], [role="listbox"]'

export function isTypingTarget(target: EventTarget | null): boolean {
  return target instanceof Element && !!target.closest(`${TEXT_ENTRY}, ${KEYBOARD_WIDGET}`)
}

/** Text entry always keeps the keyboard. Other controls keep the keys they operate on only
 * when reached by keyboard: a click leaves focus behind without meaning to claim keys. */
function focusOwnsKey(target: EventTarget | null, key: string, pointerFocused: boolean) {
  if (!(target instanceof Element)) return false
  if (target.closest(TEXT_ENTRY)) return true
  if (pointerFocused) return false
  if (target.closest(KEYBOARD_WIDGET)) return true
  // Space on a keyboard-focused button/link must retain its native activation behavior.
  return key === " " && !!target.closest(ACTIVATABLE)
}

/** Modal overlays and overlays holding focus own the keyboard; hover previews such as
 * non-focusing popovers do not. */
function overlayOwnsKeyboard() {
  return (
    !!document.querySelector(MODAL_OVERLAY) || !!document.activeElement?.closest('[role="dialog"]')
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
    | "shiftKey"
    | "repeat"
    | "isComposing"
    | "defaultPrevented"
  >,
  {
    enabled,
    pickerOpen,
    overlayOpen,
    pointerFocused = false,
  }: { enabled: boolean; pickerOpen: boolean; overlayOpen: boolean; pointerFocused?: boolean },
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
  if (!enabled || pickerOpen) return null
  const key =
    event.shiftKey && (event.key.startsWith("Arrow") || event.key === " ")
      ? `shift+${event.key.toLowerCase()}`
      : event.key.toLowerCase()
  const action = TABLE_HOTKEYS.find(({ keys }) => (keys as readonly string[]).includes(key))?.action
  if (!action || focusOwnsKey(event.target, event.key, pointerFocused)) return null
  return action
}

/** A control left focused by a click (rail handle, Select or menu trigger, button) does not
 * own the keyboard: the table claims shortcuts in the capture phase, before that control's
 * handlers, then releases its focus. Keyboard-focused controls handle keys first as usual. */
export function useTableHotkeys(
  enabled: boolean,
  pickerOpen: boolean,
  onAction: (action: TableAction) => void,
) {
  const dispatch = useEffectEvent((event: KeyboardEvent, pointerFocused: boolean) => {
    const action = tableHotkeyAction(event, {
      enabled,
      pickerOpen,
      overlayOpen: overlayOwnsKeyboard(),
      pointerFocused,
    })
    if (!action) return false
    event.preventDefault()
    onAction(action)
    return true
  })

  useEffect(() => {
    let pointerInteraction = false
    let pointerFocus: EventTarget | null = null
    const onPointerDown = () => {
      pointerInteraction = true
    }
    const onFocusIn = (event: FocusEvent) => {
      pointerFocus = pointerInteraction ? event.target : null
    }
    const clickedFocus = (event: KeyboardEvent) =>
      event.target !== document.body && event.target === pointerFocus
    const claim = (event: KeyboardEvent) => {
      pointerInteraction = false
      if (!clickedFocus(event)) return
      // Releasing focus stops the control's keyup from activating it and lets later keys
      // reach the table directly.
      if (dispatch(event, true) && event.target instanceof HTMLElement) event.target.blur()
    }
    const handle = (event: KeyboardEvent) => {
      if (!clickedFocus(event)) dispatch(event, false)
    }
    window.addEventListener("pointerdown", onPointerDown, true)
    window.addEventListener("focusin", onFocusIn, true)
    window.addEventListener("keydown", claim, true)
    window.addEventListener("keydown", handle)
    return () => {
      window.removeEventListener("pointerdown", onPointerDown, true)
      window.removeEventListener("focusin", onFocusIn, true)
      window.removeEventListener("keydown", claim, true)
      window.removeEventListener("keydown", handle)
    }
  }, [])
}

export function HotkeyHelp({
  open,
  onOpenChange,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
}) {
  const shortcuts = [
    ...TABLE_HOTKEYS,
    {
      chips: ["/"],
      section: "Cards",
      title: "Quick gallery search",
      description: "Search by name, set or collector number when the card picker is open.",
    },
    {
      chips: ["1–5"],
      section: "Cards",
      title: "Choose a suggestion",
      description: "Select one of the card picker's five suggestions.",
    },
    {
      chips: ["Esc"],
      section: "Cards",
      title: "Close overlay",
      description: "Dismiss the card picker or dialog, even with shortcuts disabled.",
    },
  ]
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        className="border-white/10 bg-base-100 text-base-content sm:max-w-lg"
        onKeyDown={(event) => {
          if (
            !event.repeat &&
            !event.nativeEvent.isComposing &&
            !event.defaultPrevented &&
            !event.ctrlKey &&
            !event.metaKey &&
            !event.altKey &&
            !isTypingTarget(event.target) &&
            ["?", "h"].includes(event.key.toLowerCase())
          ) {
            event.preventDefault()
            onOpenChange(false)
          }
        }}
      >
        <DialogHeader className="shrink-0 border-white/10">
          <DialogTitle>Keyboard shortcuts</DialogTitle>
          <DialogClose onClose={() => onOpenChange(false)} />
        </DialogHeader>
        <div className="min-h-0 overflow-y-auto p-5">
          {["Turn & counters", "Video", "Cards", "View & panels"].map((section) => (
            <section key={section} className="mb-5">
              <h3 className="mb-2 text-[0.65rem] font-bold tracking-wider text-base-content/60 uppercase">
                {section}
              </h3>
              <div className="grid gap-2">
                {shortcuts
                  .filter((binding) => binding.section === section)
                  .map(({ chips, title, description }) => (
                    <div
                      key={title}
                      className="flex items-start gap-4 rounded-xl border border-white/10 p-3"
                    >
                      <div className="flex shrink-0 gap-1">
                        {chips.map((chip) => (
                          <kbd
                            key={chip}
                            className="rounded-lg border border-white/15 bg-base-300 px-2 py-2 text-xs font-bold"
                          >
                            {chip}
                          </kbd>
                        ))}
                      </div>
                      <div className="min-w-0">
                        <h4 className="text-sm font-bold">{title}</h4>
                        <p className="mt-1 text-xs text-base-content/60">{description}</p>
                      </div>
                    </div>
                  ))}
              </div>
            </section>
          ))}
          <p className="mt-3 text-xs text-base-content/60">
            Table shortcuts pause while typing or using an overlay. Enable them in Settings.
          </p>
        </div>
        <footer className="shrink-0 border-t border-white/10 px-5 py-4 text-xs text-base-content/60">
          Press <kbd className="kbd kbd-xs">?</kbd> or <kbd className="kbd kbd-xs">H</kbd> to toggle
          this menu
        </footer>
      </DialogContent>
    </Dialog>
  )
}
