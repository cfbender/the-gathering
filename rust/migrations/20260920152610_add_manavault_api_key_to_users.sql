-- Migration 20260920152610: add manavault api key to users.

ALTER TABLE "users" ADD COLUMN "manavault_api_key" TEXT;

