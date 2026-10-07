-- Migration 20260921193915: add kills to game players.

ALTER TABLE "game_players" ADD COLUMN "kills" INTEGER;

