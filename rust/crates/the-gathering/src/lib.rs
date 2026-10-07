//! The Gathering's server, ported from the Elixir/Phoenix application: the JSON API,
//! realtime webcam tables, the Discord bot, and background jobs, on the same SQLite schema.

pub mod accounts;
pub mod card_id;
pub mod catalog;
pub mod changeset;
pub mod cloudflare_turn;
pub mod config;
pub mod crypto;
pub mod db;
pub mod decklists;
pub mod error;
pub mod games;
pub mod local_time;
pub mod rate_limit;
pub mod regex;
pub mod self_update;
pub mod state;
pub mod stats;
pub mod web;
pub mod webcam;

/// `TheGathering.Release.bootstrap_admin/0`: creates the administrator named by
/// `THE_GATHERING_ADMIN_USERNAME`/`THE_GATHERING_ADMIN_PASSWORD` unless it exists.
pub async fn bootstrap_admin(state: &state::AppState) -> anyhow::Result<()> {
    let username = std::env::var("THE_GATHERING_ADMIN_USERNAME")?;
    let password = std::env::var("THE_GATHERING_ADMIN_PASSWORD")?;
    match state.accounts.get_user_by_username(&username).await? {
        Some(user) if user.is_admin() => Ok(()),
        Some(_) => anyhow::bail!("bootstrap username already belongs to a non-admin account"),
        None => state
            .accounts
            .create_admin(&username, &password)
            .await
            .map(|_| ())
            .map_err(|error| anyhow::anyhow!("could not create bootstrap admin: {error:?}")),
    }
}
