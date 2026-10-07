#!/usr/bin/env python3
"""Turn `mix ecto.migrate --log-migrations-sql` output into one SQL file per Ecto migration.

The Rust server applies these files to databases that are missing Ecto migrations, recording
each version in `schema_migrations` exactly as Ecto does. Reads of data migrations (SELECTs)
are dropped: their effect is ported to Rust (see `db::migrate`).
"""
import re
import sys
from pathlib import Path

log = Path(sys.argv[1]).read_text()
out = Path(sys.argv[2])
out.mkdir(parents=True, exist_ok=True)
for old in out.glob("*.sql"):
    old.unlink()

running = re.compile(r"== Running (\d+) TheGathering\.Repo\.Migrations\.(\w+)\.")
blocks = re.split(r"\n(?=\d\d:\d\d:\d\d\.\d+ \[)", log)
current = None
statements = {}
names = {}
for block in blocks:
    match = running.search(block)
    if match:
        current = match.group(1)
        names[current] = re.sub(r"(?<!^)(?=[A-Z])", "_", match.group(2)).lower()
        statements[current] = []
        continue
    if current is None or "QUERY OK" not in block:
        continue
    sql = block.split("\n", 1)[1].rstrip()
    sql = re.sub(r"\s*\[[^\[\]]*\]$", "", sql).strip()
    if not sql or sql.upper().startswith("SELECT") or "schema_migrations" in sql:
        continue
    statements[current].append(sql)

for version, sqls in statements.items():
    body = "".join(f"{sql};\n\n" for sql in sqls)
    (out / f"{version}_{names[version]}.sql").write_text(
        f"-- Generated from priv/repo/migrations/{version}_*.exs by rust/scripts/dump-migrations.py.\n\n" + body
    )
print(f"wrote {len(statements)} migrations to {out}")
