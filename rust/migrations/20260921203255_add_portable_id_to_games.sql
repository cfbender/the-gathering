-- Generated from priv/repo/migrations/20260921203255_*.exs by rust/scripts/dump-migrations.py.

ALTER TABLE "games" ADD COLUMN "portable_id" TEXT;

CREATE UNIQUE INDEX "games_portable_id_index" ON "games" ("portable_id");

