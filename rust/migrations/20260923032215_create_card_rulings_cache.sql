-- Migration 20260923032215: create card rulings cache.

CREATE TABLE "card_rulings_cache" ("id" TEXT PRIMARY KEY, "rulings" TEXT NOT NULL, "fetched_at" TEXT NOT NULL);

