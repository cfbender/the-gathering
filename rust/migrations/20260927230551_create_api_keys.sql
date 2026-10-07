-- Migration 20260927230551: create api keys.

CREATE TABLE "api_keys" ("id" INTEGER PRIMARY KEY AUTOINCREMENT, "user_id" INTEGER NOT NULL CONSTRAINT "api_keys_user_id_fkey" REFERENCES "users"("id") ON DELETE CASCADE, "name" TEXT NOT NULL, "token_hash" BLOB NOT NULL, "prefix" TEXT NOT NULL, "last_used_at" TEXT, "inserted_at" TEXT NOT NULL);

CREATE INDEX "api_keys_user_id_index" ON "api_keys" ("user_id");

CREATE UNIQUE INDEX "api_keys_token_hash_index" ON "api_keys" ("token_hash");

