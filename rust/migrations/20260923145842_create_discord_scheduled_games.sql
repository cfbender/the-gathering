-- Migration 20260923145842: create discord scheduled games.

CREATE TABLE "discord_scheduled_games" ("id" INTEGER PRIMARY KEY AUTOINCREMENT, "guild_id" TEXT NOT NULL, "channel_id" TEXT NOT NULL, "message_id" TEXT, "host_discord_id" TEXT NOT NULL, "title" TEXT NOT NULL, "format" TEXT, "start_at" TEXT, "min_players" INTEGER DEFAULT 3 NOT NULL, "status" TEXT DEFAULT 'open' NOT NULL, "room_id" TEXT, "players" TEXT DEFAULT ('{}') NOT NULL, "announcement_id" TEXT, "message_dirty" INTEGER DEFAULT true NOT NULL, "inserted_at" TEXT NOT NULL, "updated_at" TEXT NOT NULL);

CREATE INDEX "discord_scheduled_games_status_start_at_index" ON "discord_scheduled_games" ("status", "start_at");

CREATE INDEX "discord_scheduled_games_message_dirty_index" ON "discord_scheduled_games" ("message_dirty");

