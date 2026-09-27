# Personal API keys

Members can read game history from scripts, spreadsheets, and other tools with a personal API
key. Create one under **Settings → API keys**. The key is shown once; only its SHA-256 digest is
stored, along with a short prefix so you can tell keys apart.

A key has no permissions of its own. Each request runs as the key's owner, using their current
account, so it sees exactly what they see when signed in. If an administrator disables the account,
its keys stop working immediately. Deleting the account deletes its keys. Keys never count as
recent password authentication, so they cannot perform actions that require it. Keys do not
expire. Revoke a key from Settings to stop it working.

Key-authenticated requests are limited to 120 per minute per owner.

## `GET /api/v1/games`

Lists games newest first. The request carries the key in the `Authorization` header:

```sh
curl -H "Authorization: Bearer tg_…" \
  "https://gathering.example.com/api/v1/games?player_id=me&date_from=2026-09-01&date_to=2026-09-30&tz=America/New_York"
```

| Parameter   | Description                                                                          |
| ----------- | ------------------------------------------------------------------------------------ |
| `player_id` | Only games this player played in. Use `me` for the player linked to the key's owner. |
| `date_from` | First day to include, `YYYY-MM-DD`.                                                  |
| `date_to`   | Last day to include, `YYYY-MM-DD`.                                                   |
| `tz`        | IANA time zone for `date_from` and `date_to`. Defaults to UTC.                       |
| `page`      | Page number, starting at 1.                                                          |
| `per_page`  | Games per page, default 20, maximum 100.                                             |

The response matches the app's own game list:

```json
{
  "data": [
    {
      "id": 42,
      "played_at": "2026-09-20T23:15:00Z",
      "format": "commander",
      "turns": 9,
      "win_condition": "combat",
      "seats": [
        { "seat": 1, "result": "win", "player": { "id": 3, "name": "Cody" }, "deck": { "…": "…" } }
      ]
    }
  ],
  "pagination": { "page": 1, "per_page": 20, "total": 1, "total_pages": 1 }
}
```

Errors use the rest of the API's format, `{"errors": {"detail": "…"}}`:

- `400`: a filter value is invalid, such as a non-numeric `player_id`, a malformed date, or an
  unknown time zone. Invalid values are rejected, never ignored.
- `401`: the key is missing, unknown, or revoked, or its owner is disabled.
- `404`: `player_id=me`, but no player is linked to the owner's account.
- `429`: the rate limit was exceeded. Wait for the number of seconds in the `retry-after` header.

Keys only work on `/api/v1` routes. The app's other `/api` endpoints still require a browser
session.
