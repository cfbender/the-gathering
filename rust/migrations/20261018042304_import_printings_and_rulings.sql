-- Migration 20261018042304: import printings and rulings.
--
-- The catalog sync stores every printing of Scryfall's default_cards file and every ruling of its
-- rulings file, so printing pickers, printing details, and rulings no longer call the Scryfall API.

-- The rest of the Scryfall card object the details and pickers read (faces, rules text, prices),
-- the sync that last wrote the row (rows a sync no longer lists are pruned unless a deck uses
-- them), whether pickers list it (English, paper, not from a token or memorabilia set), and its
-- release date for ordering.
ALTER TABLE "card_printings" ADD COLUMN "card" TEXT;
ALTER TABLE "card_printings" ADD COLUMN "catalog_sync_id" INTEGER;
ALTER TABLE "card_printings" ADD COLUMN "selectable" INTEGER DEFAULT false NOT NULL;
ALTER TABLE "card_printings" ADD COLUMN "released_at" TEXT;

CREATE INDEX "card_printings_oracle_id_index" ON "card_printings" ("oracle_id");

CREATE TABLE "card_rulings" ("id" INTEGER PRIMARY KEY, "oracle_id" TEXT NOT NULL, "source" TEXT, "published_at" TEXT, "comment" TEXT NOT NULL);
CREATE INDEX "card_rulings_oracle_id_index" ON "card_rulings" ("oracle_id");
CREATE TABLE "card_rulings_staging" ("id" INTEGER PRIMARY KEY, "oracle_id" TEXT NOT NULL, "source" TEXT, "published_at" TEXT, "comment" TEXT NOT NULL);

DROP TABLE "card_details_cache";
DROP TABLE "card_rulings_cache";
