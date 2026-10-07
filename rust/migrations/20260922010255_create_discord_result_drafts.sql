-- Generated from priv/repo/migrations/20260922010255_*.exs by rust/scripts/dump-migrations.py.

CREATE TABLE "discord_result_drafts" ("id" TEXT PRIMARY KEY, "pending_game_id" INTEGER NOT NULL CONSTRAINT "discord_result_drafts_pending_game_id_fkey" REFERENCES "pending_discord_games"("id") ON DELETE CASCADE, "discord_id" TEXT NOT NULL, "guild_id" TEXT NOT NULL, "channel_id" TEXT NOT NULL, "snapshot" BLOB NOT NULL, "data" TEXT DEFAULT ('{}') NOT NULL, "expires_at" TEXT NOT NULL);

CREATE INDEX "discord_result_drafts_pending_game_id_index" ON "discord_result_drafts" ("pending_game_id");

CREATE INDEX "discord_result_drafts_expires_at_index" ON "discord_result_drafts" ("expires_at");

