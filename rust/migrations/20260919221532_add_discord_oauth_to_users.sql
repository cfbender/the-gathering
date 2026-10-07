-- Generated from priv/repo/migrations/20260919221532_*.exs by rust/scripts/dump-migrations.py.

ALTER TABLE "users" ADD COLUMN "oauth_hashed_password" TEXT;

ALTER TABLE "users" ADD COLUMN "discord_id" TEXT;

ALTER TABLE "users" ADD COLUMN "avatar_url" TEXT;

UPDATE users SET oauth_hashed_password = hashed_password;

ALTER TABLE "users" DROP COLUMN "hashed_password";

ALTER TABLE "users" RENAME COLUMN "oauth_hashed_password" TO "hashed_password";

CREATE UNIQUE INDEX "users_discord_id_index" ON "users" ("discord_id");

