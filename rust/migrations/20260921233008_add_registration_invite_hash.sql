-- Migration 20260921233008: add registration invite hash.

ALTER TABLE "server_settings" ADD COLUMN "registration_invite_hash" BLOB;

