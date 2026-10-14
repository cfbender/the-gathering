-- Migration 20261014020504: add thread to discord scheduled games.

ALTER TABLE "discord_scheduled_games" ADD COLUMN "thread_id" TEXT;

CREATE INDEX "discord_scheduled_games_thread_id_index" ON "discord_scheduled_games" ("thread_id");
