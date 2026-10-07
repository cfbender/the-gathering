//! Player, deck, and game rows (`Games.Player`, `Games.Deck`, `Games.Game`,
//! `Games.GamePlayer`) and the queries that load them with their associations.

use std::collections::HashMap;
use std::fmt;

use serde::{Deserialize, Serialize};
use sqlx::SqliteConnection;

use crate::db::UtcDateTime;

macro_rules! text_enum {
    ($(#[$meta:meta])* $name:ident { $($(#[$vmeta:meta])* $variant:ident => $text:literal),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, sqlx::Type)]
        #[serde(rename_all = "snake_case")]
        #[sqlx(rename_all = "snake_case")]
        pub enum $name {
            $($(#[$vmeta])* $variant),+
        }

        impl $name {
            /// Every value, in declaration order.
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];

            /// The stored text.
            pub fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $text),+
                }
            }

            /// Parses the stored text.
            pub fn parse(value: &str) -> Option<Self> {
                match value {
                    $($text => Some(Self::$variant),)+
                    _ => None,
                }
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }
    };
}

text_enum! {
    /// A seat's result.
    GameResult {
        /// Won.
        Win => "win",
        /// Lost.
        Loss => "loss",
        /// Drew.
        Draw => "draw",
    }
}

text_enum! {
    /// A game's format, which decides how many winners it has.
    GameFormat {
        /// Free-for-all Commander.
        Commander => "commander",
        /// Two-Headed Giant: two winners.
        TwoHeadedGiant => "two_headed_giant",
        /// Five-player Star.
        FiveStar => "five_star",
    }
}

text_enum! {
    /// Where a game was recorded.
    GameSource {
        /// The web form or API.
        Manual => "manual",
        /// A CSV import.
        Csv => "csv",
        /// A Mythic Track import.
        MythicTrack => "mythic_track",
        /// The Discord bot.
        Discord => "discord",
    }
}

text_enum! {
    /// Where a deck list lives (`Deck.put_decklist_source/1`).
    DecklistSource {
        /// moxfield.com.
        Moxfield => "moxfield",
        /// archidekt.com.
        Archidekt => "archidekt",
        /// manavault.app.
        Manavault => "manavault",
        /// Any other URL.
        Other => "other",
    }
}

impl DecklistSource {
    /// `Deck.source/1`: by host, including subdomains.
    ///
    /// `lotus::decklist::DeckLink` only recognizes the bare and `www.` hosts and has no
    /// "other" source, so this keeps the Elixir rule.
    pub fn of_url(url: &str) -> Self {
        let host = url::Url::parse(url).ok().and_then(|url| url.host_str().map(str::to_owned)).unwrap_or_default();
        let matches = |domain: &str| host == domain || host.ends_with(&format!(".{domain}"));
        if matches("moxfield.com") {
            Self::Moxfield
        } else if matches("archidekt.com") {
            Self::Archidekt
        } else if matches("manavault.app") {
            Self::Manavault
        } else {
            Self::Other
        }
    }
}

/// A player (`players`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Player {
    /// Primary key.
    pub id: i64,
    /// Display name, unique case-insensitively.
    pub name: String,
    /// Linked account.
    pub user_id: Option<i64>,
    /// Discord snowflake.
    pub discord_id: Option<String>,
    /// Hidden from pickers.
    pub archived_at: Option<UtcDateTime>,
    /// The linked user's avatar; loaded only by `list_players` and `get_player_detail`
    /// (`Player.avatar_url` is a virtual field populated there).
    pub avatar_url: Option<String>,
    /// Created.
    pub inserted_at: UtcDateTime,
    /// Updated.
    pub updated_at: UtcDateTime,
}

/// A deck (`decks`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Deck {
    /// Primary key.
    pub id: i64,
    /// Owner.
    pub player_id: i64,
    /// Name, unique per player case-insensitively.
    pub name: String,
    /// Commander's Scryfall id.
    pub commander_card_id: Option<String>,
    /// Commander's name.
    pub commander_name: String,
    /// Chosen commander printing.
    pub commander_printing_id: Option<String>,
    /// Partner's Scryfall id.
    pub partner_card_id: Option<String>,
    /// Partner's name.
    pub partner_name: Option<String>,
    /// Chosen partner printing.
    pub partner_printing_id: Option<String>,
    /// Unique WUBRG letters (not necessarily ordered).
    pub color_identity: String,
    /// Deck list link.
    pub decklist_url: Option<String>,
    /// Derived from the link.
    pub decklist_source: Option<DecklistSource>,
    /// Retired.
    pub archived_at: Option<UtcDateTime>,
    /// Deck chooser skips since last played.
    pub skip_count: i64,
    /// Offered by the deck chooser.
    pub included_for_play: bool,
    /// Created.
    pub inserted_at: UtcDateTime,
    /// Updated.
    pub updated_at: UtcDateTime,
}

impl Default for Deck {
    fn default() -> Self {
        Self {
            id: 0,
            player_id: 0,
            name: String::new(),
            commander_card_id: None,
            commander_name: String::new(),
            commander_printing_id: None,
            partner_card_id: None,
            partner_name: None,
            partner_printing_id: None,
            color_identity: String::new(),
            decklist_url: None,
            decklist_source: None,
            archived_at: None,
            skip_count: 0,
            included_for_play: true,
            inserted_at: UtcDateTime::default(),
            updated_at: UtcDateTime::default(),
        }
    }
}

/// A seat at a game (`game_players`) with its player and deck.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Seat {
    /// Primary key.
    pub id: i64,
    /// The game.
    pub game_id: i64,
    /// Who sat here.
    pub player_id: i64,
    /// Which deck they played.
    pub deck_id: Option<i64>,
    /// Seat number, 1-based.
    pub seat: i64,
    /// Result.
    pub result: GameResult,
    /// Kills (`nil` when not recorded, distinct from zero).
    pub kills: Option<i64>,
    /// Turn eliminated.
    pub eliminated_turn: Option<i64>,
    /// Who eliminated them.
    pub eliminated_by_player_id: Option<i64>,
    /// MVP card id.
    pub mvp_card_id: Option<String>,
    /// MVP card name.
    pub mvp_card_name: Option<String>,
    /// Notes.
    pub notes: Option<String>,
    /// Created.
    pub inserted_at: UtcDateTime,
    /// Updated.
    pub updated_at: UtcDateTime,
    /// The player.
    pub player: Player,
    /// The deck.
    pub deck: Option<Deck>,
}

impl Default for Seat {
    fn default() -> Self {
        Self {
            id: 0,
            game_id: 0,
            player_id: 0,
            deck_id: None,
            seat: 1,
            result: GameResult::Loss,
            kills: None,
            eliminated_turn: None,
            eliminated_by_player_id: None,
            mvp_card_id: None,
            mvp_card_name: None,
            notes: None,
            inserted_at: UtcDateTime::default(),
            updated_at: UtcDateTime::default(),
            player: Player::default(),
            deck: None,
        }
    }
}

/// A recorded game (`games`) with its seats (ordered by row id), players, and decks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Game {
    /// Primary key.
    pub id: i64,
    /// When it was played.
    pub played_at: UtcDateTime,
    /// Length in minutes.
    pub duration_minutes: Option<i64>,
    /// Turns.
    pub turns: Option<i64>,
    /// How it was won.
    pub win_condition: Option<super::win_condition::WinCondition>,
    /// Notes.
    pub notes: Option<String>,
    /// Provenance.
    pub source: GameSource,
    /// Format.
    pub format: GameFormat,
    /// Import or bot id, unique per source.
    pub external_id: Option<String>,
    /// Stable id for portable exports.
    pub portable_id: Option<String>,
    /// Who recorded it.
    pub created_by_user_id: Option<i64>,
    /// Created.
    pub inserted_at: UtcDateTime,
    /// Updated.
    pub updated_at: UtcDateTime,
    /// Seats.
    pub seats: Vec<Seat>,
}

impl Default for Game {
    fn default() -> Self {
        Self {
            id: 0,
            played_at: UtcDateTime::default(),
            duration_minutes: None,
            turns: None,
            win_condition: None,
            notes: None,
            source: GameSource::Manual,
            format: GameFormat::Commander,
            external_id: None,
            portable_id: None,
            created_by_user_id: None,
            inserted_at: UtcDateTime::default(),
            updated_at: UtcDateTime::default(),
            seats: Vec::new(),
        }
    }
}

impl Game {
    /// The first winning seat.
    pub fn winner(&self) -> Option<&Seat> {
        self.seats.iter().find(|seat| seat.result == GameResult::Win)
    }
}

/// Selects full deck rows: `select_decks!("WHERE id = ?", id)`.
macro_rules! select_decks {
    ($tail:literal $(, $arg:expr)* $(,)?) => {
        sqlx::query_as!(
            crate::games::model::Deck,
            r#"SELECT id AS "id!", player_id, name, commander_card_id, commander_name, commander_printing_id,
                partner_card_id, partner_name, partner_printing_id, color_identity, decklist_url,
                decklist_source AS "decklist_source: crate::games::model::DecklistSource",
                archived_at AS "archived_at: crate::db::UtcDateTime", skip_count, included_for_play AS "included_for_play: bool",
                inserted_at AS "inserted_at: crate::db::UtcDateTime", updated_at AS "updated_at: crate::db::UtcDateTime"
               FROM decks "# + $tail
            $(, $arg)*
        )
    };
}
pub(crate) use select_decks;

/// Selects player rows without the avatar: `select_players!("WHERE id = ?", id)`.
macro_rules! select_players {
    ($tail:literal $(, $arg:expr)* $(,)?) => {
        sqlx::query_as!(
            crate::games::model::Player,
            r#"SELECT id AS "id!", name, user_id, discord_id, archived_at AS "archived_at: crate::db::UtcDateTime",
                NULL AS "avatar_url?: String",
                inserted_at AS "inserted_at: crate::db::UtcDateTime", updated_at AS "updated_at: crate::db::UtcDateTime"
               FROM players "# + $tail
            $(, $arg)*
        )
    };
}
pub(crate) use select_players;

/// A player by id (no avatar).
pub async fn get_player(conn: &mut SqliteConnection, id: i64) -> Result<Option<Player>, sqlx::Error> {
    select_players!("WHERE id = ?", id).fetch_optional(&mut *conn).await
}

/// A deck by id.
pub async fn get_deck(conn: &mut SqliteConnection, id: i64) -> Result<Option<Deck>, sqlx::Error> {
    select_decks!("WHERE id = ?", id).fetch_optional(&mut *conn).await
}

/// Games by id with seats, players, and decks, in the order of `ids`.
pub async fn load_games(conn: &mut SqliteConnection, ids: &[i64]) -> Result<Vec<Game>, sqlx::Error> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let ids_json = serde_json::to_string(ids).unwrap_or_else(|_| "[]".into());
    let rows = sqlx::query!(
        r#"SELECT id AS "id!", played_at AS "played_at: UtcDateTime", duration_minutes, turns,
                  win_condition AS "win_condition: super::win_condition::WinCondition", notes,
                  source AS "source: GameSource", format AS "format: GameFormat", external_id, portable_id,
                  created_by_user_id, inserted_at AS "inserted_at: UtcDateTime", updated_at AS "updated_at: UtcDateTime"
           FROM games WHERE id IN (SELECT value FROM json_each(?))"#,
        ids_json
    )
    .fetch_all(&mut *conn)
    .await?;
    let mut seats = load_seats(conn, &ids_json).await?;
    let mut games: HashMap<i64, Game> = rows
        .into_iter()
        .map(|row| {
            let game = Game {
                id: row.id,
                played_at: row.played_at,
                duration_minutes: row.duration_minutes,
                turns: row.turns,
                win_condition: row.win_condition,
                notes: row.notes,
                source: row.source,
                format: row.format,
                external_id: row.external_id,
                portable_id: row.portable_id,
                created_by_user_id: row.created_by_user_id,
                inserted_at: row.inserted_at,
                updated_at: row.updated_at,
                seats: seats.remove(&row.id).unwrap_or_default(),
            };
            (game.id, game)
        })
        .collect();
    Ok(ids.iter().filter_map(|id| games.remove(id)).collect())
}

/// One game with its seats.
pub async fn load_game(conn: &mut SqliteConnection, id: i64) -> Result<Option<Game>, sqlx::Error> {
    Ok(load_games(conn, &[id]).await?.into_iter().next())
}

async fn load_seats(conn: &mut SqliteConnection, game_ids_json: &str) -> Result<HashMap<i64, Vec<Seat>>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT s.id AS "id!", s.game_id, s.player_id, s.deck_id, s.seat, s.result AS "result: GameResult",
                  s.kills, s.eliminated_turn, s.eliminated_by_player_id, s.mvp_card_id, s.mvp_card_name, s.notes,
                  s.inserted_at AS "inserted_at: UtcDateTime", s.updated_at AS "updated_at: UtcDateTime",
                  p.name AS player_name, p.user_id AS player_user_id, p.discord_id AS player_discord_id,
                  p.archived_at AS "player_archived_at: UtcDateTime",
                  p.inserted_at AS "player_inserted_at: UtcDateTime", p.updated_at AS "player_updated_at: UtcDateTime",
                  d.id AS "d_id?: i64", d.player_id AS "d_player_id?: i64", d.name AS "d_name?: String",
                  d.commander_card_id AS "d_commander_card_id?: String", d.commander_name AS "d_commander_name?: String",
                  d.commander_printing_id AS "d_commander_printing_id?: String",
                  d.partner_card_id AS "d_partner_card_id?: String", d.partner_name AS "d_partner_name?: String",
                  d.partner_printing_id AS "d_partner_printing_id?: String",
                  d.color_identity AS "d_color_identity?: String", d.decklist_url AS "d_decklist_url?: String",
                  d.decklist_source AS "d_decklist_source?: DecklistSource",
                  d.archived_at AS "d_archived_at?: UtcDateTime", d.skip_count AS "d_skip_count?: i64",
                  d.included_for_play AS "d_included_for_play?: bool",
                  d.inserted_at AS "d_inserted_at?: UtcDateTime", d.updated_at AS "d_updated_at?: UtcDateTime"
           FROM game_players s
           JOIN players p ON p.id = s.player_id
           LEFT JOIN decks d ON d.id = s.deck_id
           WHERE s.game_id IN (SELECT value FROM json_each(?))
           ORDER BY s.id"#,
        game_ids_json
    )
    .fetch_all(&mut *conn)
    .await?;
    let mut seats: HashMap<i64, Vec<Seat>> = HashMap::new();
    for row in rows {
        let deck = match (row.d_id, row.d_player_id, row.d_name, row.d_commander_name) {
            (Some(id), Some(player_id), Some(name), Some(commander_name)) => Some(Deck {
                id,
                player_id,
                name,
                commander_card_id: row.d_commander_card_id,
                commander_name,
                commander_printing_id: row.d_commander_printing_id,
                partner_card_id: row.d_partner_card_id,
                partner_name: row.d_partner_name,
                partner_printing_id: row.d_partner_printing_id,
                color_identity: row.d_color_identity.unwrap_or_default(),
                decklist_url: row.d_decklist_url,
                decklist_source: row.d_decklist_source,
                archived_at: row.d_archived_at,
                skip_count: row.d_skip_count.unwrap_or_default(),
                included_for_play: row.d_included_for_play.unwrap_or(true),
                inserted_at: row.d_inserted_at.unwrap_or_default(),
                updated_at: row.d_updated_at.unwrap_or_default(),
            }),
            _ => None,
        };
        let seat = Seat {
            id: row.id,
            game_id: row.game_id,
            player_id: row.player_id,
            deck_id: row.deck_id,
            seat: row.seat,
            result: row.result,
            kills: row.kills,
            eliminated_turn: row.eliminated_turn,
            eliminated_by_player_id: row.eliminated_by_player_id,
            mvp_card_id: row.mvp_card_id,
            mvp_card_name: row.mvp_card_name,
            notes: row.notes,
            inserted_at: row.inserted_at,
            updated_at: row.updated_at,
            player: Player {
                id: row.player_id,
                name: row.player_name,
                user_id: row.player_user_id,
                discord_id: row.player_discord_id,
                archived_at: row.player_archived_at,
                avatar_url: None,
                inserted_at: row.player_inserted_at,
                updated_at: row.player_updated_at,
            },
            deck,
        };
        seats.entry(seat.game_id).or_default().push(seat);
    }
    Ok(seats)
}
