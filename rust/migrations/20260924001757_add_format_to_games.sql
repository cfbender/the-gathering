-- Migration 20260924001757: add format to games.

ALTER TABLE "games" ADD COLUMN "format" TEXT DEFAULT 'commander' NOT NULL;

