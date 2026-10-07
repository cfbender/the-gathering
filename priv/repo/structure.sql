CREATE TABLE "schema_migrations" ("version" INTEGER PRIMARY KEY, "inserted_at" TEXT);
CREATE TABLE "users" ("id" INTEGER PRIMARY KEY AUTOINCREMENT, "username" TEXT NOT NULL, "display_name" TEXT NOT NULL, "role" TEXT DEFAULT 'member' NOT NULL, "disabled_at" TEXT, "inserted_at" TEXT NOT NULL, "updated_at" TEXT NOT NULL, "hashed_password" TEXT, "discord_id" TEXT, "avatar_url" TEXT, "moxfield_username" TEXT, "archidekt_username" TEXT, "manavault_url" TEXT, "manavault_api_key" TEXT, "palette" TEXT DEFAULT 'claret' NOT NULL, "theme_style" TEXT DEFAULT 'glass' NOT NULL);
CREATE TABLE sqlite_sequence(name,seq);
CREATE UNIQUE INDEX "users_username_index" ON "users" ("username");
CREATE TABLE "server_settings" ("id" INTEGER PRIMARY KEY, "registration_enabled" INTEGER DEFAULT false NOT NULL, "inserted_at" TEXT NOT NULL, "updated_at" TEXT NOT NULL, "detailed_stats_from" TEXT, "registration_invite_hash" BLOB);
CREATE TABLE "decks" ("id" INTEGER PRIMARY KEY AUTOINCREMENT, "player_id" INTEGER NOT NULL CONSTRAINT "decks_player_id_fkey" REFERENCES "players"("id") ON DELETE RESTRICT, "name" TEXT NOT NULL, "commander_card_id" TEXT, "commander_name" TEXT NOT NULL, "partner_card_id" TEXT, "partner_name" TEXT, "color_identity" TEXT DEFAULT '' NOT NULL, "decklist_url" TEXT, "decklist_source" TEXT, "archived_at" TEXT, "inserted_at" TEXT NOT NULL, "updated_at" TEXT NOT NULL, "skip_count" INTEGER DEFAULT 0 NOT NULL, "included_for_play" INTEGER DEFAULT true NOT NULL, "commander_printing_id" TEXT CONSTRAINT "decks_commander_printing_id_fkey" REFERENCES "card_printings"("id"), "partner_printing_id" TEXT CONSTRAINT "decks_partner_printing_id_fkey" REFERENCES "card_printings"("id"));
CREATE INDEX "decks_player_id_index" ON "decks" ("player_id");
CREATE UNIQUE INDEX decks_player_name_nocase_index ON decks (player_id, name COLLATE NOCASE);
CREATE TABLE "game_players" ("id" INTEGER PRIMARY KEY AUTOINCREMENT, "game_id" INTEGER NOT NULL CONSTRAINT "game_players_game_id_fkey" REFERENCES "games"("id") ON DELETE CASCADE, "player_id" INTEGER NOT NULL CONSTRAINT "game_players_player_id_fkey" REFERENCES "players"("id") ON DELETE RESTRICT, "deck_id" INTEGER CONSTRAINT "game_players_deck_id_fkey" REFERENCES "decks"("id") ON DELETE RESTRICT, "seat" INTEGER NOT NULL, "result" TEXT NOT NULL, "eliminated_turn" INTEGER, "eliminated_by_player_id" INTEGER CONSTRAINT "game_players_eliminated_by_player_id_fkey" REFERENCES "players"("id") ON DELETE RESTRICT, "mvp_card_id" TEXT, "mvp_card_name" TEXT, "notes" TEXT, "inserted_at" TEXT NOT NULL, "updated_at" TEXT NOT NULL, "kills" INTEGER);
CREATE UNIQUE INDEX "game_players_game_id_player_id_index" ON "game_players" ("game_id", "player_id");
CREATE UNIQUE INDEX "game_players_game_id_seat_index" ON "game_players" ("game_id", "seat");
CREATE INDEX "game_players_player_id_index" ON "game_players" ("player_id");
CREATE INDEX "game_players_deck_id_index" ON "game_players" ("deck_id");
CREATE TABLE "cards" ("id" TEXT PRIMARY KEY, "oracle_id" TEXT NOT NULL, "name" TEXT NOT NULL, "normalized_name" TEXT NOT NULL, "mana_cost" TEXT, "cmc" NUMERIC DEFAULT 0.0 NOT NULL, "type_line" TEXT NOT NULL, "oracle_text" TEXT, "colors" TEXT DEFAULT ('[]') NOT NULL, "color_identity" TEXT DEFAULT ('[]') NOT NULL, "image_uris" TEXT DEFAULT ('{}') NOT NULL, "set_code" TEXT NOT NULL, "collector_number" TEXT NOT NULL, "released_at" TEXT, "layout" TEXT NOT NULL, "rarity" TEXT NOT NULL, "commander_legal" INTEGER DEFAULT false NOT NULL, "can_be_commander" INTEGER DEFAULT false NOT NULL, "commander_pairing" TEXT, "inserted_at" TEXT NOT NULL, "updated_at" TEXT NOT NULL, "game_changer" INTEGER DEFAULT false NOT NULL);
CREATE UNIQUE INDEX "cards_oracle_id_index" ON "cards" ("oracle_id");
CREATE INDEX "cards_normalized_name_index" ON "cards" ("normalized_name");
CREATE INDEX "cards_can_be_commander_normalized_name_index" ON "cards" ("can_be_commander", "normalized_name");
CREATE TABLE "catalog_cards_staging" ("id" TEXT PRIMARY KEY, "oracle_id" TEXT NOT NULL, "name" TEXT NOT NULL, "normalized_name" TEXT NOT NULL, "mana_cost" TEXT, "cmc" NUMERIC DEFAULT 0.0 NOT NULL, "type_line" TEXT NOT NULL, "oracle_text" TEXT, "colors" TEXT DEFAULT ('[]') NOT NULL, "color_identity" TEXT DEFAULT ('[]') NOT NULL, "image_uris" TEXT DEFAULT ('{}') NOT NULL, "set_code" TEXT NOT NULL, "collector_number" TEXT NOT NULL, "released_at" TEXT, "layout" TEXT NOT NULL, "rarity" TEXT NOT NULL, "commander_legal" INTEGER DEFAULT false NOT NULL, "can_be_commander" INTEGER DEFAULT false NOT NULL, "commander_pairing" TEXT, "selection_key" TEXT NOT NULL, "inserted_at" TEXT NOT NULL, "updated_at" TEXT NOT NULL, "game_changer" INTEGER DEFAULT false NOT NULL);
CREATE UNIQUE INDEX "catalog_cards_staging_oracle_id_index" ON "catalog_cards_staging" ("oracle_id");
CREATE TABLE "catalog_syncs" ("id" INTEGER PRIMARY KEY AUTOINCREMENT, "status" TEXT DEFAULT 'never' NOT NULL, "last_started_at" TEXT, "last_finished_at" TEXT, "card_count" INTEGER DEFAULT 0 NOT NULL, "scryfall_updated_at" TEXT, "last_error" TEXT, "inserted_at" TEXT NOT NULL, "updated_at" TEXT NOT NULL);
CREATE TABLE "users_tokens" ("id" INTEGER PRIMARY KEY AUTOINCREMENT, "user_id" INTEGER NOT NULL CONSTRAINT "users_tokens_user_id_fkey" REFERENCES "users"("id") ON DELETE CASCADE, "token" BLOB NOT NULL, "context" TEXT NOT NULL, "sent_to" TEXT, "authenticated_at" TEXT, "inserted_at" TEXT NOT NULL);
CREATE INDEX "users_tokens_user_id_index" ON "users_tokens" ("user_id");
CREATE UNIQUE INDEX "users_tokens_context_token_index" ON "users_tokens" ("context", "token");
CREATE UNIQUE INDEX "users_discord_id_index" ON "users" ("discord_id");
CREATE TABLE players (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  name TEXT NOT NULL,
  user_id INTEGER,
  discord_id TEXT,
  archived_at TEXT,
  inserted_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
  , CONSTRAINT players_user_id_fkey FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE RESTRICT
);
CREATE UNIQUE INDEX players_user_id_index ON players (user_id);
CREATE UNIQUE INDEX players_discord_id_index ON players (discord_id);
CREATE UNIQUE INDEX players_name_nocase_index ON players (name COLLATE NOCASE);
CREATE TABLE games (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  played_at TEXT NOT NULL,
  duration_minutes INTEGER,
  turns INTEGER,
  notes TEXT,
  source TEXT NOT NULL DEFAULT 'manual',
  external_id TEXT,
  created_by_user_id INTEGER,
  inserted_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
  , "portable_id" TEXT, "win_condition" TEXT, "format" TEXT DEFAULT 'commander' NOT NULL, CONSTRAINT games_created_by_user_id_fkey FOREIGN KEY (created_by_user_id) REFERENCES users(id) ON DELETE RESTRICT
);
CREATE UNIQUE INDEX games_source_external_id_index
ON games (source, external_id)
WHERE external_id IS NOT NULL
;
CREATE INDEX games_played_at_index ON games (played_at);
CREATE TABLE "pending_discord_games" ("id" INTEGER PRIMARY KEY AUTOINCREMENT, "external_id" TEXT NOT NULL, "guild_id" TEXT NOT NULL, "channel_id" TEXT NOT NULL, "played_at" TEXT NOT NULL, "players" TEXT NOT NULL, "raw" TEXT DEFAULT ('{}') NOT NULL, "inserted_at" TEXT NOT NULL, "updated_at" TEXT NOT NULL);
CREATE UNIQUE INDEX "pending_discord_games_external_id_index" ON "pending_discord_games" ("external_id");
CREATE INDEX "pending_discord_games_channel_id_played_at_index" ON "pending_discord_games" ("channel_id", "played_at");
CREATE TABLE "card_printings" ("id" TEXT PRIMARY KEY, "oracle_id" TEXT NOT NULL, "name" TEXT NOT NULL, "set_code" TEXT NOT NULL, "set_name" TEXT NOT NULL, "collector_number" TEXT NOT NULL, "lang" TEXT DEFAULT 'en' NOT NULL, "image_uris" TEXT NOT NULL, "game_changer" INTEGER DEFAULT false NOT NULL);
CREATE TABLE "sheet_import_receipts" ("key" TEXT PRIMARY KEY, "game_id" INTEGER NOT NULL CONSTRAINT "sheet_import_receipts_game_id_fkey" REFERENCES "games"("id") ON DELETE CASCADE);
CREATE INDEX "sheet_import_receipts_game_id_index" ON "sheet_import_receipts" ("game_id");
CREATE UNIQUE INDEX "games_portable_id_index" ON "games" ("portable_id");
CREATE TABLE "discord_result_drafts" ("id" TEXT PRIMARY KEY, "pending_game_id" INTEGER NOT NULL CONSTRAINT "discord_result_drafts_pending_game_id_fkey" REFERENCES "pending_discord_games"("id") ON DELETE CASCADE, "discord_id" TEXT NOT NULL, "guild_id" TEXT NOT NULL, "channel_id" TEXT NOT NULL, "snapshot" BLOB NOT NULL, "data" TEXT DEFAULT ('{}') NOT NULL, "expires_at" TEXT NOT NULL);
CREATE INDEX "discord_result_drafts_pending_game_id_index" ON "discord_result_drafts" ("pending_game_id");
CREATE INDEX "discord_result_drafts_expires_at_index" ON "discord_result_drafts" ("expires_at");
CREATE TABLE "card_rulings_cache" ("id" TEXT PRIMARY KEY, "rulings" TEXT NOT NULL, "fetched_at" TEXT NOT NULL);
CREATE TABLE "discord_scheduled_games" ("id" INTEGER PRIMARY KEY AUTOINCREMENT, "guild_id" TEXT NOT NULL, "channel_id" TEXT NOT NULL, "message_id" TEXT, "host_discord_id" TEXT NOT NULL, "title" TEXT NOT NULL, "format" TEXT, "start_at" TEXT, "min_players" INTEGER DEFAULT 3 NOT NULL, "status" TEXT DEFAULT 'open' NOT NULL, "room_id" TEXT, "players" TEXT DEFAULT ('{}') NOT NULL, "announcement_id" TEXT, "message_dirty" INTEGER DEFAULT true NOT NULL, "inserted_at" TEXT NOT NULL, "updated_at" TEXT NOT NULL, "maybe" TEXT DEFAULT ('{}') NOT NULL, "maybe_pinged_at" TEXT, "maybe_ping_id" TEXT);
CREATE INDEX "discord_scheduled_games_status_start_at_index" ON "discord_scheduled_games" ("status", "start_at");
CREATE INDEX "discord_scheduled_games_message_dirty_index" ON "discord_scheduled_games" ("message_dirty");
CREATE TABLE "webcam_table_sessions" ("id" TEXT PRIMARY KEY, "snapshot" BLOB NOT NULL, "expires_at" TEXT NOT NULL);
CREATE INDEX "webcam_table_sessions_expires_at_index" ON "webcam_table_sessions" ("expires_at");
CREATE TABLE "card_details_cache" ("id" TEXT PRIMARY KEY, "details" TEXT NOT NULL, "fetched_at" TEXT NOT NULL);
CREATE TABLE "api_keys" ("id" INTEGER PRIMARY KEY AUTOINCREMENT, "user_id" INTEGER NOT NULL CONSTRAINT "api_keys_user_id_fkey" REFERENCES "users"("id") ON DELETE CASCADE, "name" TEXT NOT NULL, "token_hash" BLOB NOT NULL, "prefix" TEXT NOT NULL, "last_used_at" TEXT, "inserted_at" TEXT NOT NULL);
CREATE INDEX "api_keys_user_id_index" ON "api_keys" ("user_id");
CREATE UNIQUE INDEX "api_keys_token_hash_index" ON "api_keys" ("token_hash");
INSERT INTO schema_migrations VALUES(20260919195831,NULL);
INSERT INTO schema_migrations VALUES(20260919195952,NULL);
INSERT INTO schema_migrations VALUES(20260919200035,NULL);
INSERT INTO schema_migrations VALUES(20260919201759,NULL);
INSERT INTO schema_migrations VALUES(20260919221532,NULL);
INSERT INTO schema_migrations VALUES(20260920141424,NULL);
INSERT INTO schema_migrations VALUES(20260920142607,NULL);
INSERT INTO schema_migrations VALUES(20260920142610,NULL);
INSERT INTO schema_migrations VALUES(20260920143006,NULL);
INSERT INTO schema_migrations VALUES(20260920152610,NULL);
INSERT INTO schema_migrations VALUES(20260920162256,NULL);
INSERT INTO schema_migrations VALUES(20260921171051,NULL);
INSERT INTO schema_migrations VALUES(20260921193915,NULL);
INSERT INTO schema_migrations VALUES(20260921194157,NULL);
INSERT INTO schema_migrations VALUES(20260921203255,NULL);
INSERT INTO schema_migrations VALUES(20260921205311,NULL);
INSERT INTO schema_migrations VALUES(20260921233008,NULL);
INSERT INTO schema_migrations VALUES(20260922010255,NULL);
INSERT INTO schema_migrations VALUES(20260923032215,NULL);
INSERT INTO schema_migrations VALUES(20260923145842,NULL);
INSERT INTO schema_migrations VALUES(20260923185658,NULL);
INSERT INTO schema_migrations VALUES(20260923185819,NULL);
INSERT INTO schema_migrations VALUES(20260924001757,NULL);
INSERT INTO schema_migrations VALUES(20260924221950,NULL);
INSERT INTO schema_migrations VALUES(20260924222547,NULL);
INSERT INTO schema_migrations VALUES(20260925033311,NULL);
INSERT INTO schema_migrations VALUES(20260926020644,NULL);
INSERT INTO schema_migrations VALUES(20260927230551,NULL);
INSERT INTO schema_migrations VALUES(20260929224420,NULL);
INSERT INTO schema_migrations VALUES(20261006212354,NULL);
INSERT INTO schema_migrations VALUES(20261007074539,NULL);
