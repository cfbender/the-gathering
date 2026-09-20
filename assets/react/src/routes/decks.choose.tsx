import { createFileRoute } from "@tanstack/react-router"
import { DeckChooserPage } from "@/features/decks/deck-chooser-page"

export const Route = createFileRoute("/decks/choose")({ component: DeckChooserPage })
