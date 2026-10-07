-- Migration 20260923185819: add game changer to cards.

ALTER TABLE "cards" ADD COLUMN "game_changer" INTEGER DEFAULT false NOT NULL;

ALTER TABLE "catalog_cards_staging" ADD COLUMN "game_changer" INTEGER DEFAULT false NOT NULL;

ALTER TABLE "card_printings" ADD COLUMN "game_changer" INTEGER DEFAULT false NOT NULL;

