import { createFileRoute } from "@tanstack/react-router"
import { DeckScriptPage } from "@/features/decks/deck-script-page"

export const Route = createFileRoute("/decks/choose_/script")({ component: DeckScriptPage })
