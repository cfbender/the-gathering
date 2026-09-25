import type { PanelTab } from "./side-panel"
import { useTableHotkeys } from "./table-hotkeys"
import type { TableView } from "./table-view"
import { unpassTarget } from "./turns"
import type { CardIdentificationFlow } from "./use-card-identification-flow"

interface PanelControls {
  togglePanel: () => void
  showTab: (tab: PanelTab) => void
  openHelp: () => void
}

/** Routes table shortcuts to the seat, the active board, the picker, and the side panel. */
export function useRoomHotkeys(
  view: TableView,
  flow: Pick<CardIdentificationFlow, "pickerOpen" | "dismissPicker">,
  panel: PanelControls,
) {
  const { room, preferences, seated, activeParticipant } = view
  useTableHotkeys(preferences.hotkeys && !room.spectating, flow.pickerOpen, (action) => {
    switch (action) {
      case "gainLife":
        return room.changeLife(1)
      case "loseLife":
        return room.changeLife(-1)
      case "gainTenLife":
        return room.changeLife(10)
      case "loseTenLife":
        return room.changeLife(-10)
      case "passTurn":
        if (room.turns.active_player_id !== null) room.passTurn()
        return
      case "unpassTurn":
        if (unpassTarget(room.turns) !== null) room.unpassTurn()
        return
      case "gainTax":
      case "loseTax": {
        const deck = view.decks.find((deck) => deck.id === view.localParticipant.deck_id)
        if (deck)
          room.adjustCounter(
            { kind: "casts", commander: deck.commander_name },
            action === "gainTax" ? 1 : -1,
          )
        return
      }
      case "camera":
        return view.toggleCamera()
      case "panel":
        return panel.togglePanel()
      case "help":
        return panel.openHelp()
      case "grid":
        return preferences.update({ viewMode: preferences.viewMode === "grid" ? "follow" : "grid" })
      case "dismiss":
        return flow.dismissPicker()
      case "previous":
      case "next": {
        const index = seated.findIndex(
          (participant) => participant.peer_id === activeParticipant.peer_id,
        )
        const next = seated[(index + (action === "next" ? 1 : -1) + seated.length) % seated.length]
        if (next) view.selectBoard(next.peer_id)
        return
      }
      default:
        panel.showTab(action)
    }
  })
}
