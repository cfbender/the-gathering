-- Migration 20260925033311: create card details cache.

CREATE TABLE "card_details_cache" ("id" TEXT PRIMARY KEY, "details" TEXT NOT NULL, "fetched_at" TEXT NOT NULL);

