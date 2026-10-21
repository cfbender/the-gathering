-- AI rulings: the OpenRouter credentials and spend limit administrators configure, what
-- each member's rulings cost, and the Magic Comprehensive Rules the model looks rules up in.

-- Row 1 only. `api_key` is sealed with the server secret like other stored credentials;
-- `monthly_limit_usd` caps each member's spend per calendar month (UTC), NULL for no cap.
CREATE TABLE ai_settings (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  api_key TEXT,
  model TEXT,
  monthly_limit_usd REAL,
  inserted_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

-- One row per answered (or failed) question: OpenRouter's reported cost in USD.
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

-- Every numbered rule, chapter, and section heading of the installed rules, in document order.
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

-- Row 1 only: the installed document and the updater's last check.
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

-- ai_settings changes are audited without the key itself.
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
