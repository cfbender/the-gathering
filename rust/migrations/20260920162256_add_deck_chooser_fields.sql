-- Generated from priv/repo/migrations/20260920162256_*.exs by rust/scripts/dump-migrations.py.

ALTER TABLE "decks" ADD COLUMN "skip_count" INTEGER DEFAULT 0 NOT NULL;

ALTER TABLE "decks" ADD COLUMN "included_for_play" INTEGER DEFAULT true NOT NULL;

