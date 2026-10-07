-- Migration 20260921171051: add deck printings.

CREATE TABLE "card_printings" ("id" TEXT PRIMARY KEY, "oracle_id" TEXT NOT NULL, "name" TEXT NOT NULL, "set_code" TEXT NOT NULL, "set_name" TEXT NOT NULL, "collector_number" TEXT NOT NULL, "lang" TEXT DEFAULT 'en' NOT NULL, "image_uris" TEXT NOT NULL);

ALTER TABLE "decks" ADD COLUMN "commander_printing_id" TEXT CONSTRAINT "decks_commander_printing_id_fkey" REFERENCES "card_printings"("id");

ALTER TABLE "decks" ADD COLUMN "partner_printing_id" TEXT CONSTRAINT "decks_partner_printing_id_fkey" REFERENCES "card_printings"("id");

