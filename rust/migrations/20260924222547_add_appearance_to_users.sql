-- Generated from priv/repo/migrations/20260924222547_*.exs by rust/scripts/dump-migrations.py.

ALTER TABLE "users" ADD COLUMN "palette" TEXT DEFAULT 'claret' NOT NULL;

ALTER TABLE "users" ADD COLUMN "theme_style" TEXT DEFAULT 'glass' NOT NULL;

