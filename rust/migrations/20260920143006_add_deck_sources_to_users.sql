-- Migration 20260920143006: add deck sources to users.

ALTER TABLE "users" ADD COLUMN "moxfield_username" TEXT;

ALTER TABLE "users" ADD COLUMN "archidekt_username" TEXT;

ALTER TABLE "users" ADD COLUMN "manavault_url" TEXT;

