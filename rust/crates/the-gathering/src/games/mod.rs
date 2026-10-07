//! Players, decks, and games (`TheGathering.Games`).
//!
//! [`Games`] is the pool-level API (each call runs in its own transaction when it writes).
//! The submodules expose the same operations on a `&mut SqliteConnection`, so callers that
//! already hold a transaction (imports, the Discord bot) can compose them; those open a
//! savepoint where Elixir used `Repo.transaction`.

pub mod color_identity;
pub mod deck;
pub mod deck_picker;
pub mod game;
pub mod link_catalog_cards;
pub mod list_games;
pub mod merge_players;
pub mod model;
pub mod player;
pub mod record_game;
pub mod resolve_player;
pub mod summary;
pub mod summary_card;
pub mod summary_image;
pub mod win_condition;

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, OnceLock};

use serde::Serialize;
use serde_json::Value;
use sqlx::SqliteConnection;

use crate::accounts::User;
use crate::db::{self, Pool, UtcDateTime};
use crate::error::{ApiError, Errors};

pub use self::deck_picker::{Candidate, DeckPick, Outcome};
pub use self::merge_players::LinkPlan;
pub use self::model::{
    Deck, DecklistSource, Game, GameFormat, GameResult, GameSource, Player, Seat, get_deck, get_player, load_game,
    load_games,
};
pub use self::player::{PlayerDetail, PlayerIdentityRow, SeatGame};
pub use self::resolve_player::{PlayerIdentity, Resolution, ResolveError};
pub use self::summary_image::{ArtFetcher, RenderError};
pub use self::win_condition::WinCondition;

/// Case-folds a player or deck name the way SQLite compares them (`Games.fold_name/1`).
///
/// The `players_name_nocase_index` and `decks_player_name_nocase_index` unique indexes use
/// `COLLATE NOCASE`, and `lower()` in queries is ASCII-only, so folding must be ASCII-only too:
/// `Éowyn` stays `Éowyn`.
pub fn fold_name(name: &str) -> String {
    name.trim().to_ascii_lowercase()
}

/// Why a games operation failed.
#[derive(Debug, thiserror::Error)]
pub enum GamesError {
    /// Validation errors (422).
    #[error("invalid")]
    Invalid(Errors),
    /// A bad request (400).
    #[error("bad request")]
    BadRequest,
    /// Not found (404).
    #[error("not found")]
    NotFound,
    /// Database error.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

impl From<Errors> for GamesError {
    fn from(errors: Errors) -> Self {
        Self::Invalid(errors)
    }
}

impl From<GamesError> for ApiError {
    fn from(error: GamesError) -> Self {
        match error {
            GamesError::Invalid(errors) => Self::Validation(errors),
            GamesError::BadRequest => Self::BadRequest,
            GamesError::NotFound => Self::NotFound,
            GamesError::Database(error) => error.into(),
        }
    }
}

/// List pagination: `{page, per_page, total, total_pages}`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Pagination {
    /// 1-based page.
    pub page: i64,
    /// Page size.
    pub per_page: i64,
    /// Matching rows.
    pub total: i64,
    /// At least 1.
    pub total_pages: i64,
}

impl Pagination {
    /// Computes `total_pages` (`max(ceil(total / per_page), 1)`).
    pub fn new(page: i64, per_page: i64, total: i64) -> Self {
        let total_pages = if per_page > 0 { total.saturating_add(per_page - 1) / per_page } else { 1 };
        Self { page, per_page, total, total_pages: total_pages.max(1) }
    }
}

pub(crate) async fn user_exists(conn: &mut SqliteConnection, user_id: i64) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(r#"SELECT EXISTS(SELECT 1 FROM users WHERE id = ?) AS "e!: bool""#, user_id)
        .fetch_one(&mut *conn)
        .await
}

/// `Games.can_manage_player?/2`: administrators manage everyone; members manage their own
/// linked player and unclaimed guests.
pub fn can_manage_player(user: &User, player: &Player) -> bool {
    user.is_admin() || player.user_id.is_none_or(|owner| owner == user.id)
}

/// Asks whether a player holds a seat at an open webcam table (`WebcamTables.seated?/1`).
pub type SeatedCheck = Arc<dyn Fn(i64) -> Pin<Box<dyn Future<Output = bool> + Send>> + Send + Sync>;

/// The pool-level games API (`state.games`).
#[derive(Clone)]
pub struct Games {
    /// Database.
    pub pool: Pool,
    seated: Arc<OnceLock<SeatedCheck>>,
}

impl std::fmt::Debug for Games {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Games").finish_non_exhaustive()
    }
}

/// Runs `$body` (an expression using `conn`) in an immediate write transaction, committing
/// on `Ok`.
macro_rules! write_tx {
    ($self:ident, |$conn:ident| $body:expr) => {{
        let mut tx = db::begin(&$self.pool).await?;
        let $conn: &mut SqliteConnection = &mut tx;
        let result = $body;
        if result.is_ok() {
            tx.commit().await?;
        }
        result
    }};
}

impl Games {
    /// The API over `pool`.
    pub fn new(pool: Pool) -> Self {
        Self { pool, seated: Arc::new(OnceLock::new()) }
    }

    /// Installs the webcam-table seat check merges consult (set once at startup).
    pub fn set_seated_check(&self, check: SeatedCheck) {
        let _ = self.seated.set(check);
    }

    async fn seated(&self, player_id: i64) -> bool {
        match self.seated.get() {
            Some(check) => check(player_id).await,
            None => false,
        }
    }

    // Players

    /// `Games.list_players/1` (avatars included).
    pub async fn list_players(&self, include_archived: bool) -> Result<Vec<Player>, sqlx::Error> {
        player::list_players(&mut *self.pool.acquire().await?, include_archived).await
    }

    /// `Games.get_player/1` (no avatar).
    pub async fn get_player(&self, id: i64) -> Result<Option<Player>, sqlx::Error> {
        model::get_player(&mut *self.pool.acquire().await?, id).await
    }

    /// `Games.get_player!/1`: the player page.
    pub async fn get_player_detail(&self, id: i64) -> Result<Option<PlayerDetail>, sqlx::Error> {
        player::get_player_detail(&mut *self.pool.acquire().await?, id).await
    }

    /// `Games.get_player_for_user/1`.
    pub async fn get_player_for_user(&self, user_id: i64) -> Result<Option<Player>, sqlx::Error> {
        player::get_player_for_user(&mut *self.pool.acquire().await?, user_id).await
    }

    /// `Games.list_player_identities/1` (`page`, `per_page`, `search`).
    pub async fn list_player_identities(&self, params: &Value) -> Result<(Vec<PlayerIdentityRow>, Pagination), sqlx::Error> {
        player::list_player_identities(&mut *self.pool.acquire().await?, params).await
    }

    /// `Games.unlink_player_identity/1`.
    pub async fn unlink_player_identity(&self, player: &Player) -> Result<Player, sqlx::Error> {
        player::unlink_player_identity(&mut *self.pool.acquire().await?, player).await
    }

    /// `Games.create_player/2`.
    pub async fn create_player(&self, attrs: &Value, user_id: Option<i64>) -> Result<Player, GamesError> {
        write_tx!(self, |conn| player::create_player(conn, attrs, user_id).await)
    }

    /// `Games.update_player/2`.
    pub async fn update_player(&self, player: &Player, attrs: &Value) -> Result<Player, GamesError> {
        write_tx!(self, |conn| player::update_player(conn, player, attrs).await)
    }

    /// `Games.delete_player/1`.
    pub async fn delete_player(&self, player: &Player) -> Result<(), GamesError> {
        write_tx!(self, |conn| player::delete_player(conn, player).await)
    }

    /// `Games.resolve_player/3`.
    pub async fn resolve_player(
        &self,
        name: &str,
        discord_id: Option<&str>,
        user_id: Option<i64>,
    ) -> Result<PlayerIdentity, ResolveError> {
        let mut tx = db::begin(&self.pool).await?;
        let result = resolve_player::run(&mut tx, name, discord_id, user_id).await;
        if result.is_ok() {
            tx.commit().await?;
        }
        result
    }

    /// `Games.preview_player_resolutions/1`.
    pub async fn preview_player_resolutions(
        &self,
        identities: &[(String, Option<String>)],
    ) -> Result<Vec<Resolution>, sqlx::Error> {
        resolve_player::preview(&mut *self.pool.acquire().await?, identities).await
    }

    /// `Games.find_or_create_player_by_name/2`.
    pub async fn find_or_create_player_by_name(&self, name: &str, attrs: &Value) -> Result<Player, GamesError> {
        write_tx!(self, |conn| player::find_or_create_player_by_name(conn, name, attrs).await)
    }

    /// `Games.merge_players/2`: refuses a source seated at an open webcam table.
    pub async fn merge_players(&self, source: &Player, target: &Player) -> Result<Player, GamesError> {
        if source.id == target.id {
            return Err(GamesError::BadRequest);
        }
        if self.seated(source.id).await {
            return Err(merge_players::seated_error(source));
        }
        write_tx!(self, |conn| merge_players::merge_unseated(conn, source, target).await)
    }

    /// `Games.link_player_to_user/2`: makes `player` the account's player, merging the
    /// account's current player into it.
    pub async fn link_player_to_user(&self, player: &Player, user: &User) -> Result<Player, GamesError> {
        let plan = write_tx!(self, |conn| merge_players::plan_link(conn, player, user).await)?;
        match plan {
            LinkPlan::Done(player) => Ok(player),
            LinkPlan::Merge { current, player } => self.merge_players(&current, &player).await,
        }
    }

    // Decks

    /// `Games.list_decks/1` with each deck's player.
    pub async fn list_decks(&self, include_archived: bool, player_id: Option<i64>) -> Result<Vec<(Deck, Player)>, sqlx::Error> {
        deck::list_decks(&mut *self.pool.acquire().await?, include_archived, player_id).await
    }

    /// `Games.get_deck/1`.
    pub async fn get_deck(&self, id: i64) -> Result<Option<Deck>, sqlx::Error> {
        model::get_deck(&mut *self.pool.acquire().await?, id).await
    }

    /// `Games.create_deck/1`.
    pub async fn create_deck(&self, attrs: &Value) -> Result<Deck, GamesError> {
        write_tx!(self, |conn| deck::create_deck(conn, attrs).await)
    }

    /// `Games.update_deck/2`.
    pub async fn update_deck(&self, deck: &Deck, attrs: &Value) -> Result<Deck, GamesError> {
        write_tx!(self, |conn| deck::update_deck(conn, deck, attrs).await)
    }

    /// `Games.delete_deck/2`.
    pub async fn delete_deck(&self, deck: &Deck, replacement: Option<&Deck>) -> Result<Deck, GamesError> {
        write_tx!(self, |conn| deck::delete_deck(conn, deck, replacement).await)
    }

    /// `Games.find_deck/4`.
    pub async fn find_deck(
        &self,
        player_id: i64,
        name: &str,
        commander_name: Option<&str>,
        partner_name: Option<&str>,
    ) -> Result<Option<Deck>, sqlx::Error> {
        deck::find_deck(&mut *self.pool.acquire().await?, player_id, name, commander_name, partner_name).await
    }

    /// `Games.find_or_create_deck/3`.
    pub async fn find_or_create_deck(&self, player_id: i64, name: &str, attrs: &Value) -> Result<Deck, GamesError> {
        write_tx!(self, |conn| deck::find_or_create_deck(conn, player_id, name, attrs).await)
    }

    /// `Games.can_manage_deck?/2`: administrators, or the member linked to the deck's player.
    pub async fn can_manage_deck(&self, user: &User, deck: &Deck) -> Result<bool, sqlx::Error> {
        if user.is_admin() {
            return Ok(true);
        }
        sqlx::query_scalar!(
            r#"SELECT EXISTS(SELECT 1 FROM players WHERE id = ? AND user_id = ?) AS "e!: bool""#,
            deck.player_id,
            user.id
        )
        .fetch_one(&self.pool)
        .await
    }

    /// `Games.pick_deck/2`.
    pub async fn pick_deck(&self, user: &User, exclude_id: Option<&Value>, random: f64) -> Result<DeckPick, sqlx::Error> {
        deck_picker::random_deck(&mut *self.pool.acquire().await?, user.id, exclude_id, UtcDateTime::now(), random)
            .await
    }

    /// `Games.record_deck_outcome/3`.
    pub async fn record_deck_outcome(&self, user: &User, deck_id: i64, outcome: Outcome) -> Result<Deck, GamesError> {
        write_tx!(self, |conn| deck_picker::record_outcome(conn, user.id, deck_id, outcome).await)
    }

    // Games

    /// `Games.list_games/1`.
    pub async fn list_games(&self, opts: &Value) -> Result<(Vec<Game>, Pagination), sqlx::Error> {
        list_games::list_games(&mut *self.pool.acquire().await?, opts).await
    }

    /// `Games.get_game!/1` (seats, players, and decks loaded).
    pub async fn get_game(&self, id: i64) -> Result<Option<Game>, sqlx::Error> {
        model::load_game(&mut *self.pool.acquire().await?, id).await
    }

    /// `Games.find_summary_game/1`.
    pub async fn find_summary_game(&self, reference: &str) -> Result<Game, GamesError> {
        summary::find(&mut *self.pool.acquire().await?, reference).await
    }

    /// `Games.can_manage_game?/2`: administrators, the creator, or a member whose linked
    /// player sat in the game.
    pub async fn can_manage_game(&self, user: &User, game: &Game) -> Result<bool, sqlx::Error> {
        if user.is_admin() || game.created_by_user_id == Some(user.id) {
            return Ok(true);
        }
        sqlx::query_scalar!(
            r#"SELECT EXISTS(SELECT 1 FROM game_players s JOIN players p ON p.id = s.player_id
                             WHERE s.game_id = ? AND p.user_id = ?) AS "e!: bool""#,
            game.id,
            user.id
        )
        .fetch_one(&self.pool)
        .await
    }

    /// `Games.create_game/2`.
    pub async fn create_game(&self, attrs: &Value, created_by_user_id: Option<i64>) -> Result<Game, GamesError> {
        write_tx!(self, |conn| record_game::create(conn, attrs, created_by_user_id).await)
    }

    /// `Games.find_or_create_game_by_external_id/3`.
    pub async fn find_or_create_game_by_external_id(
        &self,
        source: &str,
        external_id: &str,
        attrs: &Value,
    ) -> Result<Game, GamesError> {
        write_tx!(self, |conn| record_game::find_or_create_by_external_id(conn, source, external_id, attrs).await)
    }

    /// `Games.upsert_game_by_external_id/3`.
    pub async fn upsert_game_by_external_id(&self, source: &str, external_id: &str, attrs: &Value) -> Result<Game, GamesError> {
        write_tx!(self, |conn| record_game::upsert_by_external_id(conn, source, external_id, attrs).await)
    }

    /// `Games.update_game/2`.
    pub async fn update_game(&self, game: &Game, attrs: &Value) -> Result<Game, GamesError> {
        write_tx!(self, |conn| record_game::update(conn, game, attrs).await)
    }

    /// `Games.delete_game/1`.
    pub async fn delete_game(&self, game: &Game) -> Result<(), sqlx::Error> {
        record_game::delete(&mut *self.pool.acquire().await?, game.id).await
    }

    /// `LinkCatalogCards.link_game/1`.
    pub async fn link_catalog_cards(&self, game_id: i64) -> Result<link_catalog_cards::LinkResult, GamesError> {
        write_tx!(self, |conn| link_catalog_cards::link_game(conn, game_id).await)
    }

    /// `LinkCatalogCards.repair_batch/2`.
    pub async fn repair_catalog_links(
        &self,
        cursor: link_catalog_cards::Cursor,
        limit: Option<i64>,
    ) -> Result<link_catalog_cards::BatchResult, GamesError> {
        write_tx!(self, |conn| link_catalog_cards::repair_batch(conn, cursor, limit).await)
    }
}

/// The `summary_images` budget: 30 renders a minute across the server.
pub const SUMMARY_IMAGES_LIMIT: crate::config::WindowLimit =
    crate::config::WindowLimit { limit: 30, scale: crate::rate_limit::MINUTE };

/// `Games.render_summary/1`: the summary card PNG, within the `summary_images` budget.
pub async fn render_summary(state: &crate::state::AppState, game: &Game) -> Result<Vec<u8>, RenderError> {
    match state.rate_limiter.hit("summary_images", SUMMARY_IMAGES_LIMIT) {
        crate::rate_limit::Decision::Deny(_) => Err(RenderError::RateLimited),
        crate::rate_limit::Decision::Allow(_) => {
            summary_image::render(&state.pool, &ArtFetcher::new(state.http.clone()), game).await
        }
    }
}
