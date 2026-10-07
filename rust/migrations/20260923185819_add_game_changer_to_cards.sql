-- Generated from priv/repo/migrations/20260923185819_*.exs by rust/scripts/dump-migrations.py.

ALTER TABLE "cards" ADD COLUMN "game_changer" INTEGER DEFAULT false NOT NULL;

ALTER TABLE "catalog_cards_staging" ADD COLUMN "game_changer" INTEGER DEFAULT false NOT NULL;

ALTER TABLE "card_printings" ADD COLUMN "game_changer" INTEGER DEFAULT false NOT NULL;

