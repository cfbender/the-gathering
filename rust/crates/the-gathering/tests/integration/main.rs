//! Every integration test in one binary, so an edit links the server into one test
//! executable instead of one per file. Each module is a former `tests/<name>.rs`.

// Test code may unwrap, index, and panic; clippy.toml only exempts `#[test]` bodies, and the
// helpers in `support` and `webcam_support` are outside them.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::assert_is_empty
)]

mod accounts;
mod admin_players_api;
mod admin_software_update_api;
mod admin_users_api;
mod api_key_api;
mod audit_api;
mod auth_api;
mod card_api;
mod cardid_api;
mod catalog;
mod decklists;
mod decks_api;
mod dev_auto_login;
mod discord_api;
mod discord_auth;
mod discord_new_game;
mod discord_parsing;
mod discord_summary;
mod discord_summary_logs;
mod discord_tracker;
mod discord_won;
mod games;
mod games_api;
mod imports;
mod imports_api;
mod players_api;
mod rate_limit;
mod registration_invite_api;
mod seed;
mod self_update;
mod static_assets;
mod stats;
mod stats_api;
mod support;
mod v1_games_api;
mod web_foundation;
mod webcam_api;
mod webcam_game_modes;
mod webcam_room_lifecycle;
mod webcam_support;
mod webcam_table_connection;
mod webcam_table_state;
mod webcam_tables_pure;
