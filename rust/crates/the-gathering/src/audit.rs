//! Audit history: who changed what, and the rows each change touched.
//!
//! An audited request records an `audit_operations` row with [`start`], runs its handler
//! inside [`scope`], and records the response status with [`finish`]. Every write
//! transaction opened with [`crate::db::begin`] inside the scope stores the operation id in
//! `audit_context` for its lifetime, and the database triggers on the domain tables copy an
//! allowlist of safe columns into `audit_changes` (`before_json` is `NULL` for inserts,
//! `after_json` is `NULL` for deletes, and cascaded deletions record each child row). Writes
//! outside a scope, such as background jobs, still record their changes with a `NULL`
//! operation. Rolled-back transactions leave no changes behind.
//!
//! Snapshots never include password hashes, API key token hashes, the ManaVault API key,
//! the registration invite hash, raw Discord payloads, or draft roster snapshots; they show
//! only whether a credential is set (`password_set`, `manavault_api_key_set`,
//! `registration_invite_set`). Updates that change no recorded column (other than
//! `updated_at`, or an API key's `last_used_at`) record nothing.

use std::future::Future;

use crate::accounts::User;
use crate::db::{Pool, UtcDateTime};

tokio::task_local! {
    static OPERATION_ID: i64;
}

/// Runs `future` with `operation_id` as the current operation, so the write transactions it
/// begins are attributed to it. The scope belongs to this future only: tasks it spawns, and
/// other tasks, do not inherit it.
pub async fn scope<F: Future>(operation_id: i64, future: F) -> F::Output {
    OPERATION_ID.scope(operation_id, future).await
}

/// The operation of the enclosing [`scope`], if any.
pub fn current_operation() -> Option<i64> {
    OPERATION_ID.try_with(|id| *id).ok()
}

/// Records the start of an operation and returns its id. `action` names the route (for
/// example `PATCH /api/games/{id}`) and `target` the request path; neither may hold
/// secrets. The actor's id and username are copied, so the record survives the account.
pub async fn start(
    pool: &Pool,
    actor: Option<&User>,
    action: &str,
    target: &str,
    request_id: Option<&str>,
) -> Result<i64, sqlx::Error> {
    let actor_id = actor.map(|user| user.id);
    let actor_name = actor.map(|user| user.username.as_str());
    let now = UtcDateTime::now();
    let row = sqlx::query!(
        "INSERT INTO audit_operations (actor_id, actor_name, action, target, request_id, inserted_at)
         VALUES (?, ?, ?, ?, ?, ?) RETURNING id",
        actor_id,
        actor_name,
        action,
        target,
        request_id,
        now
    )
    .fetch_one(pool)
    .await?;
    Ok(row.id)
}

/// Records the response status and completion time of operation `id`.
pub async fn finish(pool: &Pool, id: i64, status: u16) -> Result<(), sqlx::Error> {
    let status = i64::from(status);
    let now = UtcDateTime::now();
    sqlx::query!(
        "UPDATE audit_operations SET status = ?, completed_at = ? WHERE id = ?",
        status,
        now,
        id
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Attributes operation `id` to `user`, for a sign-in that started without an actor.
pub async fn identify(pool: &Pool, id: i64, user: &User) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE audit_operations SET actor_id = ?, actor_name = ? WHERE id = ?",
        user.id,
        user.username,
        id
    )
    .execute(pool)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use serde_json::Value;
    use sqlx::Row;

    use super::*;
    use crate::db;

    struct Change {
        operation_id: Option<i64>,
        entity: String,
        entity_id: String,
        before: Option<Value>,
        after: Option<Value>,
    }

    async fn pool(dir: &Path) -> Pool {
        let pool = db::connect(&dir.join("audit.db"), 4)
            .await
            .unwrap_or_else(|error| unreachable!("connect: {error}"));
        db::migrate::run(&pool)
            .await
            .unwrap_or_else(|error| unreachable!("migrate: {error:?}"));
        pool
    }

    fn json(text: Option<String>) -> Option<Value> {
        text.and_then(|text| serde_json::from_str(&text).ok())
    }

    async fn changes(pool: &Pool) -> Vec<Change> {
        sqlx::query(
            "SELECT operation_id, entity, entity_id, before_json, after_json FROM audit_changes ORDER BY id",
        )
        .fetch_all(pool)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|row| Change {
            operation_id: row.get(0),
            entity: row.get(1),
            entity_id: row.get(2),
            before: json(row.get(3)),
            after: json(row.get(4)),
        })
        .collect()
    }

    async fn context(pool: &Pool) -> Option<i64> {
        sqlx::query_scalar("SELECT operation_id FROM audit_context WHERE id = 1")
            .fetch_one(pool)
            .await
            .unwrap_or(Some(-1))
    }

    async fn exec(conn: &mut sqlx::SqliteConnection, sql: &'static str) -> i64 {
        sqlx::query(sql)
            .execute(conn)
            .await
            .unwrap_or_else(|error| unreachable!("{sql}: {error}"))
            .last_insert_rowid()
    }

    const NOW: &str = "2026-10-10T00:00:00Z";

    async fn insert_user(conn: &mut sqlx::SqliteConnection) -> i64 {
        sqlx::query(
            "INSERT INTO users (username, display_name, hashed_password, manavault_api_key, inserted_at, updated_at)
             VALUES ('ada', 'Ada', '$2b$12$secrethash', 'sealed-manavault-key', ?, ?)",
        )
        .bind(NOW)
        .bind(NOW)
        .execute(conn)
        .await
        .unwrap_or_else(|error| unreachable!("insert user: {error}"))
        .last_insert_rowid()
    }

    fn user(id: i64) -> User {
        User {
            id,
            username: "ada".into(),
            display_name: "Ada".into(),
            role: "admin".into(),
            disabled_at: None,
            hashed_password: None,
            discord_id: None,
            avatar_url: None,
            moxfield_username: None,
            archidekt_username: None,
            manavault_url: None,
            manavault_api_key: None,
            palette: "claret".into(),
            theme_style: "glass".into(),
            inserted_at: UtcDateTime::now(),
            updated_at: UtcDateTime::now(),
            authenticated_at: None,
        }
    }

    #[tokio::test]
    async fn operations_record_actor_status_and_completion() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| unreachable!("{error}"));
        let pool = pool(dir.path()).await;
        let id = start(
            &pool,
            None,
            "POST /api/session",
            "/api/session",
            Some("req-1"),
        )
        .await
        .unwrap_or_else(|error| unreachable!("{error}"));
        identify(&pool, id, &user(7))
            .await
            .unwrap_or_else(|error| unreachable!("{error}"));
        finish(&pool, id, 201)
            .await
            .unwrap_or_else(|error| unreachable!("{error}"));
        let row = sqlx::query(
            "SELECT actor_id, actor_name, action, target, request_id, status, completed_at IS NOT NULL FROM audit_operations WHERE id = ?",
        )
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap_or_else(|error| unreachable!("{error}"));
        assert_eq!(row.get::<Option<i64>, _>(0), Some(7));
        assert_eq!(row.get::<Option<String>, _>(1).as_deref(), Some("ada"));
        assert_eq!(row.get::<String, _>(2), "POST /api/session");
        assert_eq!(row.get::<String, _>(3), "/api/session");
        assert_eq!(row.get::<Option<String>, _>(4).as_deref(), Some("req-1"));
        assert_eq!(row.get::<Option<i64>, _>(5), Some(201));
        assert!(row.get::<bool, _>(6));
    }

    #[tokio::test]
    async fn scoped_transactions_attribute_changes_and_never_commit_the_context() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| unreachable!("{error}"));
        let pool = pool(dir.path()).await;
        let op = start(&pool, None, "POST /api/players", "/api/players", None)
            .await
            .unwrap_or_else(|error| unreachable!("{error}"));
        scope(op, async {
            assert_eq!(current_operation(), Some(op));
            let mut tx = db::begin(&pool)
                .await
                .unwrap_or_else(|error| unreachable!("{error}"));
            exec(
                &mut tx,
                "INSERT INTO players (name, inserted_at, updated_at) VALUES ('Ada', 'x', 'x')",
            )
            .await;
            // A savepoint keeps the outer transaction's operation.
            {
                use sqlx::Connection as _;
                let mut nested = tx
                    .begin()
                    .await
                    .unwrap_or_else(|error| unreachable!("{error}"));
                exec(
                    &mut nested,
                    "UPDATE players SET name = 'Grace' WHERE name = 'Ada'",
                )
                .await;
                nested
                    .commit()
                    .await
                    .unwrap_or_else(|error| unreachable!("{error}"));
            }
            tx.commit()
                .await
                .unwrap_or_else(|error| unreachable!("{error}"));
        })
        .await;
        assert_eq!(current_operation(), None);
        assert_eq!(context(&pool).await, None);

        // Outside a scope (background jobs, plain pool writes) changes record no operation.
        let mut tx = db::begin(&pool)
            .await
            .unwrap_or_else(|error| unreachable!("{error}"));
        exec(&mut tx, "UPDATE players SET name = 'Linus'").await;
        tx.commit()
            .await
            .unwrap_or_else(|error| unreachable!("{error}"));
        sqlx::query("UPDATE players SET archived_at = 'y'")
            .execute(&pool)
            .await
            .unwrap_or_else(|error| unreachable!("{error}"));

        let changes = changes(&pool).await;
        let ops: Vec<_> = changes.iter().map(|change| change.operation_id).collect();
        assert_eq!(ops, [Some(op), Some(op), None, None]);
        assert!(changes.iter().all(|change| change.entity == "players"));
        assert_eq!(changes.first().and_then(|c| c.before.clone()), None);
        assert_eq!(
            changes
                .get(1)
                .and_then(|c| c.after.as_ref())
                .and_then(|after| after.get("name").cloned()),
            Some(Value::from("Grace"))
        );
    }

    #[tokio::test]
    async fn rollback_and_drop_discard_changes_and_context() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| unreachable!("{error}"));
        let pool = pool(dir.path()).await;
        let op = start(&pool, None, "POST /api/players", "/api/players", None)
            .await
            .unwrap_or_else(|error| unreachable!("{error}"));
        scope(op, async {
            let mut tx = db::begin(&pool)
                .await
                .unwrap_or_else(|error| unreachable!("{error}"));
            exec(
                &mut tx,
                "INSERT INTO players (name, inserted_at, updated_at) VALUES ('Ada', 'x', 'x')",
            )
            .await;
            tx.rollback()
                .await
                .unwrap_or_else(|error| unreachable!("{error}"));

            let mut tx = db::begin(&pool)
                .await
                .unwrap_or_else(|error| unreachable!("{error}"));
            exec(
                &mut tx,
                "INSERT INTO players (name, inserted_at, updated_at) VALUES ('Bea', 'x', 'x')",
            )
            .await;
            drop(tx);
        })
        .await;
        assert_eq!(context(&pool).await, None);
        assert!(changes(&pool).await.is_empty());
        let players: i64 = sqlx::query_scalar("SELECT count(*) FROM players")
            .fetch_one(&pool)
            .await
            .unwrap_or(-1);
        assert_eq!(players, 0);
    }

    #[tokio::test]
    async fn deleting_a_game_records_cascaded_children() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| unreachable!("{error}"));
        let pool = pool(dir.path()).await;
        let mut conn = pool
            .acquire()
            .await
            .unwrap_or_else(|error| unreachable!("{error}"));
        let player = exec(
            &mut conn,
            "INSERT INTO players (name, inserted_at, updated_at) VALUES ('Ada', 'x', 'x')",
        )
        .await;
        let game = exec(
            &mut conn,
            "INSERT INTO games (played_at, inserted_at, updated_at) VALUES ('2026-10-01', 'x', 'x')",
        )
        .await;
        let seat = exec(
            &mut conn,
            "INSERT INTO game_players (game_id, player_id, seat, result, inserted_at, updated_at)
             SELECT max(games.id), max(players.id), 1, 'win', 'x', 'x' FROM games, players",
        )
        .await;
        exec(
            &mut conn,
            "INSERT INTO sheet_import_receipts (key, game_id) SELECT 'row-1', max(id) FROM games",
        )
        .await;
        drop(conn);

        let op = start(&pool, None, "DELETE /api/games/{id}", "/api/games/1", None)
            .await
            .unwrap_or_else(|error| unreachable!("{error}"));
        scope(op, async {
            let mut tx = db::begin(&pool)
                .await
                .unwrap_or_else(|error| unreachable!("{error}"));
            exec(&mut tx, "DELETE FROM games").await;
            tx.commit()
                .await
                .unwrap_or_else(|error| unreachable!("{error}"));
        })
        .await;

        let deleted: Vec<_> = changes(&pool)
            .await
            .into_iter()
            .filter(|change| change.operation_id == Some(op))
            .collect();
        let mut entities: Vec<_> = deleted
            .iter()
            .map(|change| (change.entity.as_str(), change.entity_id.clone()))
            .collect();
        entities.sort();
        assert_eq!(
            entities,
            [
                ("game_players", seat.to_string()),
                ("games", game.to_string()),
                ("sheet_import_receipts", "row-1".to_string()),
            ]
        );
        assert!(deleted.iter().all(|change| change.after.is_none()));
        let seat_before = deleted
            .iter()
            .find(|change| change.entity == "game_players")
            .and_then(|change| change.before.clone())
            .unwrap_or_default();
        assert_eq!(seat_before.get("game_id"), Some(&Value::from(game)));
        assert_eq!(seat_before.get("player_id"), Some(&Value::from(player)));
        assert_eq!(seat_before.get("result"), Some(&Value::from("win")));
    }

    #[tokio::test]
    async fn snapshots_exclude_credentials_and_skip_no_op_updates() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| unreachable!("{error}"));
        let pool = pool(dir.path()).await;
        let mut conn = pool
            .acquire()
            .await
            .unwrap_or_else(|error| unreachable!("{error}"));
        let user = insert_user(&mut conn).await;
        // No recorded column changes: nothing is logged.
        exec(
            &mut conn,
            "UPDATE users SET updated_at = 'later', display_name = display_name",
        )
        .await;
        exec(
            &mut conn,
            "UPDATE users SET hashed_password = '$2b$12$otherhash'",
        )
        .await;
        // A recorded change: logged, still without credentials.
        exec(
            &mut conn,
            "UPDATE users SET manavault_api_key = NULL, display_name = 'Ada L'",
        )
        .await;
        exec(
            &mut conn,
            "INSERT INTO api_keys (user_id, name, token_hash, prefix, inserted_at)
             SELECT max(id), 'CI', X'DEADBEEF', 'tg_abcd', 'x' FROM users",
        )
        .await;
        exec(&mut conn, "UPDATE api_keys SET last_used_at = 'now'").await;
        // The settings row exists from the migrations; only `registration_invite_set` shows.
        exec(
            &mut conn,
            "UPDATE server_settings SET registration_invite_hash = X'CAFE'",
        )
        .await;
        // Replacing the invite with another one changes nothing recorded.
        exec(
            &mut conn,
            "INSERT INTO server_settings (id, registration_enabled, registration_invite_hash, inserted_at, updated_at)
             VALUES (1, 1, X'CAFE', 'x', 'x')
             ON CONFLICT (id) DO UPDATE SET registration_invite_hash = X'BEEF'",
        )
        .await;
        exec(
            &mut conn,
            "INSERT INTO pending_discord_games (external_id, guild_id, channel_id, played_at, players, raw, inserted_at, updated_at)
             VALUES ('e1', 'g', 'c', 'x', '[\"1\",\"2\"]', '{\"token\":\"discord-raw-secret\"}', 'x', 'x')",
        )
        .await;
        exec(
            &mut conn,
            "INSERT INTO discord_result_drafts (id, pending_game_id, discord_id, guild_id, channel_id, snapshot, data, expires_at)
             SELECT 'draft-1', max(id), 'd', 'g', 'c', X'534E415053484F54', '{\"winner\":\"1\"}', 'x' FROM pending_discord_games",
        )
        .await;
        exec(&mut conn, "DELETE FROM api_keys").await;
        drop(conn);

        let changes = changes(&pool).await;
        let summary: Vec<_> = changes
            .iter()
            .map(|change| {
                (
                    change.entity.as_str(),
                    change.before.is_some(),
                    change.after.is_some(),
                )
            })
            .collect();
        assert_eq!(
            summary,
            [
                ("users", false, true),
                ("users", true, true),
                ("api_keys", false, true),
                ("server_settings", true, true),
                ("pending_discord_games", false, true),
                ("discord_result_drafts", false, true),
                ("api_keys", true, false),
            ]
        );

        let dump = sqlx::query_scalar::<_, String>(
            "SELECT group_concat(coalesce(before_json, '') || coalesce(after_json, ''), '') FROM audit_changes",
        )
        .fetch_one(&pool)
        .await
        .unwrap_or_default();
        for secret in [
            "hashed_password",
            "secrethash",
            "otherhash",
            "sealed-manavault-key",
            "token_hash",
            "DEADBEEF",
            "registration_invite_hash",
            "CAFE",
            "BEEF",
            "discord-raw-secret",
            "snapshot",
            "SNAPSHOT",
        ] {
            assert!(!dump.contains(secret), "{secret} leaked into {dump}");
        }

        let created = changes
            .first()
            .and_then(|c| c.after.clone())
            .unwrap_or_default();
        assert_eq!(created.get("id"), Some(&Value::from(user)));
        assert_eq!(created.get("password_set"), Some(&Value::Bool(true)));
        assert_eq!(
            created.get("manavault_api_key_set"),
            Some(&Value::Bool(true))
        );
        let updated = changes
            .get(1)
            .and_then(|c| c.after.clone())
            .unwrap_or_default();
        assert_eq!(
            updated.get("manavault_api_key_set"),
            Some(&Value::Bool(false))
        );
        assert_eq!(updated.get("display_name"), Some(&Value::from("Ada L")));
        let settings = changes
            .get(3)
            .and_then(|c| c.after.clone())
            .unwrap_or_default();
        assert_eq!(
            settings.get("registration_invite_set"),
            Some(&Value::Bool(true))
        );
        let pending = changes
            .get(4)
            .and_then(|c| c.after.clone())
            .unwrap_or_default();
        assert_eq!(pending.get("players"), Some(&serde_json::json!(["1", "2"])));
        let draft = changes
            .get(5)
            .and_then(|c| c.after.clone())
            .unwrap_or_default();
        assert_eq!(draft.get("data"), Some(&serde_json::json!({"winner": "1"})));
        let key = changes
            .get(2)
            .and_then(|c| c.after.clone())
            .unwrap_or_default();
        assert_eq!(key.get("prefix"), Some(&Value::from("tg_abcd")));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_scopes_keep_their_own_operations() {
        let dir = tempfile::tempdir().unwrap_or_else(|error| unreachable!("{error}"));
        let pool = pool(dir.path()).await;
        let mut tasks = Vec::new();
        for task in 0..8_i64 {
            let pool = pool.clone();
            let op = start(&pool, None, "POST /api/players", "/api/players", None)
                .await
                .unwrap_or_else(|error| unreachable!("{error}"));
            tasks.push(tokio::spawn(async move {
                let scoped = async {
                    for round in 0..5_i64 {
                        let mut tx = db::begin(&pool)
                            .await
                            .unwrap_or_else(|error| unreachable!("{error}"));
                        sqlx::query(
                            "INSERT INTO players (name, inserted_at, updated_at) VALUES (?, 'x', 'x')",
                        )
                        .bind(format!("p-{op}-{round}"))
                        .execute(&mut *tx)
                        .await
                        .unwrap_or_else(|error| unreachable!("{error}"));
                        tokio::task::yield_now().await;
                        tx.commit()
                            .await
                            .unwrap_or_else(|error| unreachable!("{error}"));
                    }
                };
                if task % 2 == 0 {
                    // Unscoped writers run alongside and must never borrow an operation.
                    scoped.await;
                    (None, op)
                } else {
                    scope(op, scoped).await;
                    (Some(op), op)
                }
            }));
        }
        let mut expected = Vec::new();
        for task in tasks {
            expected.push(task.await.unwrap_or_else(|error| unreachable!("{error}")));
        }

        let changes = changes(&pool).await;
        assert_eq!(changes.len(), 40);
        for (scoped, op) in expected {
            let prefix = format!("\"p-{op}-");
            let mine: Vec<_> = changes
                .iter()
                .filter(|change| {
                    change
                        .after
                        .as_ref()
                        .is_some_and(|after| after.to_string().contains(&prefix))
                })
                .collect();
            assert_eq!(mine.len(), 5);
            for change in mine {
                assert_eq!(change.operation_id, scoped, "player of op {op}");
            }
        }
        assert_eq!(context(&pool).await, None);
    }
}
