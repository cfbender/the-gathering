-- Migration 20260920142610: create pending discord games.

CREATE TABLE "pending_discord_games" ("id" INTEGER PRIMARY KEY AUTOINCREMENT, "external_id" TEXT NOT NULL, "guild_id" TEXT NOT NULL, "channel_id" TEXT NOT NULL, "played_at" TEXT NOT NULL, "players" TEXT NOT NULL, "raw" TEXT DEFAULT ('{}') NOT NULL, "inserted_at" TEXT NOT NULL, "updated_at" TEXT NOT NULL);

CREATE UNIQUE INDEX "pending_discord_games_external_id_index" ON "pending_discord_games" ("external_id");

CREATE INDEX "pending_discord_games_channel_id_played_at_index" ON "pending_discord_games" ("channel_id", "played_at");

