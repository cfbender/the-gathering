-- Migration 20261018054335: add deck chooser script to users.

-- The member's Lua script that weights deck chooser picks; NULL uses the built-in weights.
ALTER TABLE "users" ADD COLUMN "deck_chooser_script" TEXT;
