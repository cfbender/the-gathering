-- Migration 20260921203255: add portable id to games.

ALTER TABLE "games" ADD COLUMN "portable_id" TEXT;

CREATE UNIQUE INDEX "games_portable_id_index" ON "games" ("portable_id");

