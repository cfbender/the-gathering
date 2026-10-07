-- Generated from priv/repo/migrations/20260920143006_*.exs by rust/scripts/dump-migrations.py.

ALTER TABLE "users" ADD COLUMN "moxfield_username" TEXT;

ALTER TABLE "users" ADD COLUMN "archidekt_username" TEXT;

ALTER TABLE "users" ADD COLUMN "manavault_url" TEXT;

