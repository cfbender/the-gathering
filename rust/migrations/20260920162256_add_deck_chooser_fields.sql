-- Migration 20260920162256: add deck chooser fields.

ALTER TABLE "decks" ADD COLUMN "skip_count" INTEGER DEFAULT 0 NOT NULL;

ALTER TABLE "decks" ADD COLUMN "included_for_play" INTEGER DEFAULT true NOT NULL;

