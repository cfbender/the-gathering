-- How much of each question's prompt the provider read from or wrote to its prompt cache,
-- as OpenRouter reports it (both are part of prompt_tokens).
ALTER TABLE ai_ruling_usage ADD COLUMN cached_tokens INTEGER DEFAULT 0 NOT NULL;
ALTER TABLE ai_ruling_usage ADD COLUMN cache_write_tokens INTEGER DEFAULT 0 NOT NULL;
