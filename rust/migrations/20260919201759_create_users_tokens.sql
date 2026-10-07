-- Generated from priv/repo/migrations/20260919201759_*.exs by rust/scripts/dump-migrations.py.

CREATE TABLE "users_tokens" ("id" INTEGER PRIMARY KEY AUTOINCREMENT, "user_id" INTEGER NOT NULL CONSTRAINT "users_tokens_user_id_fkey" REFERENCES "users"("id") ON DELETE CASCADE, "token" BLOB NOT NULL, "context" TEXT NOT NULL, "sent_to" TEXT, "authenticated_at" TEXT, "inserted_at" TEXT NOT NULL);

CREATE INDEX "users_tokens_user_id_index" ON "users_tokens" ("user_id");

CREATE UNIQUE INDEX "users_tokens_context_token_index" ON "users_tokens" ("context", "token");

