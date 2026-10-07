-- Generated from priv/repo/migrations/20261007074539_*.exs by rust/scripts/dump-migrations.py.

UPDATE cards SET normalized_name = replace(replace(normalized_name, '''', ''), '’', '');

UPDATE catalog_cards_staging SET normalized_name = replace(replace(normalized_name, '''', ''), '’', '');

