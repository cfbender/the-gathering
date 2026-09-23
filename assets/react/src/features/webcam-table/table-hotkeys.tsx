import { useEffect } from "react"
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
    description: "Select and pin the previous board.",
  },
  {
    keys: ["."],
    chips: ["."],
    action: "next",
    section: "View & panels",
    title: "Next board",
    description: "Select and pin the next board.",
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
    | "shiftKey"
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
  // Space on a focused button/link must retain its native activation behavior.
  if (
    event.key === " " &&
    event.target instanceof Element &&
    event.target.closest('button, a, [role="button"], summary')
  )
    return null
  const key =
    event.shiftKey && event.key.startsWith("Arrow")
      ? `shift+${event.key.toLowerCase()}`
      : event.key.toLowerCase()
  if (event.shiftKey && event.key === " ") return null
  return TABLE_HOTKEYS.find(({ keys }) => (keys as readonly string[]).includes(key))?.action ?? null
}

export function useTableHotkeys(
  enabled: boolean,
  pickerOpen: boolean,
  onAction: (action: TableAction) => void,
) {
  useEffect(() => {
    const handle = (event: KeyboardEvent) => {
      const overlayOpen = !!document.querySelector(
        '[role="dialog"], [role="alertdialog"], dialog[open], [role="menu"], [role="listbox"]',
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
        className="border-slate-700 bg-slate-900 text-slate-200 sm:max-w-lg"
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
        <DialogHeader className="shrink-0 border-slate-700">
          <DialogTitle>Keyboard shortcuts</DialogTitle>
          <DialogClose onClose={() => onOpenChange(false)} />
        </DialogHeader>
        <div className="min-h-0 overflow-y-auto p-5">
          {["Turn & counters", "Video", "Cards", "View & panels"].map((section) => (
            <section key={section} className="mb-5">
              <h3 className="mb-2 text-[0.65rem] font-bold tracking-wider text-slate-400 uppercase">
                {section}
              </h3>
              <div className="grid gap-2">
                {shortcuts
                  .filter((binding) => binding.section === section)
                  .map(({ chips, title, description }) => (
                    <div
                      key={title}
                      className="flex items-start gap-4 rounded-xl border border-slate-700/70 p-3"
                    >
                      <div className="flex shrink-0 gap-1">
                        {chips.map((chip) => (
                          <kbd
                            key={chip}
                            className="rounded-lg border border-slate-600 bg-slate-800 px-2 py-2 text-xs font-bold"
                          >
                            {chip}
                          </kbd>
                        ))}
                      </div>
                      <div className="min-w-0">
                        <h4 className="text-sm font-bold">{title}</h4>
                        <p className="mt-1 text-xs text-slate-400">{description}</p>
                      </div>
                    </div>
                  ))}
              </div>
            </section>
          ))}
          <p className="mt-3 text-xs text-slate-400">
            Table shortcuts pause while typing or using an overlay. Enable them in Settings.
          </p>
        </div>
        <footer className="shrink-0 border-t border-slate-700 px-5 py-4 text-xs text-slate-400">
          Press <kbd className="kbd kbd-xs">?</kbd> or <kbd className="kbd kbd-xs">H</kbd> to toggle
          this menu
        </footer>
      </DialogContent>
    </Dialog>
  )
}
