#!/usr/bin/env bash
# Builds the schema from rust/migrations/*.sql, the schema's only source:
#
#   rust/target/schema.db   an empty database with every migration applied, which
#                           `sqlx::query!` macros compile against when SQLX_OFFLINE=false
#   rust/schema.sql         its dump (tables, indexes, and the schema_migrations versions),
#                           committed so a schema change shows up in review
#
# Migrations run in version order exactly as the server's migrator applies them to a new
# database. Steps that compute data in Rust (`db::migrate::data_step`) have nothing to do on
# an empty database, so plain SQL is enough here.
#
#   bash rust/scripts/schema.sh          rebuild both
#   bash rust/scripts/schema.sh --check  also fail when rust/schema.sql is out of date
set -euo pipefail
rust="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
db="$rust/target/schema.db"
out="$rust/schema.sql"

mkdir -p "$rust/target"
rm -f "$db" "$db-journal"
sqlite3 "$db" 'CREATE TABLE IF NOT EXISTS "schema_migrations" ("version" INTEGER PRIMARY KEY, "inserted_at" TEXT);'
for migration in "$rust"/migrations/*.sql; do
  name="$(basename "$migration" .sql)"
  version="${name%%_*}"
  [[ "$version" =~ ^[0-9]{14}$ ]] || {
    echo "migration $name must be named <14-digit version>_<name>.sql" >&2
    exit 1
  }
  sqlite3 -bail "$db" <"$migration"
  sqlite3 "$db" "INSERT INTO schema_migrations VALUES(${version},NULL);"
done

{
  sqlite3 "$db" .schema
  sqlite3 "$db" "SELECT 'INSERT INTO schema_migrations VALUES(' || version || ',NULL);' FROM schema_migrations ORDER BY version;"
} >"$out.new"

if [[ "${1:-}" == --check ]] && ! cmp -s "$out.new" "$out"; then
  diff -u "$out" "$out.new" || true
  rm -f "$out.new"
  echo "rust/schema.sql is out of date: run mise run rust:schema and commit it" >&2
  exit 1
fi
mv "$out.new" "$out"
