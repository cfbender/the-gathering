//! The public `/newgame` queue message, ready announcement, and maybe ping
//! (`NewGameMessage`).

use jiff::Timestamp;
use jiff::tz::TimeZone;

use crate::db::UtcDateTime;

use super::api::{
    AllowedMentions, Button, ButtonStyle, Component, Embed, EmbedField, MessagePayload,
};
use super::scheduled::{MAYBE_GRACE_SECONDS, Roster, ScheduledGame, Status};

fn roster(list: &Roster) -> String {
    let mut entries: Vec<(&UtcDateTime, &String)> = list
        .iter()
        .map(|(id, entry)| (&entry.joined_at, id))
        .collect();
    entries.sort();
    entries
        .into_iter()
        .map(|(_, id)| format!("<@{id}>"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn url(public_url: &str, game: &ScheduledGame) -> String {
    format!(
        "{public_url}/table/{}",
        game.room_id.as_deref().unwrap_or_default()
    )
}

fn deadline(game: &ScheduledGame) -> i64 {
    game.maybe_pinged_at.map_or(0, UtcDateTime::unix) + MAYBE_GRACE_SECONDS
}

fn description(public_url: &str, game: &ScheduledGame) -> String {
    match game.status {
        Status::Started => format!("Your game is ready! [Open lobby]({})", url(public_url, game)),
        Status::Expired => "This game did not fill before its start time.".into(),
        Status::Cancelled => "This game was cancelled.".into(),
        Status::Open if game.maybe_pinged_at.is_some() => {
            let deadline = deadline(game);
            format!(
                "Short of the minimum at start time, so the maybe list was pinged. Join by <t:{deadline}:t> (<t:{deadline}:R>) or this game expires."
            )
        }
        Status::Open if game.start_at.is_none() => "Join the roster to play. Maybe doesn't count toward the minimum. The host or a Discord Administrator can change the time or cancel.".into(),
        Status::Open => "Join the roster to play. Maybe doesn't count toward the minimum, but if the game is short at its start time, the maybe list is pinged. The host or a Discord Administrator can change the time or cancel.".into(),
    }
}

fn start(game: &ScheduledGame) -> String {
    match game.start_at {
        None => "As soon as the minimum is met".into(),
        Some(time) => {
            let unix = time.unix();
            format!("<t:{unix}:F> (<t:{unix}:R>)")
        }
    }
}

fn button(game: &ScheduledGame, action: &str, label: &str, style: ButtonStyle) -> Component {
    Component::Button(Button {
        custom_id: Some(format!("newgame:{}:{action}", game.id)),
        label: label.to_owned(),
        style,
        url: None,
        disabled: Some(game.status != Status::Open),
    })
}

/// The queue embed and buttons.
pub fn render(public_url: &str, game: &ScheduledGame) -> MessagePayload {
    let players = roster(&game.players);
    let mut fields = vec![
        EmbedField::new("Start", start(game)),
        EmbedField::inline("Minimum", game.min_players.to_string()),
        EmbedField::inline("Format", game.format.as_deref().unwrap_or("Commander")),
        EmbedField::new(
            format!("Players ({}/10)", game.players.len()),
            if players.is_empty() {
                "No players yet. Click Join!".to_owned()
            } else {
                players
            },
        ),
    ];
    if !game.maybe.is_empty() {
        fields.push(EmbedField::new(
            format!("Maybe ({}) — not counted", game.maybe.len()),
            roster(&game.maybe),
        ));
    }
    MessagePayload {
        content: Some(String::new()),
        allowed_mentions: Some(AllowedMentions::none()),
        embeds: Some(vec![Embed {
            title: game.title.clone(),
            description: description(public_url, game),
            fields,
        }]),
        components: Some(vec![Component::row(vec![
            button(game, "join", "Join", ButtonStyle::Success),
            button(game, "maybe", "Maybe", ButtonStyle::Secondary),
            button(game, "leave", "Leave", ButtonStyle::Secondary),
            button(game, "time", "Change time", ButtonStyle::Primary),
            button(game, "cancel", "Cancel", ButtonStyle::Danger),
        ])]),
        ..MessagePayload::default()
    }
}

/// The game's thread name: its title and local date, for example `Friday pod · Oct 16`.
/// Games that start when filled use the day they were posted.
pub fn thread_name(game: &ScheduledGame, zone: &str) -> String {
    let at = game.start_at.unwrap_or(game.inserted_at);
    let tz = TimeZone::get(zone).unwrap_or(TimeZone::UTC);
    let Ok(local) = Timestamp::from_second(at.unix()).map(|at| at.to_zoned(tz)) else {
        return game.title.clone();
    };
    let suffix = format!(" · {}", local.strftime("%b %-d"));
    // Discord caps thread names at 100 characters.
    let title = super::api::slice(
        &game.title,
        100_usize.saturating_sub(suffix.chars().count()),
    );
    format!("{title}{suffix}")
}

/// Announces a game in its thread to the opt-in role. Mentioning a role in a thread adds
/// the role's members to it (Discord does this only for roles under 100 members).
pub fn role_ping(game: &ScheduledGame, role_id: &str) -> MessagePayload {
    MessagePayload {
        content: Some(format!(
            "<@&{role_id}> New game: **{}**, starting {}.",
            game.title,
            match game.start_at {
                Some(start) => format!("<t:{}:F>", start.unix()),
                None => "as soon as the minimum joins".into(),
            }
        )),
        allowed_mentions: Some(AllowedMentions::roles(vec![role_id.to_owned()])),
        ..MessagePayload::default()
    }
}

/// The queue message before its first render, which has no buttons yet.
pub fn placeholder() -> MessagePayload {
    MessagePayload {
        content: Some("Preparing your game…".into()),
        allowed_mentions: Some(AllowedMentions::none()),
        ..MessagePayload::default()
    }
}

/// The `/newgame` reply once the queue has moved into the thread started on it.
pub fn pointer(game: &ScheduledGame) -> MessagePayload {
    MessagePayload {
        content: Some(format!("**{}**: sign up in the thread below.", game.title)),
        allowed_mentions: Some(AllowedMentions::none()),
        ..MessagePayload::default()
    }
}

fn mentions(ids: &[String]) -> String {
    ids.iter()
        .map(|id| format!("<@{id}>"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Pings the roster with the lobby link.
pub fn announcement(public_url: &str, game: &ScheduledGame) -> MessagePayload {
    let ids: Vec<String> = game.players.keys().cloned().collect();
    MessagePayload {
        content: Some(format!(
            "{} Your game is ready! {}\nSign in with Discord to join the table.",
            mentions(&ids),
            url(public_url, game)
        )),
        allowed_mentions: Some(AllowedMentions::users(ids)),
        nonce: Some(format!("newgame:{}", game.id)),
        enforce_nonce: Some(true),
        ..MessagePayload::default()
    }
}

/// Asks the maybe list to fill an underfilled game.
pub fn maybe_ping(game: &ScheduledGame) -> MessagePayload {
    let ids: Vec<String> = game.maybe.keys().cloned().collect();
    let missing = game.min_players - i64::try_from(game.players.len()).unwrap_or(i64::MAX);
    let deadline = deadline(game);
    MessagePayload {
        content: Some(format!(
            "{} **{}** is {missing} {} short at its start time. Click **Join** on the game by <t:{deadline}:t> (<t:{deadline}:R>) if you can play: https://discord.com/channels/{}/{}/{}",
            mentions(&ids),
            game.title,
            if missing == 1 { "player" } else { "players" },
            game.guild_id,
            game.queue_channel(),
            game.message_id.as_deref().unwrap_or_default()
        )),
        allowed_mentions: Some(AllowedMentions::users(ids)),
        // Discord nonces are capped at 25 characters; the ping time distinguishes re-pings.
        nonce: Some(format!(
            "ngm:{}:{}",
            game.id,
            game.maybe_pinged_at.map_or(0, UtcDateTime::unix)
        )),
        enforce_nonce: Some(true),
        ..MessagePayload::default()
    }
}
