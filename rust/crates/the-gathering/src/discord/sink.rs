//! Destinations for normalized Discord reports (`Discord.Sink`, `Sink.Games`,
//! `Sink.Logger`).
//!
//! A sink runs on the caller's connection, so resolving a staged game can record the game
//! and consume the staging row in one transaction.

use std::collections::HashSet;

use futures_util::future::BoxFuture;

use sqlx::{Connection, SqliteConnection};

use crate::games::{
    self, DeckInput, GameInput, GamesError, ResolveError as PlayerError, SeatInput,
};

use super::report::{GameReport, ReportDetails, ReportPlayer};

/// Why a sink rejected a report.
#[derive(Debug, thiserror::Error)]
pub enum SinkError {
    /// Not a Discord report.
    #[error("invalid_source")]
    InvalidSource,
    /// Blank external id.
    #[error("invalid_external_id")]
    InvalidExternalId,
    /// Fewer than 2 or more than 6 players.
    #[error("invalid_player_count")]
    InvalidPlayerCount,
    /// A player without an id or name.
    #[error("invalid_player")]
    InvalidPlayer,
    /// The same Discord id twice.
    #[error("duplicate_player")]
    DuplicatePlayer,
    /// More than one winner.
    #[error("multiple_winners")]
    MultipleWinners,
    /// A winner who did not play.
    #[error("unknown_winner")]
    UnknownWinner,
    /// Resolving a player failed.
    #[error("player could not be resolved")]
    Player(PlayerError),
    /// Recording the game or a deck failed.
    #[error("game could not be recorded")]
    Games(GamesError),
    /// Any other failure (test sinks).
    #[error("{0}")]
    Other(String),
    /// Database error.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

impl From<GamesError> for SinkError {
    fn from(error: GamesError) -> Self {
        Self::Games(error)
    }
}

impl From<PlayerError> for SinkError {
    fn from(error: PlayerError) -> Self {
        Self::Player(error)
    }
}

/// `@callback handle_report(GameReport.t())`.
pub trait Sink: Send + Sync {
    /// Handles a report on `conn` (inside the caller's transaction).
    fn handle_report<'a>(
        &'a self,
        conn: &'a mut SqliteConnection,
        report: &'a GameReport,
    ) -> BoxFuture<'a, Result<(), SinkError>>;
}

/// Records receipt without logging raw Discord data (`Sink.Logger`).
#[derive(Clone, Copy, Debug, Default)]
pub struct LoggerSink;

impl Sink for LoggerSink {
    fn handle_report<'a>(
        &'a self,
        _conn: &'a mut SqliteConnection,
        report: &'a GameReport,
    ) -> BoxFuture<'a, Result<(), SinkError>> {
        Box::pin(async move {
            tracing::info!(
                "Discord game report received external_id={} player_count={} winner_count={}",
                report.external_id,
                report.players.len(),
                report.winner_discord_ids.len()
            );
            Ok(())
        })
    }
}

/// Persists completed reports in the games domain (`Sink.Games`).
#[derive(Clone, Copy, Debug, Default)]
pub struct GamesSink;

impl Sink for GamesSink {
    fn handle_report<'a>(
        &'a self,
        conn: &'a mut SqliteConnection,
        report: &'a GameReport,
    ) -> BoxFuture<'a, Result<(), SinkError>> {
        Box::pin(async move {
            validate(report)?;
            if report.winner_discord_ids.is_empty() {
                tracing::info!(
                    "Discord game awaiting winner external_id={} player_count={}",
                    report.external_id,
                    report.players.len()
                );
                return Ok(());
            }
            let mut tx = conn.begin().await?;
            match persist(&mut tx, report).await {
                Ok(game_id) => {
                    tx.commit().await?;
                    tracing::info!(
                        "Discord game recorded external_id={} game_id={game_id} player_count={}",
                        report.external_id,
                        report.players.len()
                    );
                    Ok(())
                }
                Err(error) => {
                    tx.rollback().await?;
                    tracing::warn!(
                        "Could not record Discord game external_id={}",
                        report.external_id
                    );
                    Err(error)
                }
            }
        })
    }
}

async fn persist(conn: &mut SqliteConnection, report: &GameReport) -> Result<i64, SinkError> {
    let seats = build_seats(conn, report).await?;
    let mut input = GameInput {
        played_at: Some(report.played_at).into(),
        seats: Some(seats).into(),
        ..GameInput::default()
    };
    if let Some(details) = &report.details {
        input.win_condition = details.win_condition.clone().into();
        input.turns = details.turns.into();
        input.duration_minutes = details.duration_minutes.into();
        input.notes = details.notes.clone().into();
    }
    let game =
        games::record_game::upsert_by_external_id(conn, "discord", &report.external_id, &input)
            .await?;
    Ok(game.id)
}

async fn build_seats(
    conn: &mut SqliteConnection,
    report: &GameReport,
) -> Result<Vec<SeatInput>, SinkError> {
    let winners: HashSet<&str> = report
        .winner_discord_ids
        .iter()
        .map(String::as_str)
        .collect();
    let mut seats = Vec::with_capacity(report.players.len());
    for (index, reported) in report.players.iter().enumerate() {
        let player = games::resolve_player::run(
            conn,
            &reported.display_name,
            Some(&reported.discord_id),
            None,
        )
        .await?;
        let deck_id =
            find_or_create_deck(conn, player.id, reported, report.details.as_ref()).await?;
        let won = winners.contains(reported.discord_id.as_str());
        let kills = report
            .details
            .as_ref()
            .and_then(|details| details.kills.get(&reported.discord_id).copied().flatten());
        let mut seat = SeatInput {
            player_id: Some(player.id).into(),
            deck_id: deck_id.into(),
            seat: i64::try_from(index + 1).ok().into(),
            kills: kills.into(),
            result: Some(if won { "win" } else { "loss" }.to_owned()).into(),
            ..SeatInput::default()
        };
        if won && let Some(details) = &report.details {
            seat.mvp_card_id = details.mvp_card_id.clone().into();
            seat.mvp_card_name = details.mvp_card_name.clone().into();
        }
        seats.push(seat);
    }
    Ok(seats)
}

/// Discord reports never carry deck-list URLs, so no server-specific link rules apply.
const NO_URLS: &games::DeckLinks = &games::DeckLinks::EMPTY;

async fn find_or_create_deck(
    conn: &mut SqliteConnection,
    player_id: i64,
    reported: &ReportPlayer,
    details: Option<&ReportDetails>,
) -> Result<Option<i64>, SinkError> {
    match details.and_then(|details| details.commanders.get(&reported.discord_id)) {
        Some(None) => Ok(None),
        Some(Some(attrs)) => {
            let name: String = [Some(&attrs.commander_name), attrs.partner_name.as_ref()]
                .into_iter()
                .flatten()
                .cloned()
                .collect::<Vec<_>>()
                .join(" + ")
                .chars()
                .take(100)
                .collect();
            let input = DeckInput {
                commander_card_id: Some(attrs.commander_card_id.clone()).into(),
                commander_name: Some(attrs.commander_name.clone()).into(),
                partner_card_id: attrs.partner_card_id.clone().into(),
                partner_name: attrs.partner_name.clone().into(),
                color_identity: Some(attrs.color_identity.clone()).into(),
                ..DeckInput::default()
            };
            let deck =
                games::deck::find_or_create_deck(conn, NO_URLS, player_id, &name, &input).await?;
            Ok(Some(deck.id))
        }
        None => match reported.commander_name.as_deref() {
            None | Some("") => Ok(None),
            Some(commander) => {
                let deck = games::deck::find_or_create_deck(
                    conn,
                    NO_URLS,
                    player_id,
                    commander,
                    &DeckInput {
                        commander_name: Some(commander.to_owned()).into(),
                        ..DeckInput::default()
                    },
                )
                .await?;
                Ok(Some(deck.id))
            }
        },
    }
}

fn validate(report: &GameReport) -> Result<(), SinkError> {
    if report.source != "discord" {
        return Err(SinkError::InvalidSource);
    }
    if report.external_id.is_empty() {
        return Err(SinkError::InvalidExternalId);
    }
    if !(2..=6).contains(&report.players.len()) {
        return Err(SinkError::InvalidPlayerCount);
    }
    if report
        .players
        .iter()
        .any(|player| player.discord_id.is_empty() || player.display_name.trim().is_empty())
    {
        return Err(SinkError::InvalidPlayer);
    }
    let ids: Vec<&str> = report
        .players
        .iter()
        .map(|player| player.discord_id.as_str())
        .collect();
    if ids.iter().collect::<HashSet<_>>().len() != ids.len() {
        return Err(SinkError::DuplicatePlayer);
    }
    if report.winner_discord_ids.len() > 1 {
        return Err(SinkError::MultipleWinners);
    }
    if report
        .winner_discord_ids
        .iter()
        .any(|winner| !ids.contains(&winner.as_str()))
    {
        return Err(SinkError::UnknownWinner);
    }
    Ok(())
}
