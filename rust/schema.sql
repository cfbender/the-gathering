CREATE TABLE "schema_migrations" ("version" INTEGER PRIMARY KEY, "inserted_at" TEXT);
CREATE TABLE "users" ("id" INTEGER PRIMARY KEY AUTOINCREMENT, "username" TEXT NOT NULL, "display_name" TEXT NOT NULL, "role" TEXT DEFAULT 'member' NOT NULL, "disabled_at" TEXT, "inserted_at" TEXT NOT NULL, "updated_at" TEXT NOT NULL, "hashed_password" TEXT, "discord_id" TEXT, "avatar_url" TEXT, "moxfield_username" TEXT, "archidekt_username" TEXT, "manavault_url" TEXT, "manavault_api_key" TEXT, "palette" TEXT DEFAULT 'claret' NOT NULL, "theme_style" TEXT DEFAULT 'glass' NOT NULL, "deck_chooser_script" TEXT);
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
WHERE external_id IS NOT NULL;
CREATE INDEX games_played_at_index ON games (played_at);
CREATE TABLE "pending_discord_games" ("id" INTEGER PRIMARY KEY AUTOINCREMENT, "external_id" TEXT NOT NULL, "guild_id" TEXT NOT NULL, "channel_id" TEXT NOT NULL, "played_at" TEXT NOT NULL, "players" TEXT NOT NULL, "raw" TEXT DEFAULT ('{}') NOT NULL, "inserted_at" TEXT NOT NULL, "updated_at" TEXT NOT NULL);
CREATE UNIQUE INDEX "pending_discord_games_external_id_index" ON "pending_discord_games" ("external_id");
CREATE INDEX "pending_discord_games_channel_id_played_at_index" ON "pending_discord_games" ("channel_id", "played_at");
CREATE TABLE "card_printings" ("id" TEXT PRIMARY KEY, "oracle_id" TEXT NOT NULL, "name" TEXT NOT NULL, "set_code" TEXT NOT NULL, "set_name" TEXT NOT NULL, "collector_number" TEXT NOT NULL, "lang" TEXT DEFAULT 'en' NOT NULL, "image_uris" TEXT NOT NULL, "game_changer" INTEGER DEFAULT false NOT NULL, "card" TEXT, "catalog_sync_id" INTEGER, "selectable" INTEGER DEFAULT false NOT NULL, "released_at" TEXT);
CREATE TABLE "sheet_import_receipts" ("key" TEXT PRIMARY KEY, "game_id" INTEGER NOT NULL CONSTRAINT "sheet_import_receipts_game_id_fkey" REFERENCES "games"("id") ON DELETE CASCADE);
CREATE INDEX "sheet_import_receipts_game_id_index" ON "sheet_import_receipts" ("game_id");
CREATE UNIQUE INDEX "games_portable_id_index" ON "games" ("portable_id");
CREATE TABLE "discord_result_drafts" ("id" TEXT PRIMARY KEY, "pending_game_id" INTEGER NOT NULL CONSTRAINT "discord_result_drafts_pending_game_id_fkey" REFERENCES "pending_discord_games"("id") ON DELETE CASCADE, "discord_id" TEXT NOT NULL, "guild_id" TEXT NOT NULL, "channel_id" TEXT NOT NULL, "snapshot" BLOB NOT NULL, "data" TEXT DEFAULT ('{}') NOT NULL, "expires_at" TEXT NOT NULL);
CREATE INDEX "discord_result_drafts_pending_game_id_index" ON "discord_result_drafts" ("pending_game_id");
CREATE INDEX "discord_result_drafts_expires_at_index" ON "discord_result_drafts" ("expires_at");
CREATE TABLE "discord_scheduled_games" ("id" INTEGER PRIMARY KEY AUTOINCREMENT, "guild_id" TEXT NOT NULL, "channel_id" TEXT NOT NULL, "message_id" TEXT, "host_discord_id" TEXT NOT NULL, "title" TEXT NOT NULL, "format" TEXT, "start_at" TEXT, "min_players" INTEGER DEFAULT 3 NOT NULL, "status" TEXT DEFAULT 'open' NOT NULL, "room_id" TEXT, "players" TEXT DEFAULT ('{}') NOT NULL, "announcement_id" TEXT, "message_dirty" INTEGER DEFAULT true NOT NULL, "inserted_at" TEXT NOT NULL, "updated_at" TEXT NOT NULL, "maybe" TEXT DEFAULT ('{}') NOT NULL, "maybe_pinged_at" TEXT, "maybe_ping_id" TEXT, "thread_id" TEXT);
CREATE INDEX "discord_scheduled_games_status_start_at_index" ON "discord_scheduled_games" ("status", "start_at");
CREATE INDEX "discord_scheduled_games_message_dirty_index" ON "discord_scheduled_games" ("message_dirty");
CREATE TABLE "webcam_table_sessions" ("id" TEXT PRIMARY KEY, "snapshot" BLOB NOT NULL, "expires_at" TEXT NOT NULL);
CREATE INDEX "webcam_table_sessions_expires_at_index" ON "webcam_table_sessions" ("expires_at");
CREATE TABLE "api_keys" ("id" INTEGER PRIMARY KEY AUTOINCREMENT, "user_id" INTEGER NOT NULL CONSTRAINT "api_keys_user_id_fkey" REFERENCES "users"("id") ON DELETE CASCADE, "name" TEXT NOT NULL, "token_hash" BLOB NOT NULL, "prefix" TEXT NOT NULL, "last_used_at" TEXT, "inserted_at" TEXT NOT NULL);
CREATE INDEX "api_keys_user_id_index" ON "api_keys" ("user_id");
CREATE UNIQUE INDEX "api_keys_token_hash_index" ON "api_keys" ("token_hash");
CREATE TABLE audit_operations (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  actor_id INTEGER,
  actor_name TEXT,
  action TEXT NOT NULL,
  target TEXT NOT NULL,
  request_id TEXT,
  status INTEGER,
  inserted_at TEXT NOT NULL,
  completed_at TEXT
);
CREATE INDEX audit_operations_inserted_at_index ON audit_operations (inserted_at);
CREATE INDEX audit_operations_actor_id_index ON audit_operations (actor_id);
CREATE TABLE audit_changes (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  operation_id INTEGER CONSTRAINT audit_changes_operation_id_fkey REFERENCES audit_operations(id) ON DELETE RESTRICT,
  entity TEXT NOT NULL,
  entity_id TEXT NOT NULL,
  before_json TEXT,
  after_json TEXT,
  inserted_at TEXT NOT NULL
);
CREATE INDEX audit_changes_operation_id_index ON audit_changes (operation_id);
CREATE INDEX audit_changes_entity_index ON audit_changes (entity, entity_id);
CREATE TABLE audit_context (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  operation_id INTEGER
);
CREATE TRIGGER audit_users_insert AFTER INSERT ON users
BEGIN
  INSERT INTO audit_changes (operation_id, entity, entity_id, before_json, after_json, inserted_at)
  VALUES (
    (SELECT operation_id FROM audit_context WHERE id = 1),
    'users',
    CAST(NEW.id AS TEXT),
    NULL,
    json_object(
      'id', NEW.id,
      'username', NEW.username,
      'display_name', NEW.display_name,
      'role', NEW.role,
      'disabled_at', NEW.disabled_at,
      'discord_id', NEW.discord_id,
      'avatar_url', NEW.avatar_url,
      'moxfield_username', NEW.moxfield_username,
      'archidekt_username', NEW.archidekt_username,
      'manavault_url', NEW.manavault_url,
      'palette', NEW.palette,
      'theme_style', NEW.theme_style,
      'inserted_at', NEW.inserted_at,
      'updated_at', NEW.updated_at,
      'password_set', json(CASE WHEN NEW.hashed_password IS NOT NULL THEN 'true' ELSE 'false' END),
      'manavault_api_key_set', json(CASE WHEN NEW.manavault_api_key IS NOT NULL THEN 'true' ELSE 'false' END)
    ),
    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
  );
END;
CREATE TRIGGER audit_users_update AFTER UPDATE ON users
WHEN OLD.id IS NOT NEW.id
  OR OLD.username IS NOT NEW.username
  OR OLD.display_name IS NOT NEW.display_name
  OR OLD.role IS NOT NEW.role
  OR OLD.disabled_at IS NOT NEW.disabled_at
  OR OLD.discord_id IS NOT NEW.discord_id
  OR OLD.avatar_url IS NOT NEW.avatar_url
  OR OLD.moxfield_username IS NOT NEW.moxfield_username
  OR OLD.archidekt_username IS NOT NEW.archidekt_username
  OR OLD.manavault_url IS NOT NEW.manavault_url
  OR OLD.palette IS NOT NEW.palette
  OR OLD.theme_style IS NOT NEW.theme_style
  OR OLD.inserted_at IS NOT NEW.inserted_at
  OR json(CASE WHEN OLD.hashed_password IS NOT NULL THEN 'true' ELSE 'false' END) IS NOT json(CASE WHEN NEW.hashed_password IS NOT NULL THEN 'true' ELSE 'false' END)
  OR json(CASE WHEN OLD.manavault_api_key IS NOT NULL THEN 'true' ELSE 'false' END) IS NOT json(CASE WHEN NEW.manavault_api_key IS NOT NULL THEN 'true' ELSE 'false' END)
BEGIN
  INSERT INTO audit_changes (operation_id, entity, entity_id, before_json, after_json, inserted_at)
  VALUES (
    (SELECT operation_id FROM audit_context WHERE id = 1),
    'users',
    CAST(NEW.id AS TEXT),
    json_object(
      'id', OLD.id,
      'username', OLD.username,
      'display_name', OLD.display_name,
      'role', OLD.role,
      'disabled_at', OLD.disabled_at,
      'discord_id', OLD.discord_id,
      'avatar_url', OLD.avatar_url,
      'moxfield_username', OLD.moxfield_username,
      'archidekt_username', OLD.archidekt_username,
      'manavault_url', OLD.manavault_url,
      'palette', OLD.palette,
      'theme_style', OLD.theme_style,
      'inserted_at', OLD.inserted_at,
      'updated_at', OLD.updated_at,
      'password_set', json(CASE WHEN OLD.hashed_password IS NOT NULL THEN 'true' ELSE 'false' END),
      'manavault_api_key_set', json(CASE WHEN OLD.manavault_api_key IS NOT NULL THEN 'true' ELSE 'false' END)
    ),
    json_object(
      'id', NEW.id,
      'username', NEW.username,
      'display_name', NEW.display_name,
      'role', NEW.role,
      'disabled_at', NEW.disabled_at,
      'discord_id', NEW.discord_id,
      'avatar_url', NEW.avatar_url,
      'moxfield_username', NEW.moxfield_username,
      'archidekt_username', NEW.archidekt_username,
      'manavault_url', NEW.manavault_url,
      'palette', NEW.palette,
      'theme_style', NEW.theme_style,
      'inserted_at', NEW.inserted_at,
      'updated_at', NEW.updated_at,
      'password_set', json(CASE WHEN NEW.hashed_password IS NOT NULL THEN 'true' ELSE 'false' END),
      'manavault_api_key_set', json(CASE WHEN NEW.manavault_api_key IS NOT NULL THEN 'true' ELSE 'false' END)
    ),
    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
  );
END;
CREATE TRIGGER audit_users_delete AFTER DELETE ON users
BEGIN
  INSERT INTO audit_changes (operation_id, entity, entity_id, before_json, after_json, inserted_at)
  VALUES (
    (SELECT operation_id FROM audit_context WHERE id = 1),
    'users',
    CAST(OLD.id AS TEXT),
    json_object(
      'id', OLD.id,
      'username', OLD.username,
      'display_name', OLD.display_name,
      'role', OLD.role,
      'disabled_at', OLD.disabled_at,
      'discord_id', OLD.discord_id,
      'avatar_url', OLD.avatar_url,
      'moxfield_username', OLD.moxfield_username,
      'archidekt_username', OLD.archidekt_username,
      'manavault_url', OLD.manavault_url,
      'palette', OLD.palette,
      'theme_style', OLD.theme_style,
      'inserted_at', OLD.inserted_at,
      'updated_at', OLD.updated_at,
      'password_set', json(CASE WHEN OLD.hashed_password IS NOT NULL THEN 'true' ELSE 'false' END),
      'manavault_api_key_set', json(CASE WHEN OLD.manavault_api_key IS NOT NULL THEN 'true' ELSE 'false' END)
    ),
    NULL,
    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
  );
END;
CREATE TRIGGER audit_players_insert AFTER INSERT ON players
BEGIN
  INSERT INTO audit_changes (operation_id, entity, entity_id, before_json, after_json, inserted_at)
  VALUES (
    (SELECT operation_id FROM audit_context WHERE id = 1),
    'players',
    CAST(NEW.id AS TEXT),
    NULL,
    json_object(
      'id', NEW.id,
      'name', NEW.name,
      'user_id', NEW.user_id,
      'discord_id', NEW.discord_id,
      'archived_at', NEW.archived_at,
      'inserted_at', NEW.inserted_at,
      'updated_at', NEW.updated_at
    ),
    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
  );
END;
CREATE TRIGGER audit_players_update AFTER UPDATE ON players
WHEN OLD.id IS NOT NEW.id
  OR OLD.name IS NOT NEW.name
  OR OLD.user_id IS NOT NEW.user_id
  OR OLD.discord_id IS NOT NEW.discord_id
  OR OLD.archived_at IS NOT NEW.archived_at
  OR OLD.inserted_at IS NOT NEW.inserted_at
BEGIN
  INSERT INTO audit_changes (operation_id, entity, entity_id, before_json, after_json, inserted_at)
  VALUES (
    (SELECT operation_id FROM audit_context WHERE id = 1),
    'players',
    CAST(NEW.id AS TEXT),
    json_object(
      'id', OLD.id,
      'name', OLD.name,
      'user_id', OLD.user_id,
      'discord_id', OLD.discord_id,
      'archived_at', OLD.archived_at,
      'inserted_at', OLD.inserted_at,
      'updated_at', OLD.updated_at
    ),
    json_object(
      'id', NEW.id,
      'name', NEW.name,
      'user_id', NEW.user_id,
      'discord_id', NEW.discord_id,
      'archived_at', NEW.archived_at,
      'inserted_at', NEW.inserted_at,
      'updated_at', NEW.updated_at
    ),
    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
  );
END;
CREATE TRIGGER audit_players_delete AFTER DELETE ON players
BEGIN
  INSERT INTO audit_changes (operation_id, entity, entity_id, before_json, after_json, inserted_at)
  VALUES (
    (SELECT operation_id FROM audit_context WHERE id = 1),
    'players',
    CAST(OLD.id AS TEXT),
    json_object(
      'id', OLD.id,
      'name', OLD.name,
      'user_id', OLD.user_id,
      'discord_id', OLD.discord_id,
      'archived_at', OLD.archived_at,
      'inserted_at', OLD.inserted_at,
      'updated_at', OLD.updated_at
    ),
    NULL,
    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
  );
END;
CREATE TRIGGER audit_decks_insert AFTER INSERT ON decks
BEGIN
  INSERT INTO audit_changes (operation_id, entity, entity_id, before_json, after_json, inserted_at)
  VALUES (
    (SELECT operation_id FROM audit_context WHERE id = 1),
    'decks',
    CAST(NEW.id AS TEXT),
    NULL,
    json_object(
      'id', NEW.id,
      'player_id', NEW.player_id,
      'name', NEW.name,
      'commander_card_id', NEW.commander_card_id,
      'commander_name', NEW.commander_name,
      'commander_printing_id', NEW.commander_printing_id,
      'partner_card_id', NEW.partner_card_id,
      'partner_name', NEW.partner_name,
      'partner_printing_id', NEW.partner_printing_id,
      'color_identity', NEW.color_identity,
      'decklist_url', NEW.decklist_url,
      'decklist_source', NEW.decklist_source,
      'skip_count', NEW.skip_count,
      'included_for_play', NEW.included_for_play,
      'archived_at', NEW.archived_at,
      'inserted_at', NEW.inserted_at,
      'updated_at', NEW.updated_at
    ),
    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
  );
END;
CREATE TRIGGER audit_decks_update AFTER UPDATE ON decks
WHEN OLD.id IS NOT NEW.id
  OR OLD.player_id IS NOT NEW.player_id
  OR OLD.name IS NOT NEW.name
  OR OLD.commander_card_id IS NOT NEW.commander_card_id
  OR OLD.commander_name IS NOT NEW.commander_name
  OR OLD.commander_printing_id IS NOT NEW.commander_printing_id
  OR OLD.partner_card_id IS NOT NEW.partner_card_id
  OR OLD.partner_name IS NOT NEW.partner_name
  OR OLD.partner_printing_id IS NOT NEW.partner_printing_id
  OR OLD.color_identity IS NOT NEW.color_identity
  OR OLD.decklist_url IS NOT NEW.decklist_url
  OR OLD.decklist_source IS NOT NEW.decklist_source
  OR OLD.skip_count IS NOT NEW.skip_count
  OR OLD.included_for_play IS NOT NEW.included_for_play
  OR OLD.archived_at IS NOT NEW.archived_at
  OR OLD.inserted_at IS NOT NEW.inserted_at
BEGIN
  INSERT INTO audit_changes (operation_id, entity, entity_id, before_json, after_json, inserted_at)
  VALUES (
    (SELECT operation_id FROM audit_context WHERE id = 1),
    'decks',
    CAST(NEW.id AS TEXT),
    json_object(
      'id', OLD.id,
      'player_id', OLD.player_id,
      'name', OLD.name,
      'commander_card_id', OLD.commander_card_id,
      'commander_name', OLD.commander_name,
      'commander_printing_id', OLD.commander_printing_id,
      'partner_card_id', OLD.partner_card_id,
      'partner_name', OLD.partner_name,
      'partner_printing_id', OLD.partner_printing_id,
      'color_identity', OLD.color_identity,
      'decklist_url', OLD.decklist_url,
      'decklist_source', OLD.decklist_source,
      'skip_count', OLD.skip_count,
      'included_for_play', OLD.included_for_play,
      'archived_at', OLD.archived_at,
      'inserted_at', OLD.inserted_at,
      'updated_at', OLD.updated_at
    ),
    json_object(
      'id', NEW.id,
      'player_id', NEW.player_id,
      'name', NEW.name,
      'commander_card_id', NEW.commander_card_id,
      'commander_name', NEW.commander_name,
      'commander_printing_id', NEW.commander_printing_id,
      'partner_card_id', NEW.partner_card_id,
      'partner_name', NEW.partner_name,
      'partner_printing_id', NEW.partner_printing_id,
      'color_identity', NEW.color_identity,
      'decklist_url', NEW.decklist_url,
      'decklist_source', NEW.decklist_source,
      'skip_count', NEW.skip_count,
      'included_for_play', NEW.included_for_play,
      'archived_at', NEW.archived_at,
      'inserted_at', NEW.inserted_at,
      'updated_at', NEW.updated_at
    ),
    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
  );
END;
CREATE TRIGGER audit_decks_delete AFTER DELETE ON decks
BEGIN
  INSERT INTO audit_changes (operation_id, entity, entity_id, before_json, after_json, inserted_at)
  VALUES (
    (SELECT operation_id FROM audit_context WHERE id = 1),
    'decks',
    CAST(OLD.id AS TEXT),
    json_object(
      'id', OLD.id,
      'player_id', OLD.player_id,
      'name', OLD.name,
      'commander_card_id', OLD.commander_card_id,
      'commander_name', OLD.commander_name,
      'commander_printing_id', OLD.commander_printing_id,
      'partner_card_id', OLD.partner_card_id,
      'partner_name', OLD.partner_name,
      'partner_printing_id', OLD.partner_printing_id,
      'color_identity', OLD.color_identity,
      'decklist_url', OLD.decklist_url,
      'decklist_source', OLD.decklist_source,
      'skip_count', OLD.skip_count,
      'included_for_play', OLD.included_for_play,
      'archived_at', OLD.archived_at,
      'inserted_at', OLD.inserted_at,
      'updated_at', OLD.updated_at
    ),
    NULL,
    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
  );
END;
CREATE TRIGGER audit_games_insert AFTER INSERT ON games
BEGIN
  INSERT INTO audit_changes (operation_id, entity, entity_id, before_json, after_json, inserted_at)
  VALUES (
    (SELECT operation_id FROM audit_context WHERE id = 1),
    'games',
    CAST(NEW.id AS TEXT),
    NULL,
    json_object(
      'id', NEW.id,
      'portable_id', NEW.portable_id,
      'played_at', NEW.played_at,
      'format', NEW.format,
      'duration_minutes', NEW.duration_minutes,
      'turns', NEW.turns,
      'win_condition', NEW.win_condition,
      'notes', NEW.notes,
      'source', NEW.source,
      'external_id', NEW.external_id,
      'created_by_user_id', NEW.created_by_user_id,
      'inserted_at', NEW.inserted_at,
      'updated_at', NEW.updated_at
    ),
    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
  );
END;
CREATE TRIGGER audit_games_update AFTER UPDATE ON games
WHEN OLD.id IS NOT NEW.id
  OR OLD.portable_id IS NOT NEW.portable_id
  OR OLD.played_at IS NOT NEW.played_at
  OR OLD.format IS NOT NEW.format
  OR OLD.duration_minutes IS NOT NEW.duration_minutes
  OR OLD.turns IS NOT NEW.turns
  OR OLD.win_condition IS NOT NEW.win_condition
  OR OLD.notes IS NOT NEW.notes
  OR OLD.source IS NOT NEW.source
  OR OLD.external_id IS NOT NEW.external_id
  OR OLD.created_by_user_id IS NOT NEW.created_by_user_id
  OR OLD.inserted_at IS NOT NEW.inserted_at
BEGIN
  INSERT INTO audit_changes (operation_id, entity, entity_id, before_json, after_json, inserted_at)
  VALUES (
    (SELECT operation_id FROM audit_context WHERE id = 1),
    'games',
    CAST(NEW.id AS TEXT),
    json_object(
      'id', OLD.id,
      'portable_id', OLD.portable_id,
      'played_at', OLD.played_at,
      'format', OLD.format,
      'duration_minutes', OLD.duration_minutes,
      'turns', OLD.turns,
      'win_condition', OLD.win_condition,
      'notes', OLD.notes,
      'source', OLD.source,
      'external_id', OLD.external_id,
      'created_by_user_id', OLD.created_by_user_id,
      'inserted_at', OLD.inserted_at,
      'updated_at', OLD.updated_at
    ),
    json_object(
      'id', NEW.id,
      'portable_id', NEW.portable_id,
      'played_at', NEW.played_at,
      'format', NEW.format,
      'duration_minutes', NEW.duration_minutes,
      'turns', NEW.turns,
      'win_condition', NEW.win_condition,
      'notes', NEW.notes,
      'source', NEW.source,
      'external_id', NEW.external_id,
      'created_by_user_id', NEW.created_by_user_id,
      'inserted_at', NEW.inserted_at,
      'updated_at', NEW.updated_at
    ),
    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
  );
END;
CREATE TRIGGER audit_games_delete AFTER DELETE ON games
BEGIN
  INSERT INTO audit_changes (operation_id, entity, entity_id, before_json, after_json, inserted_at)
  VALUES (
    (SELECT operation_id FROM audit_context WHERE id = 1),
    'games',
    CAST(OLD.id AS TEXT),
    json_object(
      'id', OLD.id,
      'portable_id', OLD.portable_id,
      'played_at', OLD.played_at,
      'format', OLD.format,
      'duration_minutes', OLD.duration_minutes,
      'turns', OLD.turns,
      'win_condition', OLD.win_condition,
      'notes', OLD.notes,
      'source', OLD.source,
      'external_id', OLD.external_id,
      'created_by_user_id', OLD.created_by_user_id,
      'inserted_at', OLD.inserted_at,
      'updated_at', OLD.updated_at
    ),
    NULL,
    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
  );
END;
CREATE TRIGGER audit_game_players_insert AFTER INSERT ON game_players
BEGIN
  INSERT INTO audit_changes (operation_id, entity, entity_id, before_json, after_json, inserted_at)
  VALUES (
    (SELECT operation_id FROM audit_context WHERE id = 1),
    'game_players',
    CAST(NEW.id AS TEXT),
    NULL,
    json_object(
      'id', NEW.id,
      'game_id', NEW.game_id,
      'player_id', NEW.player_id,
      'deck_id', NEW.deck_id,
      'seat', NEW.seat,
      'result', NEW.result,
      'kills', NEW.kills,
      'eliminated_turn', NEW.eliminated_turn,
      'eliminated_by_player_id', NEW.eliminated_by_player_id,
      'mvp_card_id', NEW.mvp_card_id,
      'mvp_card_name', NEW.mvp_card_name,
      'notes', NEW.notes,
      'inserted_at', NEW.inserted_at,
      'updated_at', NEW.updated_at
    ),
    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
  );
END;
CREATE TRIGGER audit_game_players_update AFTER UPDATE ON game_players
WHEN OLD.id IS NOT NEW.id
  OR OLD.game_id IS NOT NEW.game_id
  OR OLD.player_id IS NOT NEW.player_id
  OR OLD.deck_id IS NOT NEW.deck_id
  OR OLD.seat IS NOT NEW.seat
  OR OLD.result IS NOT NEW.result
  OR OLD.kills IS NOT NEW.kills
  OR OLD.eliminated_turn IS NOT NEW.eliminated_turn
  OR OLD.eliminated_by_player_id IS NOT NEW.eliminated_by_player_id
  OR OLD.mvp_card_id IS NOT NEW.mvp_card_id
  OR OLD.mvp_card_name IS NOT NEW.mvp_card_name
  OR OLD.notes IS NOT NEW.notes
  OR OLD.inserted_at IS NOT NEW.inserted_at
BEGIN
  INSERT INTO audit_changes (operation_id, entity, entity_id, before_json, after_json, inserted_at)
  VALUES (
    (SELECT operation_id FROM audit_context WHERE id = 1),
    'game_players',
    CAST(NEW.id AS TEXT),
    json_object(
      'id', OLD.id,
      'game_id', OLD.game_id,
      'player_id', OLD.player_id,
      'deck_id', OLD.deck_id,
      'seat', OLD.seat,
      'result', OLD.result,
      'kills', OLD.kills,
      'eliminated_turn', OLD.eliminated_turn,
      'eliminated_by_player_id', OLD.eliminated_by_player_id,
      'mvp_card_id', OLD.mvp_card_id,
      'mvp_card_name', OLD.mvp_card_name,
      'notes', OLD.notes,
      'inserted_at', OLD.inserted_at,
      'updated_at', OLD.updated_at
    ),
    json_object(
      'id', NEW.id,
      'game_id', NEW.game_id,
      'player_id', NEW.player_id,
      'deck_id', NEW.deck_id,
      'seat', NEW.seat,
      'result', NEW.result,
      'kills', NEW.kills,
      'eliminated_turn', NEW.eliminated_turn,
      'eliminated_by_player_id', NEW.eliminated_by_player_id,
      'mvp_card_id', NEW.mvp_card_id,
      'mvp_card_name', NEW.mvp_card_name,
      'notes', NEW.notes,
      'inserted_at', NEW.inserted_at,
      'updated_at', NEW.updated_at
    ),
    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
  );
END;
CREATE TRIGGER audit_game_players_delete AFTER DELETE ON game_players
BEGIN
  INSERT INTO audit_changes (operation_id, entity, entity_id, before_json, after_json, inserted_at)
  VALUES (
    (SELECT operation_id FROM audit_context WHERE id = 1),
    'game_players',
    CAST(OLD.id AS TEXT),
    json_object(
      'id', OLD.id,
      'game_id', OLD.game_id,
      'player_id', OLD.player_id,
      'deck_id', OLD.deck_id,
      'seat', OLD.seat,
      'result', OLD.result,
      'kills', OLD.kills,
      'eliminated_turn', OLD.eliminated_turn,
      'eliminated_by_player_id', OLD.eliminated_by_player_id,
      'mvp_card_id', OLD.mvp_card_id,
      'mvp_card_name', OLD.mvp_card_name,
      'notes', OLD.notes,
      'inserted_at', OLD.inserted_at,
      'updated_at', OLD.updated_at
    ),
    NULL,
    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
  );
END;
CREATE TRIGGER audit_server_settings_insert AFTER INSERT ON server_settings
BEGIN
  INSERT INTO audit_changes (operation_id, entity, entity_id, before_json, after_json, inserted_at)
  VALUES (
    (SELECT operation_id FROM audit_context WHERE id = 1),
    'server_settings',
    CAST(NEW.id AS TEXT),
    NULL,
    json_object(
      'id', NEW.id,
      'registration_enabled', NEW.registration_enabled,
      'detailed_stats_from', NEW.detailed_stats_from,
      'inserted_at', NEW.inserted_at,
      'updated_at', NEW.updated_at,
      'registration_invite_set', json(CASE WHEN NEW.registration_invite_hash IS NOT NULL THEN 'true' ELSE 'false' END)
    ),
    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
  );
END;
CREATE TRIGGER audit_server_settings_update AFTER UPDATE ON server_settings
WHEN OLD.id IS NOT NEW.id
  OR OLD.registration_enabled IS NOT NEW.registration_enabled
  OR OLD.detailed_stats_from IS NOT NEW.detailed_stats_from
  OR OLD.inserted_at IS NOT NEW.inserted_at
  OR json(CASE WHEN OLD.registration_invite_hash IS NOT NULL THEN 'true' ELSE 'false' END) IS NOT json(CASE WHEN NEW.registration_invite_hash IS NOT NULL THEN 'true' ELSE 'false' END)
BEGIN
  INSERT INTO audit_changes (operation_id, entity, entity_id, before_json, after_json, inserted_at)
  VALUES (
    (SELECT operation_id FROM audit_context WHERE id = 1),
    'server_settings',
    CAST(NEW.id AS TEXT),
    json_object(
      'id', OLD.id,
      'registration_enabled', OLD.registration_enabled,
      'detailed_stats_from', OLD.detailed_stats_from,
      'inserted_at', OLD.inserted_at,
      'updated_at', OLD.updated_at,
      'registration_invite_set', json(CASE WHEN OLD.registration_invite_hash IS NOT NULL THEN 'true' ELSE 'false' END)
    ),
    json_object(
      'id', NEW.id,
      'registration_enabled', NEW.registration_enabled,
      'detailed_stats_from', NEW.detailed_stats_from,
      'inserted_at', NEW.inserted_at,
      'updated_at', NEW.updated_at,
      'registration_invite_set', json(CASE WHEN NEW.registration_invite_hash IS NOT NULL THEN 'true' ELSE 'false' END)
    ),
    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
  );
END;
CREATE TRIGGER audit_server_settings_delete AFTER DELETE ON server_settings
BEGIN
  INSERT INTO audit_changes (operation_id, entity, entity_id, before_json, after_json, inserted_at)
  VALUES (
    (SELECT operation_id FROM audit_context WHERE id = 1),
    'server_settings',
    CAST(OLD.id AS TEXT),
    json_object(
      'id', OLD.id,
      'registration_enabled', OLD.registration_enabled,
      'detailed_stats_from', OLD.detailed_stats_from,
      'inserted_at', OLD.inserted_at,
      'updated_at', OLD.updated_at,
      'registration_invite_set', json(CASE WHEN OLD.registration_invite_hash IS NOT NULL THEN 'true' ELSE 'false' END)
    ),
    NULL,
    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
  );
END;
CREATE TRIGGER audit_api_keys_insert AFTER INSERT ON api_keys
BEGIN
  INSERT INTO audit_changes (operation_id, entity, entity_id, before_json, after_json, inserted_at)
  VALUES (
    (SELECT operation_id FROM audit_context WHERE id = 1),
    'api_keys',
    CAST(NEW.id AS TEXT),
    NULL,
    json_object(
      'id', NEW.id,
      'user_id', NEW.user_id,
      'name', NEW.name,
      'prefix', NEW.prefix,
      'last_used_at', NEW.last_used_at,
      'inserted_at', NEW.inserted_at
    ),
    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
  );
END;
CREATE TRIGGER audit_api_keys_update AFTER UPDATE ON api_keys
WHEN OLD.id IS NOT NEW.id
  OR OLD.user_id IS NOT NEW.user_id
  OR OLD.name IS NOT NEW.name
  OR OLD.prefix IS NOT NEW.prefix
  OR OLD.inserted_at IS NOT NEW.inserted_at
BEGIN
  INSERT INTO audit_changes (operation_id, entity, entity_id, before_json, after_json, inserted_at)
  VALUES (
    (SELECT operation_id FROM audit_context WHERE id = 1),
    'api_keys',
    CAST(NEW.id AS TEXT),
    json_object(
      'id', OLD.id,
      'user_id', OLD.user_id,
      'name', OLD.name,
      'prefix', OLD.prefix,
      'last_used_at', OLD.last_used_at,
      'inserted_at', OLD.inserted_at
    ),
    json_object(
      'id', NEW.id,
      'user_id', NEW.user_id,
      'name', NEW.name,
      'prefix', NEW.prefix,
      'last_used_at', NEW.last_used_at,
      'inserted_at', NEW.inserted_at
    ),
    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
  );
END;
CREATE TRIGGER audit_api_keys_delete AFTER DELETE ON api_keys
BEGIN
  INSERT INTO audit_changes (operation_id, entity, entity_id, before_json, after_json, inserted_at)
  VALUES (
    (SELECT operation_id FROM audit_context WHERE id = 1),
    'api_keys',
    CAST(OLD.id AS TEXT),
    json_object(
      'id', OLD.id,
      'user_id', OLD.user_id,
      'name', OLD.name,
      'prefix', OLD.prefix,
      'last_used_at', OLD.last_used_at,
      'inserted_at', OLD.inserted_at
    ),
    NULL,
    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
  );
END;
CREATE TRIGGER audit_sheet_import_receipts_insert AFTER INSERT ON sheet_import_receipts
BEGIN
  INSERT INTO audit_changes (operation_id, entity, entity_id, before_json, after_json, inserted_at)
  VALUES (
    (SELECT operation_id FROM audit_context WHERE id = 1),
    'sheet_import_receipts',
    NEW.key,
    NULL,
    json_object(
      'key', NEW.key,
      'game_id', NEW.game_id
    ),
    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
  );
END;
CREATE TRIGGER audit_sheet_import_receipts_update AFTER UPDATE ON sheet_import_receipts
WHEN OLD.key IS NOT NEW.key
  OR OLD.game_id IS NOT NEW.game_id
BEGIN
  INSERT INTO audit_changes (operation_id, entity, entity_id, before_json, after_json, inserted_at)
  VALUES (
    (SELECT operation_id FROM audit_context WHERE id = 1),
    'sheet_import_receipts',
    NEW.key,
    json_object(
      'key', OLD.key,
      'game_id', OLD.game_id
    ),
    json_object(
      'key', NEW.key,
      'game_id', NEW.game_id
    ),
    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
  );
END;
CREATE TRIGGER audit_sheet_import_receipts_delete AFTER DELETE ON sheet_import_receipts
BEGIN
  INSERT INTO audit_changes (operation_id, entity, entity_id, before_json, after_json, inserted_at)
  VALUES (
    (SELECT operation_id FROM audit_context WHERE id = 1),
    'sheet_import_receipts',
    OLD.key,
    json_object(
      'key', OLD.key,
      'game_id', OLD.game_id
    ),
    NULL,
    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
  );
END;
CREATE TRIGGER audit_pending_discord_games_insert AFTER INSERT ON pending_discord_games
BEGIN
  INSERT INTO audit_changes (operation_id, entity, entity_id, before_json, after_json, inserted_at)
  VALUES (
    (SELECT operation_id FROM audit_context WHERE id = 1),
    'pending_discord_games',
    CAST(NEW.id AS TEXT),
    NULL,
    json_object(
      'id', NEW.id,
      'external_id', NEW.external_id,
      'guild_id', NEW.guild_id,
      'channel_id', NEW.channel_id,
      'played_at', NEW.played_at,
      'players', json(CASE WHEN NEW.players IS NULL THEN NULL WHEN json_valid(NEW.players) THEN NEW.players ELSE json_quote(NEW.players) END),
      'inserted_at', NEW.inserted_at,
      'updated_at', NEW.updated_at
    ),
    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
  );
END;
CREATE TRIGGER audit_pending_discord_games_update AFTER UPDATE ON pending_discord_games
WHEN OLD.id IS NOT NEW.id
  OR OLD.external_id IS NOT NEW.external_id
  OR OLD.guild_id IS NOT NEW.guild_id
  OR OLD.channel_id IS NOT NEW.channel_id
  OR OLD.played_at IS NOT NEW.played_at
  OR json(CASE WHEN OLD.players IS NULL THEN NULL WHEN json_valid(OLD.players) THEN OLD.players ELSE json_quote(OLD.players) END) IS NOT json(CASE WHEN NEW.players IS NULL THEN NULL WHEN json_valid(NEW.players) THEN NEW.players ELSE json_quote(NEW.players) END)
  OR OLD.inserted_at IS NOT NEW.inserted_at
BEGIN
  INSERT INTO audit_changes (operation_id, entity, entity_id, before_json, after_json, inserted_at)
  VALUES (
    (SELECT operation_id FROM audit_context WHERE id = 1),
    'pending_discord_games',
    CAST(NEW.id AS TEXT),
    json_object(
      'id', OLD.id,
      'external_id', OLD.external_id,
      'guild_id', OLD.guild_id,
      'channel_id', OLD.channel_id,
      'played_at', OLD.played_at,
      'players', json(CASE WHEN OLD.players IS NULL THEN NULL WHEN json_valid(OLD.players) THEN OLD.players ELSE json_quote(OLD.players) END),
      'inserted_at', OLD.inserted_at,
      'updated_at', OLD.updated_at
    ),
    json_object(
      'id', NEW.id,
      'external_id', NEW.external_id,
      'guild_id', NEW.guild_id,
      'channel_id', NEW.channel_id,
      'played_at', NEW.played_at,
      'players', json(CASE WHEN NEW.players IS NULL THEN NULL WHEN json_valid(NEW.players) THEN NEW.players ELSE json_quote(NEW.players) END),
      'inserted_at', NEW.inserted_at,
      'updated_at', NEW.updated_at
    ),
    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
  );
END;
CREATE TRIGGER audit_pending_discord_games_delete AFTER DELETE ON pending_discord_games
BEGIN
  INSERT INTO audit_changes (operation_id, entity, entity_id, before_json, after_json, inserted_at)
  VALUES (
    (SELECT operation_id FROM audit_context WHERE id = 1),
    'pending_discord_games',
    CAST(OLD.id AS TEXT),
    json_object(
      'id', OLD.id,
      'external_id', OLD.external_id,
      'guild_id', OLD.guild_id,
      'channel_id', OLD.channel_id,
      'played_at', OLD.played_at,
      'players', json(CASE WHEN OLD.players IS NULL THEN NULL WHEN json_valid(OLD.players) THEN OLD.players ELSE json_quote(OLD.players) END),
      'inserted_at', OLD.inserted_at,
      'updated_at', OLD.updated_at
    ),
    NULL,
    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
  );
END;
CREATE TRIGGER audit_discord_result_drafts_insert AFTER INSERT ON discord_result_drafts
BEGIN
  INSERT INTO audit_changes (operation_id, entity, entity_id, before_json, after_json, inserted_at)
  VALUES (
    (SELECT operation_id FROM audit_context WHERE id = 1),
    'discord_result_drafts',
    NEW.id,
    NULL,
    json_object(
      'id', NEW.id,
      'pending_game_id', NEW.pending_game_id,
      'discord_id', NEW.discord_id,
      'guild_id', NEW.guild_id,
      'channel_id', NEW.channel_id,
      'data', json(CASE WHEN NEW.data IS NULL THEN NULL WHEN json_valid(NEW.data) THEN NEW.data ELSE json_quote(NEW.data) END),
      'expires_at', NEW.expires_at
    ),
    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
  );
END;
CREATE TRIGGER audit_discord_result_drafts_update AFTER UPDATE ON discord_result_drafts
WHEN OLD.id IS NOT NEW.id
  OR OLD.pending_game_id IS NOT NEW.pending_game_id
  OR OLD.discord_id IS NOT NEW.discord_id
  OR OLD.guild_id IS NOT NEW.guild_id
  OR OLD.channel_id IS NOT NEW.channel_id
  OR json(CASE WHEN OLD.data IS NULL THEN NULL WHEN json_valid(OLD.data) THEN OLD.data ELSE json_quote(OLD.data) END) IS NOT json(CASE WHEN NEW.data IS NULL THEN NULL WHEN json_valid(NEW.data) THEN NEW.data ELSE json_quote(NEW.data) END)
  OR OLD.expires_at IS NOT NEW.expires_at
BEGIN
  INSERT INTO audit_changes (operation_id, entity, entity_id, before_json, after_json, inserted_at)
  VALUES (
    (SELECT operation_id FROM audit_context WHERE id = 1),
    'discord_result_drafts',
    NEW.id,
    json_object(
      'id', OLD.id,
      'pending_game_id', OLD.pending_game_id,
      'discord_id', OLD.discord_id,
      'guild_id', OLD.guild_id,
      'channel_id', OLD.channel_id,
      'data', json(CASE WHEN OLD.data IS NULL THEN NULL WHEN json_valid(OLD.data) THEN OLD.data ELSE json_quote(OLD.data) END),
      'expires_at', OLD.expires_at
    ),
    json_object(
      'id', NEW.id,
      'pending_game_id', NEW.pending_game_id,
      'discord_id', NEW.discord_id,
      'guild_id', NEW.guild_id,
      'channel_id', NEW.channel_id,
      'data', json(CASE WHEN NEW.data IS NULL THEN NULL WHEN json_valid(NEW.data) THEN NEW.data ELSE json_quote(NEW.data) END),
      'expires_at', NEW.expires_at
    ),
    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
  );
END;
CREATE TRIGGER audit_discord_result_drafts_delete AFTER DELETE ON discord_result_drafts
BEGIN
  INSERT INTO audit_changes (operation_id, entity, entity_id, before_json, after_json, inserted_at)
  VALUES (
    (SELECT operation_id FROM audit_context WHERE id = 1),
    'discord_result_drafts',
    OLD.id,
    json_object(
      'id', OLD.id,
      'pending_game_id', OLD.pending_game_id,
      'discord_id', OLD.discord_id,
      'guild_id', OLD.guild_id,
      'channel_id', OLD.channel_id,
      'data', json(CASE WHEN OLD.data IS NULL THEN NULL WHEN json_valid(OLD.data) THEN OLD.data ELSE json_quote(OLD.data) END),
      'expires_at', OLD.expires_at
    ),
    NULL,
    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
  );
END;
CREATE INDEX "discord_scheduled_games_thread_id_index" ON "discord_scheduled_games" ("thread_id");
CREATE INDEX "card_printings_oracle_id_index" ON "card_printings" ("oracle_id");
CREATE TABLE "card_rulings" ("id" INTEGER PRIMARY KEY, "oracle_id" TEXT NOT NULL, "source" TEXT, "published_at" TEXT, "comment" TEXT NOT NULL);
CREATE INDEX "card_rulings_oracle_id_index" ON "card_rulings" ("oracle_id");
CREATE TABLE "card_rulings_staging" ("id" INTEGER PRIMARY KEY, "oracle_id" TEXT NOT NULL, "source" TEXT, "published_at" TEXT, "comment" TEXT NOT NULL);
CREATE TABLE ai_settings (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  api_key TEXT,
  model TEXT,
  monthly_limit_usd REAL,
  inserted_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE TABLE ai_ruling_usage (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  user_id INTEGER NOT NULL CONSTRAINT ai_ruling_usage_user_id_fkey REFERENCES users(id) ON DELETE CASCADE,
  model TEXT NOT NULL,
  cost_usd REAL DEFAULT 0 NOT NULL,
  prompt_tokens INTEGER DEFAULT 0 NOT NULL,
  completion_tokens INTEGER DEFAULT 0 NOT NULL,
  inserted_at TEXT NOT NULL
);
CREATE INDEX ai_ruling_usage_user_id_inserted_at_index ON ai_ruling_usage (user_id, inserted_at);
CREATE TABLE comprehensive_rules (
  position INTEGER PRIMARY KEY,
  number TEXT NOT NULL,
  text TEXT NOT NULL
);
CREATE INDEX comprehensive_rules_number_index ON comprehensive_rules (number);
CREATE TABLE comprehensive_rules_glossary (
  position INTEGER PRIMARY KEY,
  term TEXT NOT NULL,
  definition TEXT NOT NULL
);
CREATE TABLE comprehensive_rules_updates (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  source_url TEXT,
  effective_on TEXT,
  rule_count INTEGER DEFAULT 0 NOT NULL,
  glossary_count INTEGER DEFAULT 0 NOT NULL,
  installed_at TEXT,
  checked_at TEXT,
  last_error TEXT
);
CREATE TRIGGER audit_ai_settings_insert AFTER INSERT ON ai_settings
BEGIN
  INSERT INTO audit_changes (operation_id, entity, entity_id, before_json, after_json, inserted_at)
  VALUES (
    (SELECT operation_id FROM audit_context WHERE id = 1),
    'ai_settings',
    CAST(NEW.id AS TEXT),
    NULL,
    json_object(
      'id', NEW.id,
      'model', NEW.model,
      'monthly_limit_usd', NEW.monthly_limit_usd,
      'inserted_at', NEW.inserted_at,
      'updated_at', NEW.updated_at,
      'api_key_set', json(CASE WHEN NEW.api_key IS NOT NULL THEN 'true' ELSE 'false' END)
    ),
    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
  );
END;
CREATE TRIGGER audit_ai_settings_update AFTER UPDATE ON ai_settings
WHEN OLD.id IS NOT NEW.id
  OR OLD.model IS NOT NEW.model
  OR OLD.monthly_limit_usd IS NOT NEW.monthly_limit_usd
  OR OLD.inserted_at IS NOT NEW.inserted_at
  OR OLD.api_key IS NOT NEW.api_key
BEGIN
  INSERT INTO audit_changes (operation_id, entity, entity_id, before_json, after_json, inserted_at)
  VALUES (
    (SELECT operation_id FROM audit_context WHERE id = 1),
    'ai_settings',
    CAST(NEW.id AS TEXT),
    json_object(
      'id', OLD.id,
      'model', OLD.model,
      'monthly_limit_usd', OLD.monthly_limit_usd,
      'inserted_at', OLD.inserted_at,
      'updated_at', OLD.updated_at,
      'api_key_set', json(CASE WHEN OLD.api_key IS NOT NULL THEN 'true' ELSE 'false' END)
    ),
    json_object(
      'id', NEW.id,
      'model', NEW.model,
      'monthly_limit_usd', NEW.monthly_limit_usd,
      'inserted_at', NEW.inserted_at,
      'updated_at', NEW.updated_at,
      'api_key_set', json(CASE WHEN NEW.api_key IS NOT NULL THEN 'true' ELSE 'false' END)
    ),
    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
  );
END;
CREATE TRIGGER audit_ai_settings_delete AFTER DELETE ON ai_settings
BEGIN
  INSERT INTO audit_changes (operation_id, entity, entity_id, before_json, after_json, inserted_at)
  VALUES (
    (SELECT operation_id FROM audit_context WHERE id = 1),
    'ai_settings',
    CAST(OLD.id AS TEXT),
    json_object(
      'id', OLD.id,
      'model', OLD.model,
      'monthly_limit_usd', OLD.monthly_limit_usd,
      'inserted_at', OLD.inserted_at,
      'updated_at', OLD.updated_at,
      'api_key_set', json(CASE WHEN OLD.api_key IS NOT NULL THEN 'true' ELSE 'false' END)
    ),
    NULL,
    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
  );
END;
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
INSERT INTO schema_migrations VALUES(20261007125350,NULL);
INSERT INTO schema_migrations VALUES(20261010063504,NULL);
INSERT INTO schema_migrations VALUES(20261012050542,NULL);
INSERT INTO schema_migrations VALUES(20261014020504,NULL);
INSERT INTO schema_migrations VALUES(20261018042304,NULL);
INSERT INTO schema_migrations VALUES(20261018054335,NULL);
INSERT INTO schema_migrations VALUES(20261021040425,NULL);
