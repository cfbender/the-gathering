//! The HTTP layer (`TheGatheringWeb.Router` and its pipelines).
//!
//! Route groups mirror the Phoenix router's `pipe_through` lists. Every `/api` route is
//! session-authenticated except `/api/v1`, which uses personal API keys.

pub mod api;
pub mod auth;
pub mod channels;
pub mod params;
pub mod session;
pub mod shell;
pub mod statics;

use axum::Router;
use axum::middleware::{from_fn, from_fn_with_state};
use axum::routing::{any, delete, get, patch, post, put};

use crate::error::ApiError;
use crate::state::AppState;

use self::api::{accounts, admin, cardid, cards, discord, games, imports, stats, webcam};
use self::auth::{require_admin, require_authenticated_user, require_sudo_mode};

/// The rate-limit bucket guarding a route group.
#[derive(Clone, Copy, Debug)]
pub enum Bucket {
    /// Password login and bootstrap registration, per address.
    Credentials,
    /// Password sudo, per user and address plus a global budget.
    Sudo,
    /// Personal API keys, per owner.
    ApiKeys,
}

async fn not_found() -> ApiError {
    ApiError::NotFound
}

/// The JSON 404 for API paths that match no route.
pub async fn api_not_found() -> ApiError {
    ApiError::NotFound
}

/// Builds the whole application router.
pub fn router(state: AppState) -> Router {
    // pipe_through :api
    let public = Router::new()
        .route("/api/health", get(accounts::health))
        .route("/api/registration", get(accounts::registration_show))
        .route(
            "/api/registration-invite",
            get(accounts::invite_show).post(accounts::invite_create),
        )
        .route(
            "/api/session",
            get(accounts::session_show).delete(accounts::session_delete),
        )
        .route("/api/cardid/corrections", get(cardid::corrections_index))
        .route(
            "/api/cardid/corrections/{id}/crop",
            get(cardid::corrections_crop),
        );

    // pipe_through [:api, :rate_limit_credentials]
    let credentials = Router::new()
        .route("/api/users", post(accounts::registration_create))
        .route("/api/session", post(accounts::session_create))
        .route_layer(from_fn_with_state(
            state.clone(),
            api::rate_limit_credentials,
        ));

    // pipe_through [:api, :require_authenticated_user]
    let members = Router::new()
        .route("/api/session/user", patch(accounts::update_profile))
        .route(
            "/api/session/appearance",
            patch(accounts::update_appearance),
        )
        .route("/api/session/remote-decks", get(games::remote_decks_index))
        .route(
            "/api/session/remote-decks/sync",
            post(games::remote_decks_sync),
        )
        .route(
            "/api/session/api-keys",
            get(accounts::api_keys_index).post(accounts::api_keys_create),
        )
        .route(
            "/api/session/api-keys/{id}",
            delete(accounts::api_keys_delete),
        )
        .route("/api/cards", get(cards::cards_index))
        .route("/api/cards/{id}", get(cards::cards_show))
        .route("/api/card-printings", get(cards::printings_index))
        .route("/api/card-printings/{id}", get(cards::printings_show))
        .route(
            "/api/card-printings/{id}/details",
            get(cards::printings_details),
        )
        .route(
            "/api/card-printings/{id}/rulings",
            get(cards::printings_rulings),
        )
        .route("/api/catalog", get(cards::catalog_show))
        .route("/api/stats/overview", get(stats::overview))
        .route("/api/stats/players/{id}", get(stats::player))
        .route("/api/stats/decks/{id}", get(stats::deck))
        .route("/api/stats/commanders", get(stats::commanders))
        .route("/api/stats/commanders/{id}", get(stats::commander))
        .route("/api/deck-chooser", get(games::deck_chooser_show))
        .route(
            "/api/deck-chooser/{id}/outcomes",
            post(games::deck_chooser_outcome),
        )
        .route("/api/webcam-table/config", get(webcam::config_show))
        .route("/api/webcam-table/rooms", get(webcam::rooms_index))
        .route("/api/cardid/bundle", get(cardid::bundle_show))
        .route(
            "/api/cardid/bundles/{version}/{name}",
            get(cardid::bundle_file),
        )
        .route("/api/cardid/corrections", post(cardid::corrections_create))
        .route(
            "/api/discord/result-drafts/{id}",
            get(discord::result_draft_show).post(discord::result_draft_create),
        )
        .route(
            "/api/players",
            get(games::players_index).post(games::players_create),
        )
        .route(
            "/api/players/{id}",
            get(games::players_show)
                .patch(games::players_update)
                .put(games::players_update)
                .delete(games::players_delete),
        )
        .route(
            "/api/decks",
            get(games::decks_index).post(games::decks_create),
        )
        .route(
            "/api/decks/{id}",
            get(games::decks_show)
                .patch(games::decks_update)
                .put(games::decks_update)
                .delete(games::decks_delete),
        )
        .route("/api/decks/{id}/decklist", get(games::decklist_show))
        .route("/api/games/{id}/summary", get(games::games_summary))
        .route(
            "/api/games",
            get(games::games_index).post(games::games_create),
        )
        .route(
            "/api/games/{id}",
            get(games::games_show)
                .patch(games::games_update)
                .put(games::games_update)
                .delete(games::games_delete),
        )
        .route("/api/decklists/resolve", post(games::decklist_resolve))
        .route_layer(from_fn(require_authenticated_user));

    // pipe_through [:api, :require_authenticated_user, :rate_limit_sudo]
    let sudo = Router::new()
        .route("/api/session/sudo", post(accounts::session_sudo))
        .route_layer(from_fn_with_state(state.clone(), api::rate_limit_sudo))
        .route_layer(from_fn(require_authenticated_user));

    // pipe_through [:api, :require_authenticated_user, :require_admin]
    let admins = Router::new()
        .route("/api/imports/csv/sample", get(imports::csv_sample))
        .route("/api/imports/csv/preview", post(imports::csv_preview))
        .route(
            "/api/imports/mythic_track/preview",
            post(imports::mythic_track_preview),
        )
        .route("/api/imports/sheet/preview", post(imports::sheet_preview))
        .route("/api/exports/portable", get(imports::portable_export))
        .route(
            "/api/imports/portable/preview",
            post(imports::portable_preview),
        )
        .route_layer(from_fn(require_admin))
        .route_layer(from_fn(require_authenticated_user));

    // pipe_through [:api, :require_authenticated_user, :require_admin, :require_sudo_mode]
    let sudo_admins = Router::new()
        .route("/api/imports/csv", post(imports::csv_create))
        .route(
            "/api/imports/mythic_track",
            post(imports::mythic_track_create),
        )
        .route("/api/imports/sheet", post(imports::sheet_create))
        .route("/api/imports/portable", post(imports::portable_create))
        .route("/api/players/{id}/merge", post(games::players_merge))
        .route("/api/admin/users", get(accounts::admin_users_index))
        .route(
            "/api/admin/users/{id}",
            patch(accounts::admin_users_update)
                .put(accounts::admin_users_update)
                .delete(accounts::admin_users_delete),
        )
        .route(
            "/api/admin/users/{id}/sessions",
            delete(accounts::admin_users_revoke_sessions),
        )
        .route(
            "/api/admin/users/{id}/player",
            put(games::admin_link_player),
        )
        .route("/api/admin/players", get(games::admin_players_index))
        .route(
            "/api/admin/players/{id}/identity",
            delete(games::admin_players_unlink),
        )
        .route(
            "/api/admin/settings",
            get(accounts::admin_settings_show).patch(accounts::admin_settings_update),
        )
        .route(
            "/api/admin/registration-invite",
            get(accounts::admin_invite_show).post(accounts::admin_invite_create),
        )
        .route(
            "/api/admin/software-update",
            get(admin::software_update_show).post(admin::software_update_create),
        )
        .route("/api/admin/discord/pending", get(discord::pending_index))
        .route(
            "/api/admin/discord/pending/{id}",
            patch(discord::pending_update).delete(discord::pending_delete),
        )
        .route("/api/admin/catalog/sync", post(cards::catalog_sync))
        .route("/api/admin/catalog/backfill", post(cards::catalog_backfill))
        .route_layer(from_fn_with_state(state.clone(), require_sudo_mode))
        .route_layer(from_fn(require_admin))
        .route_layer(from_fn(require_authenticated_user));

    // pipe_through [:api, :require_authenticated_user, :require_sudo_mode]
    let sudo_members = Router::new()
        .route("/api/session/password", patch(accounts::update_password))
        .route_layer(from_fn_with_state(state.clone(), require_sudo_mode))
        .route_layer(from_fn(require_authenticated_user));

    // The SPA and browser-facing redirects (pipe_through :browser).
    let browser = Router::new()
        .route("/auth/discord", get(accounts::discord_request))
        .route("/auth/discord/callback", get(accounts::discord_callback))
        .route(
            "/table/{*path}",
            get(shell::index).route_layer(from_fn(shell::cross_origin_isolation)),
        )
        .route(
            "/table",
            get(shell::index).route_layer(from_fn(shell::cross_origin_isolation)),
        )
        .route("/", get(shell::index))
        .route("/{*path}", get(shell::index))
        .route_layer(from_fn(shell::secure_browser_headers));

    // `<img>` requests negotiate images, not JSON; same session authentication.
    let card_images = Router::new()
        .route("/api/card-images", get(cards::card_image))
        .route_layer(from_fn(require_authenticated_user));

    let sessioned = Router::new()
        .merge(public)
        .merge(credentials)
        .merge(members)
        .merge(sudo)
        .merge(admins)
        .merge(sudo_admins)
        .merge(sudo_members)
        .merge(card_images)
        .route("/api", any(api_not_found))
        .route("/api/{*path}", any(api_not_found))
        .merge(browser)
        .method_not_allowed_fallback(not_found)
        .layer(from_fn_with_state(state.clone(), auth::current_user_layer))
        .layer(from_fn(session::csrf_layer))
        .layer(from_fn_with_state(state.clone(), session::session_layer));

    // Read-only, versioned API for personal API keys (pipe_through :api_key).
    let v1 = Router::new()
        .route("/api/v1/games", get(games::v1_games_index))
        .route_layer(from_fn_with_state(state.clone(), api::rate_limit_api_keys))
        .route_layer(from_fn_with_state(state.clone(), auth::api_key_layer));

    Router::new()
        .merge(v1)
        .route("/socket/websocket", get(webcam::socket))
        .merge(statics::router(&state))
        .merge(sessioned)
        .with_state(state)
}
