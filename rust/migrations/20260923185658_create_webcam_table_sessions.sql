-- Migration 20260923185658: create webcam table sessions.

CREATE TABLE "webcam_table_sessions" ("id" TEXT PRIMARY KEY, "snapshot" BLOB NOT NULL, "expires_at" TEXT NOT NULL);

CREATE INDEX "webcam_table_sessions_expires_at_index" ON "webcam_table_sessions" ("expires_at");

