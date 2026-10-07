-- Migration 20260920142607: add user foreign keys to players and games.

PRAGMA foreign_keys = OFF;

PRAGMA legacy_alter_table = ON;

UPDATE players
SET user_id = NULL
WHERE user_id IS NOT NULL
  AND NOT EXISTS (SELECT 1 FROM users WHERE users.id = players.user_id);

DROP INDEX players_user_id_index;

DROP INDEX players_discord_id_index;

DROP INDEX players_name_nocase_index;

ALTER TABLE players RENAME TO players_old;

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

INSERT INTO players
  (id, name, user_id, discord_id, archived_at, inserted_at, updated_at)
SELECT id, name, user_id, discord_id, archived_at, inserted_at, updated_at
FROM players_old;

DROP TABLE players_old;

CREATE UNIQUE INDEX players_user_id_index ON players (user_id);

CREATE UNIQUE INDEX players_discord_id_index ON players (discord_id);

CREATE UNIQUE INDEX players_name_nocase_index ON players (name COLLATE NOCASE);

UPDATE games
SET created_by_user_id = NULL
WHERE created_by_user_id IS NOT NULL
  AND NOT EXISTS (SELECT 1 FROM users WHERE users.id = games.created_by_user_id);

DROP INDEX games_source_external_id_index;

DROP INDEX games_played_at_index;

ALTER TABLE games RENAME TO games_old;

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
  , CONSTRAINT games_created_by_user_id_fkey FOREIGN KEY (created_by_user_id) REFERENCES users(id) ON DELETE RESTRICT
);

INSERT INTO games
  (id, played_at, duration_minutes, turns, notes, source, external_id,
   created_by_user_id, inserted_at, updated_at)
SELECT id, played_at, duration_minutes, turns, notes, source, external_id,
       created_by_user_id, inserted_at, updated_at
FROM games_old;

DROP TABLE games_old;

CREATE UNIQUE INDEX games_source_external_id_index
ON games (source, external_id)
WHERE external_id IS NOT NULL;

CREATE INDEX games_played_at_index ON games (played_at);

PRAGMA legacy_alter_table = OFF;

PRAGMA foreign_keys = ON;

