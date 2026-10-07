-- Migration 20260921205311: add win condition to games.

ALTER TABLE "games" ADD COLUMN "win_condition" TEXT;

