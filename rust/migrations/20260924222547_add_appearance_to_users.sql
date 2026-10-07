-- Migration 20260924222547: add appearance to users.

ALTER TABLE "users" ADD COLUMN "palette" TEXT DEFAULT 'claret' NOT NULL;

ALTER TABLE "users" ADD COLUMN "theme_style" TEXT DEFAULT 'glass' NOT NULL;

