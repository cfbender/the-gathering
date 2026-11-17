-- How much each player enjoyed a game: 1 (awful) to 5 (rad), rated by that player.
ALTER TABLE game_players ADD COLUMN satisfaction INTEGER CHECK (satisfaction BETWEEN 1 AND 5);

-- The audit history records the new column.
DROP TRIGGER audit_game_players_insert;
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
      'satisfaction', NEW.satisfaction,
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
DROP TRIGGER audit_game_players_update;
CREATE TRIGGER audit_game_players_update AFTER UPDATE ON game_players
WHEN OLD.id IS NOT NEW.id
  OR OLD.game_id IS NOT NEW.game_id
  OR OLD.player_id IS NOT NEW.player_id
  OR OLD.deck_id IS NOT NEW.deck_id
  OR OLD.seat IS NOT NEW.seat
  OR OLD.result IS NOT NEW.result
  OR OLD.kills IS NOT NEW.kills
  OR OLD.satisfaction IS NOT NEW.satisfaction
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
      'satisfaction', OLD.satisfaction,
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
      'satisfaction', NEW.satisfaction,
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
DROP TRIGGER audit_game_players_delete;
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
      'satisfaction', OLD.satisfaction,
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
