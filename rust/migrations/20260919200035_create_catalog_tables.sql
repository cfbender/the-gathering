-- Migration 20260919200035: create catalog tables.

CREATE TABLE "cards" ("id" TEXT PRIMARY KEY, "oracle_id" TEXT NOT NULL, "name" TEXT NOT NULL, "normalized_name" TEXT NOT NULL, "mana_cost" TEXT, "cmc" NUMERIC DEFAULT 0.0 NOT NULL, "type_line" TEXT NOT NULL, "oracle_text" TEXT, "colors" TEXT DEFAULT ('[]') NOT NULL, "color_identity" TEXT DEFAULT ('[]') NOT NULL, "image_uris" TEXT DEFAULT ('{}') NOT NULL, "set_code" TEXT NOT NULL, "collector_number" TEXT NOT NULL, "released_at" TEXT, "layout" TEXT NOT NULL, "rarity" TEXT NOT NULL, "commander_legal" INTEGER DEFAULT false NOT NULL, "can_be_commander" INTEGER DEFAULT false NOT NULL, "commander_pairing" TEXT, "inserted_at" TEXT NOT NULL, "updated_at" TEXT NOT NULL);

CREATE UNIQUE INDEX "cards_oracle_id_index" ON "cards" ("oracle_id");

CREATE INDEX "cards_normalized_name_index" ON "cards" ("normalized_name");

CREATE INDEX "cards_can_be_commander_normalized_name_index" ON "cards" ("can_be_commander", "normalized_name");

CREATE TABLE "catalog_cards_staging" ("id" TEXT PRIMARY KEY, "oracle_id" TEXT NOT NULL, "name" TEXT NOT NULL, "normalized_name" TEXT NOT NULL, "mana_cost" TEXT, "cmc" NUMERIC DEFAULT 0.0 NOT NULL, "type_line" TEXT NOT NULL, "oracle_text" TEXT, "colors" TEXT DEFAULT ('[]') NOT NULL, "color_identity" TEXT DEFAULT ('[]') NOT NULL, "image_uris" TEXT DEFAULT ('{}') NOT NULL, "set_code" TEXT NOT NULL, "collector_number" TEXT NOT NULL, "released_at" TEXT, "layout" TEXT NOT NULL, "rarity" TEXT NOT NULL, "commander_legal" INTEGER DEFAULT false NOT NULL, "can_be_commander" INTEGER DEFAULT false NOT NULL, "commander_pairing" TEXT, "selection_key" TEXT NOT NULL, "inserted_at" TEXT NOT NULL, "updated_at" TEXT NOT NULL);

CREATE UNIQUE INDEX "catalog_cards_staging_oracle_id_index" ON "catalog_cards_staging" ("oracle_id");

CREATE TABLE "catalog_syncs" ("id" INTEGER PRIMARY KEY AUTOINCREMENT, "status" TEXT DEFAULT 'never' NOT NULL, "last_started_at" TEXT, "last_finished_at" TEXT, "card_count" INTEGER DEFAULT 0 NOT NULL, "scryfall_updated_at" TEXT, "last_error" TEXT, "inserted_at" TEXT NOT NULL, "updated_at" TEXT NOT NULL);

