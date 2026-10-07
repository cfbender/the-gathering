-- Generated from priv/repo/migrations/20261006212354_*.exs by rust/scripts/dump-migrations.py.

ALTER TABLE "discord_scheduled_games" ADD COLUMN "maybe" TEXT DEFAULT ('{}') NOT NULL;

ALTER TABLE "discord_scheduled_games" ADD COLUMN "maybe_pinged_at" TEXT;

ALTER TABLE "discord_scheduled_games" ADD COLUMN "maybe_ping_id" TEXT;

UPDATE discord_scheduled_games SET message_dirty = 1 WHERE status = 'open' AND message_id IS NOT NULL;

