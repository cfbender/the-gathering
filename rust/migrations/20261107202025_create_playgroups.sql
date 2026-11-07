-- Playgroups: a server hosts several circles of players, each with its own players, decks,
-- and games. Every existing player and game moves into one playgroup created here, and every
-- existing account becomes a member of it, so an upgraded database behaves as before until an
-- administrator adds another.
--
-- `players` and `games` are rebuilt so `playgroup_id` is NOT NULL; the per-server unique
-- indexes on player names, Discord ids, and linked accounts become per-playgroup, as does
-- the `(source, external_id)` identity of imported games. Accounts see only the playgroups
-- they belong to (`playgroup_members`) and remember the one they last switched to
-- (`users.playgroup_id`, cleared when that playgroup is deleted). A playgroup may claim a
-- Discord guild so the bot records that guild's games into it, and the reusable sign-up
-- invitation moves from the server settings to each playgroup, so an invite link joins the
-- playgroup it was created for.

PRAGMA foreign_keys = OFF;

PRAGMA legacy_alter_table = ON;

CREATE TABLE playgroups (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  name TEXT NOT NULL,
  discord_guild_id TEXT,
  registration_invite_hash BLOB,
  inserted_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE UNIQUE INDEX playgroups_name_nocase_index ON playgroups (name COLLATE NOCASE);

CREATE UNIQUE INDEX playgroups_discord_guild_id_index ON playgroups (discord_guild_id);

INSERT INTO playgroups (name, registration_invite_hash, inserted_at, updated_at)
VALUES (
  'Main',
  (SELECT registration_invite_hash FROM server_settings WHERE id = 1),
  strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
  strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
);

CREATE TABLE playgroup_members (
  playgroup_id INTEGER NOT NULL,
  user_id INTEGER NOT NULL,
  inserted_at TEXT NOT NULL,
  PRIMARY KEY (playgroup_id, user_id),
  CONSTRAINT playgroup_members_playgroup_id_fkey FOREIGN KEY (playgroup_id) REFERENCES playgroups(id) ON DELETE CASCADE,
  CONSTRAINT playgroup_members_user_id_fkey FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
);

CREATE INDEX playgroup_members_user_id_index ON playgroup_members (user_id);

INSERT INTO playgroup_members (playgroup_id, user_id, inserted_at)
SELECT (SELECT min(id) FROM playgroups), id, strftime('%Y-%m-%dT%H:%M:%SZ', 'now') FROM users;

CREATE TRIGGER audit_playgroups_insert AFTER INSERT ON playgroups
BEGIN
  INSERT INTO audit_changes (operation_id, entity, entity_id, before_json, after_json, inserted_at)
  VALUES (
    (SELECT operation_id FROM audit_context WHERE id = 1),
    'playgroups',
    CAST(NEW.id AS TEXT),
    NULL,
    json_object(
      'id', NEW.id,
      'name', NEW.name,
      'discord_guild_id', NEW.discord_guild_id,
      'inserted_at', NEW.inserted_at,
      'updated_at', NEW.updated_at,
      'registration_invite_set', json(CASE WHEN NEW.registration_invite_hash IS NOT NULL THEN 'true' ELSE 'false' END)
    ),
    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
  );
END;

CREATE TRIGGER audit_playgroups_update AFTER UPDATE ON playgroups
WHEN OLD.id IS NOT NEW.id
  OR OLD.name IS NOT NEW.name
  OR OLD.discord_guild_id IS NOT NEW.discord_guild_id
  OR OLD.inserted_at IS NOT NEW.inserted_at
  OR json(CASE WHEN OLD.registration_invite_hash IS NOT NULL THEN 'true' ELSE 'false' END) IS NOT json(CASE WHEN NEW.registration_invite_hash IS NOT NULL THEN 'true' ELSE 'false' END)
BEGIN
  INSERT INTO audit_changes (operation_id, entity, entity_id, before_json, after_json, inserted_at)
  VALUES (
    (SELECT operation_id FROM audit_context WHERE id = 1),
    'playgroups',
    CAST(NEW.id AS TEXT),
    json_object(
      'id', OLD.id,
      'name', OLD.name,
      'discord_guild_id', OLD.discord_guild_id,
      'inserted_at', OLD.inserted_at,
      'updated_at', OLD.updated_at,
      'registration_invite_set', json(CASE WHEN OLD.registration_invite_hash IS NOT NULL THEN 'true' ELSE 'false' END)
    ),
    json_object(
      'id', NEW.id,
      'name', NEW.name,
      'discord_guild_id', NEW.discord_guild_id,
      'inserted_at', NEW.inserted_at,
      'updated_at', NEW.updated_at,
      'registration_invite_set', json(CASE WHEN NEW.registration_invite_hash IS NOT NULL THEN 'true' ELSE 'false' END)
    ),
    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
  );
END;

CREATE TRIGGER audit_playgroups_delete AFTER DELETE ON playgroups
BEGIN
  INSERT INTO audit_changes (operation_id, entity, entity_id, before_json, after_json, inserted_at)
  VALUES (
    (SELECT operation_id FROM audit_context WHERE id = 1),
    'playgroups',
    CAST(OLD.id AS TEXT),
    json_object(
      'id', OLD.id,
      'name', OLD.name,
      'discord_guild_id', OLD.discord_guild_id,
      'inserted_at', OLD.inserted_at,
      'updated_at', OLD.updated_at,
      'registration_invite_set', json(CASE WHEN OLD.registration_invite_hash IS NOT NULL THEN 'true' ELSE 'false' END)
    ),
    NULL,
    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
  );
END;

CREATE TRIGGER audit_playgroup_members_insert AFTER INSERT ON playgroup_members
BEGIN
  INSERT INTO audit_changes (operation_id, entity, entity_id, before_json, after_json, inserted_at)
  VALUES (
    (SELECT operation_id FROM audit_context WHERE id = 1),
    'playgroup_members',
    CAST(NEW.playgroup_id AS TEXT) || ':' || CAST(NEW.user_id AS TEXT),
    NULL,
    json_object(
      'playgroup_id', NEW.playgroup_id,
      'user_id', NEW.user_id,
      'inserted_at', NEW.inserted_at
    ),
    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
  );
END;

CREATE TRIGGER audit_playgroup_members_delete AFTER DELETE ON playgroup_members
BEGIN
  INSERT INTO audit_changes (operation_id, entity, entity_id, before_json, after_json, inserted_at)
  VALUES (
    (SELECT operation_id FROM audit_context WHERE id = 1),
    'playgroup_members',
    CAST(OLD.playgroup_id AS TEXT) || ':' || CAST(OLD.user_id AS TEXT),
    json_object(
      'playgroup_id', OLD.playgroup_id,
      'user_id', OLD.user_id,
      'inserted_at', OLD.inserted_at
    ),
    NULL,
    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
  );
END;

-- server_settings: the invitation now lives on each playgroup.

DROP TRIGGER audit_server_settings_insert;

DROP TRIGGER audit_server_settings_update;

DROP TRIGGER audit_server_settings_delete;

ALTER TABLE server_settings DROP COLUMN registration_invite_hash;

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
      'updated_at', NEW.updated_at
    ),
    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
  );
END;

CREATE TRIGGER audit_server_settings_update AFTER UPDATE ON server_settings
WHEN OLD.id IS NOT NEW.id
  OR OLD.registration_enabled IS NOT NEW.registration_enabled
  OR OLD.detailed_stats_from IS NOT NEW.detailed_stats_from
  OR OLD.inserted_at IS NOT NEW.inserted_at
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
      'updated_at', OLD.updated_at
    ),
    json_object(
      'id', NEW.id,
      'registration_enabled', NEW.registration_enabled,
      'detailed_stats_from', NEW.detailed_stats_from,
      'inserted_at', NEW.inserted_at,
      'updated_at', NEW.updated_at
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
      'updated_at', OLD.updated_at
    ),
    NULL,
    strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
  );
END;

-- players

DROP TRIGGER audit_players_insert;

DROP TRIGGER audit_players_update;

DROP TRIGGER audit_players_delete;

DROP INDEX players_user_id_index;

DROP INDEX players_discord_id_index;

DROP INDEX players_name_nocase_index;

ALTER TABLE players RENAME TO players_old;

CREATE TABLE players (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  playgroup_id INTEGER NOT NULL,
  name TEXT NOT NULL,
  user_id INTEGER,
  discord_id TEXT,
  archived_at TEXT,
  inserted_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  CONSTRAINT players_playgroup_id_fkey FOREIGN KEY (playgroup_id) REFERENCES playgroups(id) ON DELETE RESTRICT,
  CONSTRAINT players_user_id_fkey FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE RESTRICT
);

INSERT INTO players
  (id, playgroup_id, name, user_id, discord_id, archived_at, inserted_at, updated_at)
SELECT id, (SELECT min(id) FROM playgroups), name, user_id, discord_id, archived_at, inserted_at, updated_at
FROM players_old;

DROP TABLE players_old;

CREATE INDEX players_playgroup_id_index ON players (playgroup_id);

CREATE UNIQUE INDEX players_playgroup_user_id_index ON players (playgroup_id, user_id);

CREATE UNIQUE INDEX players_playgroup_discord_id_index ON players (playgroup_id, discord_id);

CREATE UNIQUE INDEX players_playgroup_name_nocase_index ON players (playgroup_id, name COLLATE NOCASE);

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
      'playgroup_id', NEW.playgroup_id,
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
  OR OLD.playgroup_id IS NOT NEW.playgroup_id
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
      'playgroup_id', OLD.playgroup_id,
      'name', OLD.name,
      'user_id', OLD.user_id,
      'discord_id', OLD.discord_id,
      'archived_at', OLD.archived_at,
      'inserted_at', OLD.inserted_at,
      'updated_at', OLD.updated_at
    ),
    json_object(
      'id', NEW.id,
      'playgroup_id', NEW.playgroup_id,
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
      'playgroup_id', OLD.playgroup_id,
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

-- games

DROP TRIGGER audit_games_insert;

DROP TRIGGER audit_games_update;

DROP TRIGGER audit_games_delete;

DROP INDEX games_source_external_id_index;

DROP INDEX games_played_at_index;

DROP INDEX games_portable_id_index;

ALTER TABLE games RENAME TO games_old;

CREATE TABLE games (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  playgroup_id INTEGER NOT NULL,
  played_at TEXT NOT NULL,
  duration_minutes INTEGER,
  turns INTEGER,
  notes TEXT,
  source TEXT NOT NULL DEFAULT 'manual',
  external_id TEXT,
  created_by_user_id INTEGER,
  inserted_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  portable_id TEXT,
  win_condition TEXT,
  format TEXT DEFAULT 'commander' NOT NULL,
  CONSTRAINT games_playgroup_id_fkey FOREIGN KEY (playgroup_id) REFERENCES playgroups(id) ON DELETE RESTRICT,
  CONSTRAINT games_created_by_user_id_fkey FOREIGN KEY (created_by_user_id) REFERENCES users(id) ON DELETE RESTRICT
);

INSERT INTO games
  (id, playgroup_id, played_at, duration_minutes, turns, notes, source, external_id,
   created_by_user_id, inserted_at, updated_at, portable_id, win_condition, format)
SELECT id, (SELECT min(id) FROM playgroups), played_at, duration_minutes, turns, notes, source, external_id,
       created_by_user_id, inserted_at, updated_at, portable_id, win_condition, format
FROM games_old;

DROP TABLE games_old;

CREATE UNIQUE INDEX games_playgroup_source_external_id_index
ON games (playgroup_id, source, external_id)
WHERE external_id IS NOT NULL;

CREATE INDEX games_playgroup_played_at_index ON games (playgroup_id, played_at);

CREATE UNIQUE INDEX games_portable_id_index ON games (portable_id);

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
      'playgroup_id', NEW.playgroup_id,
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
  OR OLD.playgroup_id IS NOT NEW.playgroup_id
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
      'playgroup_id', OLD.playgroup_id,
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
      'playgroup_id', NEW.playgroup_id,
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
      'playgroup_id', OLD.playgroup_id,
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

-- users

ALTER TABLE users ADD COLUMN playgroup_id INTEGER REFERENCES playgroups(id) ON DELETE SET NULL;

PRAGMA legacy_alter_table = OFF;

PRAGMA foreign_keys = ON;
