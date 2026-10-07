-- Generated from priv/repo/migrations/20260919195831_*.exs by rust/scripts/dump-migrations.py.

CREATE TABLE "users" ("id" INTEGER PRIMARY KEY AUTOINCREMENT, "username" TEXT NOT NULL, "display_name" TEXT NOT NULL, "hashed_password" TEXT NOT NULL, "role" TEXT DEFAULT 'member' NOT NULL, "disabled_at" TEXT, "inserted_at" TEXT NOT NULL, "updated_at" TEXT NOT NULL);

CREATE UNIQUE INDEX "users_username_index" ON "users" ("username");

CREATE TABLE "server_settings" ("id" INTEGER PRIMARY KEY, "registration_enabled" INTEGER DEFAULT false NOT NULL, "inserted_at" TEXT NOT NULL, "updated_at" TEXT NOT NULL);

INSERT INTO server_settings (id, registration_enabled, inserted_at, updated_at) VALUES (1, 0, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP);

