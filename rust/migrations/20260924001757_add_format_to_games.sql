-- Generated from priv/repo/migrations/20260924001757_*.exs by rust/scripts/dump-migrations.py.

ALTER TABLE "games" ADD COLUMN "format" TEXT DEFAULT 'commander' NOT NULL;

