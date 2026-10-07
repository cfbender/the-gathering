-- Migration 20260920141424: add detailed stats from to server settings.

ALTER TABLE "server_settings" ADD COLUMN "detailed_stats_from" TEXT;

