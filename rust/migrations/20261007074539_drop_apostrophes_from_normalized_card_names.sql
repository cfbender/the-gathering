-- Migration 20261007074539: drop apostrophes from normalized card names.

UPDATE cards SET normalized_name = replace(replace(normalized_name, '''', ''), '’', '');

UPDATE catalog_cards_staging SET normalized_name = replace(replace(normalized_name, '''', ''), '’', '');

