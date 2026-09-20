import { createFileRoute } from "@tanstack/react-router"
import { DeckChooserPage } from "@/components/deck-chooser-page"

export const Route = createFileRoute("/decks/choose")({ component: DeckChooserPage })
