-- Migration 20260921194157: add sheet import receipts.

CREATE TABLE "sheet_import_receipts" ("key" TEXT PRIMARY KEY, "game_id" INTEGER NOT NULL CONSTRAINT "sheet_import_receipts_game_id_fkey" REFERENCES "games"("id") ON DELETE CASCADE);

CREATE INDEX "sheet_import_receipts_game_id_index" ON "sheet_import_receipts" ("game_id");

