-- Generated from priv/repo/migrations/20260919195952_*.exs by rust/scripts/dump-migrations.py.

CREATE TABLE "players" ("id" INTEGER PRIMARY KEY AUTOINCREMENT, "name" TEXT NOT NULL, "user_id" INTEGER, "discord_id" TEXT, "archived_at" TEXT, "inserted_at" TEXT NOT NULL, "updated_at" TEXT NOT NULL);

CREATE UNIQUE INDEX "players_user_id_index" ON "players" ("user_id");

CREATE UNIQUE INDEX "players_discord_id_index" ON "players" ("discord_id");

CREATE UNIQUE INDEX players_name_nocase_index ON players (name COLLATE NOCASE);

CREATE TABLE "decks" ("id" INTEGER PRIMARY KEY AUTOINCREMENT, "player_id" INTEGER NOT NULL CONSTRAINT "decks_player_id_fkey" REFERENCES "players"("id") ON DELETE RESTRICT, "name" TEXT NOT NULL, "commander_card_id" TEXT, "commander_name" TEXT NOT NULL, "partner_card_id" TEXT, "partner_name" TEXT, "color_identity" TEXT DEFAULT '' NOT NULL, "decklist_url" TEXT, "decklist_source" TEXT, "archived_at" TEXT, "inserted_at" TEXT NOT NULL, "updated_at" TEXT NOT NULL);

CREATE INDEX "decks_player_id_index" ON "decks" ("player_id");

CREATE UNIQUE INDEX decks_player_name_nocase_index ON decks (player_id, name COLLATE NOCASE);

CREATE TABLE "games" ("id" INTEGER PRIMARY KEY AUTOINCREMENT, "played_at" TEXT NOT NULL, "duration_minutes" INTEGER, "turns" INTEGER, "notes" TEXT, "source" TEXT DEFAULT 'manual' NOT NULL, "external_id" TEXT, "created_by_user_id" INTEGER, "inserted_at" TEXT NOT NULL, "updated_at" TEXT NOT NULL);

CREATE UNIQUE INDEX "games_source_external_id_index" ON "games" ("source", "external_id") WHERE external_id IS NOT NULL;

CREATE INDEX "games_played_at_index" ON "games" ("played_at");

CREATE TABLE "game_players" ("id" INTEGER PRIMARY KEY AUTOINCREMENT, "game_id" INTEGER NOT NULL CONSTRAINT "game_players_game_id_fkey" REFERENCES "games"("id") ON DELETE CASCADE, "player_id" INTEGER NOT NULL CONSTRAINT "game_players_player_id_fkey" REFERENCES "players"("id") ON DELETE RESTRICT, "deck_id" INTEGER CONSTRAINT "game_players_deck_id_fkey" REFERENCES "decks"("id") ON DELETE RESTRICT, "seat" INTEGER NOT NULL, "result" TEXT NOT NULL, "eliminated_turn" INTEGER, "eliminated_by_player_id" INTEGER CONSTRAINT "game_players_eliminated_by_player_id_fkey" REFERENCES "players"("id") ON DELETE RESTRICT, "mvp_card_id" TEXT, "mvp_card_name" TEXT, "notes" TEXT, "inserted_at" TEXT NOT NULL, "updated_at" TEXT NOT NULL);

CREATE UNIQUE INDEX "game_players_game_id_player_id_index" ON "game_players" ("game_id", "player_id");

CREATE UNIQUE INDEX "game_players_game_id_seat_index" ON "game_players" ("game_id", "seat");

CREATE INDEX "game_players_player_id_index" ON "game_players" ("player_id");

CREATE INDEX "game_players_deck_id_index" ON "game_players" ("deck_id");

