//! Players, decks, and games.
//!
//! [`Games`] is the pool-level API (each call runs in its own transaction when it writes).
//! The submodules expose the same operations on a `&mut SqliteConnection`, so callers that
//! already hold a transaction (imports, the Discord bot) can compose them; steps that must
//! succeed or fail together open a savepoint.

pub mod color_identity;
pub mod deck;
pub mod deck_picker;
pub mod game;
pub mod input;
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
pub mod sync_remote_decks;
pub mod win_condition;

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, OnceLock};

use serde::Serialize;
use sqlx::SqliteConnection;

use crate::accounts::User;
use crate::db::{self, Pool, UtcDateTime};
use crate::error::ApiError;
use crate::validation::ValidationError;

pub use self::deck_picker::{Candidate, DeckPick, Outcome};
pub use self::input::{DeckInput, GameInput, PlayerInput, SeatInput};
pub use self::list_games::GameFilters;
pub use self::merge_players::LinkPlan;
pub use self::model::{
    Deck, DeckLinks, DecklistSource, Game, GameFormat, GameResult, GameSource, Player, Seat,
    get_deck, get_player, load_game, load_games,
};
pub use self::player::{IdentityQuery, PlayerDetail, PlayerIdentityRow, SeatGame};
pub use self::resolve_player::{PlayerIdentity, Resolution, ResolveError};
pub use self::summary_image::{ArtFetcher, RenderError};
pub use self::win_condition::WinCondition;

/// Case-folds a player or deck name the way SQLite compares them.
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
    Invalid(ValidationError),
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

impl From<ValidationError> for GamesError {
    fn from(errors: ValidationError) -> Self {
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
        let total_pages = if per_page > 0 {
            total.saturating_add(per_page - 1) / per_page
        } else {
            1
        };
        Self {
            page,
            per_page,
            total,
            total_pages: total_pages.max(1),
        }
    }
}

pub(crate) async fn user_exists(
    conn: &mut SqliteConnection,
    user_id: i64,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS(SELECT 1 FROM users WHERE id = ?) AS "e!: bool""#,
        user_id
    )
    .fetch_one(&mut *conn)
    .await
}

/// Administrators manage everyone; members manage their own
/// linked player and unclaimed guests.
pub fn can_manage_player(user: &User, player: &Player) -> bool {
    user.is_admin() || player.user_id.is_none_or(|owner| owner == user.id)
}

/// Asks whether a player holds a seat at an open webcam table.
pub type SeatedCheck = Arc<dyn Fn(i64) -> Pin<Box<dyn Future<Output = bool> + Send>> + Send + Sync>;

/// The pool-level games API (`state.games`).
#[derive(Clone)]
pub struct Games {
    /// Database.
    pub pool: Pool,
    seated: Arc<OnceLock<SeatedCheck>>,
    links: DeckLinks,
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
    pub fn new(pool: Pool, links: DeckLinks) -> Self {
        Self {
            pool,
            seated: Arc::new(OnceLock::new()),
            links,
        }
    }

    /// How this server labels deck-list URLs (`decks.decklist_source`).
    pub fn deck_links(&self) -> &DeckLinks {
        &self.links
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

    /// Every player (archived ones on request), avatars included.
    pub async fn list_players(&self, include_archived: bool) -> Result<Vec<Player>, sqlx::Error> {
        player::list_players(&mut *self.pool.acquire().await?, include_archived).await
    }

    /// A player by id (no avatar).
    pub async fn get_player(&self, id: i64) -> Result<Option<Player>, sqlx::Error> {
        model::get_player(&mut *self.pool.acquire().await?, id).await
    }

    /// The player page.
    pub async fn get_player_detail(&self, id: i64) -> Result<Option<PlayerDetail>, sqlx::Error> {
        player::get_player_detail(&mut *self.pool.acquire().await?, id).await
    }

    /// The player linked to an account, if any.
    pub async fn get_player_for_user(&self, user_id: i64) -> Result<Option<Player>, sqlx::Error> {
        player::get_player_for_user(&mut *self.pool.acquire().await?, user_id).await
    }

    /// One page of the administrator's player identity list (`page`, `per_page`, `search`).
    pub async fn list_player_identities(
        &self,
        query: &IdentityQuery,
    ) -> Result<(Vec<PlayerIdentityRow>, Pagination), sqlx::Error> {
        player::list_player_identities(&mut *self.pool.acquire().await?, query).await
    }

    /// Clears a player's linked identity.
    pub async fn unlink_player_identity(&self, player: &Player) -> Result<Player, sqlx::Error> {
        write_tx!(self, |conn| player::unlink_player_identity(conn, player)
            .await)
    }

    /// Creates a player, optionally linked to an account.
    pub async fn create_player(
        &self,
        input: &PlayerInput,
        user_id: Option<i64>,
    ) -> Result<Player, GamesError> {
        write_tx!(self, |conn| player::create_player(conn, input, user_id)
            .await)
    }

    /// Updates a player.
    pub async fn update_player(
        &self,
        player: &Player,
        input: &PlayerInput,
    ) -> Result<Player, GamesError> {
        write_tx!(self, |conn| player::update_player(conn, player, input)
            .await)
    }

    /// Deletes a player that has no decks or seats.
    pub async fn delete_player(&self, player: &Player) -> Result<(), GamesError> {
        write_tx!(self, |conn| player::delete_player(conn, player).await)
    }

    /// Finds or creates the player for a name, Discord id, and account, in one transaction.
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

    /// How each `(name, discord_id)` identity would resolve, without writing.
    pub async fn preview_player_resolutions(
        &self,
        identities: &[(String, Option<String>)],
    ) -> Result<Vec<Resolution>, sqlx::Error> {
        resolve_player::preview(&mut *self.pool.acquire().await?, identities).await
    }

    /// The player named `name`, created from `input` if missing.
    pub async fn find_or_create_player_by_name(
        &self,
        name: &str,
        input: &PlayerInput,
    ) -> Result<Player, GamesError> {
        write_tx!(self, |conn| player::find_or_create_player_by_name(
            conn, name, input
        )
        .await)
    }

    /// Refuses a source seated at an open webcam table.
    pub async fn merge_players(
        &self,
        source: &Player,
        target: &Player,
    ) -> Result<Player, GamesError> {
        if source.id == target.id {
            return Err(GamesError::BadRequest);
        }
        if self.seated(source.id).await {
            return Err(merge_players::seated_error(source));
        }
        write_tx!(self, |conn| merge_players::merge_unseated(
            conn, source, target
        )
        .await)
    }

    /// Makes `player` the account's player, merging the
    /// account's current player into it.
    pub async fn link_player_to_user(
        &self,
        player: &Player,
        user: &User,
    ) -> Result<Player, GamesError> {
        let plan = write_tx!(self, |conn| merge_players::plan_link(conn, player, user)
            .await)?;
        match plan {
            LinkPlan::Done(player) => Ok(player),
            LinkPlan::Merge { current, player } => self.merge_players(&current, &player).await,
        }
    }

    // Decks

    /// Decks (archived ones on request, optionally one player's) with each deck's player.
    pub async fn list_decks(
        &self,
        include_archived: bool,
        player_id: Option<i64>,
    ) -> Result<Vec<(Deck, Player)>, sqlx::Error> {
        deck::list_decks(
            &mut *self.pool.acquire().await?,
            include_archived,
            player_id,
        )
        .await
    }

    /// A deck by id.
    pub async fn get_deck(&self, id: i64) -> Result<Option<Deck>, sqlx::Error> {
        model::get_deck(&mut *self.pool.acquire().await?, id).await
    }

    /// Creates a deck.
    pub async fn create_deck(&self, input: &DeckInput) -> Result<Deck, GamesError> {
        write_tx!(self, |conn| deck::create_deck(conn, &self.links, input)
            .await)
    }

    /// Updates a deck.
    pub async fn update_deck(&self, deck: &Deck, input: &DeckInput) -> Result<Deck, GamesError> {
        write_tx!(self, |conn| deck::update_deck(
            conn,
            &self.links,
            deck,
            input
        )
        .await)
    }

    /// Deletes a deck; its seats move to `replacement` or lose their deck.
    pub async fn delete_deck(
        &self,
        deck: &Deck,
        replacement: Option<&Deck>,
    ) -> Result<Deck, GamesError> {
        write_tx!(self, |conn| deck::delete_deck(conn, deck, replacement)
            .await)
    }

    /// A player's deck matching the name and commanders.
    pub async fn find_deck(
        &self,
        player_id: i64,
        name: &str,
        commander_name: Option<&str>,
        partner_name: Option<&str>,
    ) -> Result<Option<Deck>, sqlx::Error> {
        deck::find_deck(
            &mut *self.pool.acquire().await?,
            player_id,
            name,
            commander_name,
            partner_name,
        )
        .await
    }

    /// A player's deck named `name`, created from `input` if missing.
    pub async fn find_or_create_deck(
        &self,
        player_id: i64,
        name: &str,
        input: &DeckInput,
    ) -> Result<Deck, GamesError> {
        write_tx!(self, |conn| deck::find_or_create_deck(
            conn,
            &self.links,
            player_id,
            name,
            input
        )
        .await)
    }

    /// Administrators, or the member linked to the deck's player.
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

    /// Picks one of the member's decks at random, weighted toward decks played least lately.
    pub async fn pick_deck(
        &self,
        user: &User,
        exclude_id: Option<i64>,
        random: f64,
    ) -> Result<DeckPick, sqlx::Error> {
        deck_picker::random_deck(
            &mut *self.pool.acquire().await?,
            user.id,
            exclude_id,
            UtcDateTime::now(),
            random,
        )
        .await
    }

    /// Records whether the member played or skipped a picked deck.
    pub async fn record_deck_outcome(
        &self,
        user: &User,
        deck_id: i64,
        outcome: Outcome,
    ) -> Result<Deck, GamesError> {
        write_tx!(self, |conn| deck_picker::record_outcome(
            conn, user.id, deck_id, outcome
        )
        .await)
    }

    // Games

    /// One page of games matching the filters.
    pub async fn list_games(
        &self,
        opts: &GameFilters,
    ) -> Result<(Vec<Game>, Pagination), sqlx::Error> {
        list_games::list_games(&mut *self.pool.acquire().await?, opts).await
    }

    /// A game by id, with seats, players, and decks loaded.
    pub async fn get_game(&self, id: i64) -> Result<Option<Game>, sqlx::Error> {
        model::load_game(&mut *self.pool.acquire().await?, id).await
    }

    /// The game a summary request refers to.
    pub async fn find_summary_game(&self, reference: &str) -> Result<Game, GamesError> {
        summary::find(&mut *self.pool.acquire().await?, reference).await
    }

    /// Administrators, the creator, or a member whose linked
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

    /// Records a new game with its seats.
    pub async fn create_game(
        &self,
        input: &GameInput,
        created_by_user_id: Option<i64>,
    ) -> Result<Game, GamesError> {
        write_tx!(self, |conn| record_game::create(
            conn,
            input,
            created_by_user_id
        )
        .await)
    }

    /// The game with this `(source, external_id)`, recorded from `input` if missing.
    pub async fn find_or_create_game_by_external_id(
        &self,
        source: &str,
        external_id: &str,
        input: &GameInput,
    ) -> Result<Game, GamesError> {
        write_tx!(self, |conn| record_game::find_or_create_by_external_id(
            conn,
            source,
            external_id,
            input
        )
        .await)
    }

    /// Records or replaces the game with this `(source, external_id)`.
    pub async fn upsert_game_by_external_id(
        &self,
        source: &str,
        external_id: &str,
        input: &GameInput,
    ) -> Result<Game, GamesError> {
        write_tx!(self, |conn| record_game::upsert_by_external_id(
            conn,
            source,
            external_id,
            input
        )
        .await)
    }

    /// Updates a game and its seats.
    pub async fn update_game(&self, game: &Game, input: &GameInput) -> Result<Game, GamesError> {
        write_tx!(self, |conn| record_game::update(conn, game, input).await)
    }

    /// Deletes a game (seats cascade).
    pub async fn delete_game(&self, game: &Game) -> Result<(), sqlx::Error> {
        write_tx!(self, |conn| record_game::delete(conn, game.id).await)
    }

    /// Links a game's unlinked decks and its seats' MVP cards to catalog cards.
    pub async fn link_catalog_cards(
        &self,
        game_id: i64,
    ) -> Result<link_catalog_cards::LinkResult, GamesError> {
        write_tx!(self, |conn| link_catalog_cards::link_game(
            conn,
            &self.links,
            game_id
        )
        .await)
    }

    /// Links the next batch of unlinked decks and seats after `cursor` to catalog cards.
    pub async fn repair_catalog_links(
        &self,
        cursor: link_catalog_cards::Cursor,
        limit: Option<i64>,
    ) -> Result<link_catalog_cards::BatchResult, GamesError> {
        write_tx!(self, |conn| link_catalog_cards::repair_batch(
            conn,
            &self.links,
            cursor,
            limit
        )
        .await)
    }
}

/// The `summary_images` budget: 30 renders a minute across the server.
pub const SUMMARY_IMAGES_LIMIT: crate::config::WindowLimit = crate::config::WindowLimit {
    limit: 30,
    scale: crate::rate_limit::MINUTE,
};

/// The summary card PNG, within the `summary_images` budget.
pub async fn render_summary(
    state: &crate::state::AppState,
    game: &Game,
) -> Result<Vec<u8>, RenderError> {
    match state
        .rate_limiter
        .hit("summary_images", SUMMARY_IMAGES_LIMIT)
    {
        crate::rate_limit::Decision::Deny(_) => Err(RenderError::RateLimited),
        crate::rate_limit::Decision::Allow(_) => {
            summary_image::render(&state.pool, &ArtFetcher::new(state.http.clone()), game).await
        }
    }
}
