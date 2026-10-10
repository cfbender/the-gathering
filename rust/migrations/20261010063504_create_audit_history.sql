-- Audit history: one `audit_operations` row per audited request and one `audit_changes`
-- row per changed domain row, written by the triggers below. Snapshots list safe columns
-- explicitly; password hashes, API key token hashes, the ManaVault API key, the
-- registration invite hash, raw Discord payloads, and draft roster snapshots are never
-- copied (only whether a credential is set).
--
-- `audit_context` holds the operation the current write transaction belongs to.
-- `db::begin` sets it inside the transaction and `Tx::commit` clears it before committing,
-- so the committed value is always NULL and writes outside an audited transaction record
-- a NULL operation.

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

INSERT INTO audit_context (id, operation_id) VALUES (1, NULL);

-- users
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

-- players
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

-- decks
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

-- games
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

-- game_players
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

-- server_settings
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

-- api_keys
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

-- sheet_import_receipts
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

-- pending_discord_games
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

-- discord_result_drafts
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
