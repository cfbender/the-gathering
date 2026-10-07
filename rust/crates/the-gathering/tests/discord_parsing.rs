//! SpellBot message parsing, start times, command routing, and pending-game pruning.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod support;

use serde_json::{Value, json};
use support::discord::{Call, RecordingApi};
use the_gathering::db::UtcDateTime;
use the_gathering::discord::api::{OptionKind, RegisteredCommand};
use the_gathering::discord::interaction::{Interaction, InteractionData, OptionValue};
use the_gathering::discord::spellbot::{self, ParseError, SpellBotMessage};
use the_gathering::discord::{command, start_time};

const SPELLBOT_ID: &str = "725510263251402832";

fn fixture() -> Value {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/discord/spellbot_game_ready.json");
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

fn parse(value: &Value) -> Result<the_gathering::discord::GameReport, ParseError> {
    let message: SpellBotMessage = serde_json::from_value(value.clone()).unwrap();
    spellbot::parse(&message, SPELLBOT_ID)
}

fn utc(value: &str) -> UtcDateTime {
    UtcDateTime::parse(value).unwrap()
}

#[test]
fn parses_a_scrubbed_real_spellbot_ready_embed_into_a_report() {
    let report = parse(&fixture()).unwrap();
    assert_eq!(report.external_id, "spellbot:SB12345");
    assert_eq!(report.source, "discord");
    assert_eq!(report.guild_id, "333333333333333333");
    assert_eq!(report.channel_id, "444444444444444444");
    assert_eq!(report.played_at, utc("2025-06-15T15:08:43Z"));
    assert!(report.winner_discord_ids.is_empty());
    let players: Vec<(&str, &str, Option<&str>)> = report
        .players
        .iter()
        .map(|player| {
            (
                player.discord_id.as_str(),
                player.display_name.as_str(),
                player.commander_name.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        players,
        [
            ("111111111111111111", "Aria", None),
            ("222222222222222222", "Bryn", None)
        ]
    );
    assert_eq!(report.raw["message_id"], "999999999999999999");
}

#[test]
fn rejects_a_lookalike_embed_from_another_author() {
    let mut message = fixture();
    message["author"]["id"] = json!("555555555555555555");
    assert_eq!(parse(&message).unwrap_err(), ParseError::NotSpellbot);
}

#[test]
fn ignores_messages_with_nullable_author_bot_flags_including_partial_updates() {
    for author in [Value::Null, json!({}), json!({"bot": false})] {
        assert_eq!(
            parse(&json!({ "author": author })).unwrap_err(),
            ParseError::NotSpellbot
        );
    }
    // Snowflakes may arrive as integers.
    let message = json!({"author": {"id": 725_510_263_251_402_832_u64, "bot": true}});
    assert_eq!(parse(&message).unwrap_err(), ParseError::NoEmbeds);
}

#[test]
fn rejects_malformed_and_unrelated_messages_without_raising() {
    assert_eq!(parse(&json!({})).unwrap_err(), ParseError::NotSpellbot);

    let mut message = fixture();
    message["embeds"][0]["title"] = json!("Looking for players");
    assert_eq!(parse(&message).unwrap_err(), ParseError::NotStartedGame);

    let mut message = fixture();
    message["embeds"][0]["fields"][2]["value"] = json!("not a timestamp");
    assert_eq!(parse(&message).unwrap_err(), ParseError::InvalidStartedAt);
}

#[test]
fn ignores_spellbots_embed_less_placeholder_and_text_messages() {
    for content in ["", "You are already in a game."] {
        let mut message = fixture();
        message["embeds"] = json!([]);
        message["content"] = json!(content);
        assert_eq!(parse(&message).unwrap_err(), ParseError::NoEmbeds);
    }
    let mut message = fixture();
    message.as_object_mut().unwrap().remove("embeds");
    assert_eq!(parse(&message).unwrap_err(), ParseError::NoEmbeds);
}

#[test]
fn parses_a_twilight_gateway_message() {
    let mut message = fixture();
    message["timestamp"] = json!("2025-06-15T15:08:43.000000+00:00");
    message["type"] = json!(0);
    message["tts"] = json!(false);
    message["mention_everyone"] = json!(false);
    message["mentions"] = json!([]);
    message["mention_roles"] = json!([]);
    message["attachments"] = json!([]);
    message["pinned"] = json!(false);
    message["content"] = json!("");
    message["author"]["discriminator"] = json!("0");
    message["author"]["avatar"] = Value::Null;
    let message: twilight_model::channel::Message = serde_json::from_value(message).unwrap();
    let report = spellbot::parse(&SpellBotMessage::from(&message), SPELLBOT_ID).unwrap();
    assert_eq!(report.external_id, "spellbot:SB12345");
    assert_eq!(report.players.len(), 2);
    assert_eq!(report.channel_id, "444444444444444444");
}

// StartTime

const ZONE: &str = "America/New_York";

fn start(input: Option<&str>, now: &str, zone: &str) -> Result<Option<UtcDateTime>, String> {
    start_time::parse(input, utc(now), zone)
}

fn at(input: &str, now: &str) -> UtcDateTime {
    start(Some(input), now, ZONE).unwrap().unwrap()
}

const NOW: &str = "2026-09-23T18:15:00Z";

#[test]
fn omitted_starts_when_filled_bare_times_use_today_or_the_next_local_day() {
    assert_eq!(start(None, NOW, ZONE), Ok(None));
    assert_eq!(at("8pm", NOW), utc("2026-09-24T00:00:00Z"));
    assert_eq!(at("20:30", NOW), utc("2026-09-24T00:30:00Z"));
    assert_eq!(at("1:30pm", NOW), utc("2026-09-24T17:30:00Z"));
    assert_eq!(at("14:15", NOW), utc("2026-09-24T18:15:00Z"));
    assert_eq!(at("Tomorrow 7PM", NOW), utc("2026-09-24T23:00:00Z"));
}

#[test]
fn noon_and_midnight_are_distinct_and_timezones_are_configurable() {
    assert_eq!(at("12am", NOW), utc("2026-09-24T04:00:00Z"));
    assert_eq!(at("12pm", NOW), utc("2026-09-24T16:00:00Z"));
    assert_eq!(
        start(Some("20:30"), NOW, "Etc/UTC"),
        Ok(Some(utc("2026-09-23T20:30:00Z")))
    );
    assert!(start(Some("20:30"), NOW, "Not/AZone").is_err());
}

#[test]
fn relative_duration_is_elapsed_time_including_over_dst() {
    assert_eq!(at("in 45m", NOW), utc("2026-09-23T19:00:00Z"));
    assert_eq!(at("in 2h", NOW), utc("2026-09-23T20:15:00Z"));
    assert_eq!(
        at("in 45m", "2026-03-08T06:30:00Z"),
        utc("2026-03-08T07:15:00Z")
    );
}

#[test]
fn discord_timestamps_are_absolute_regardless_of_timezone_or_display_style() {
    let unix = utc("2026-09-23T19:00:00Z").unix();
    for suffix in ["", ":F", ":R", ":t"] {
        assert_eq!(
            at(&format!("<t:{unix}{suffix}>"), NOW),
            utc("2026-09-23T19:00:00Z")
        );
    }
}

#[test]
fn rejects_past_equal_empty_and_malformed_inputs() {
    let now_unix = format!("<t:{}>", utc(NOW).unix());
    for input in [
        "<t:1:F>",
        now_unix.as_str(),
        "in 0m",
        "in -1h",
        "25:00",
        "0pm",
        "13pm",
        "8:77am",
        "next week",
        "",
        "<t:99999999999999999999999>",
    ] {
        assert!(start(Some(input), NOW, ZONE).is_err(), "{input}");
    }
}

#[test]
fn tomorrow_and_rollover_use_calendar_days_across_spring_and_autumn_dst() {
    assert_eq!(
        at("tomorrow 7pm", "2026-03-07T18:00:00Z"),
        utc("2026-03-08T23:00:00Z")
    );
    assert_eq!(
        at("1pm", "2026-03-07T19:00:00Z"),
        utc("2026-03-08T17:00:00Z")
    );
    assert_eq!(
        at("tomorrow 7pm", "2026-10-31T18:00:00Z"),
        utc("2026-11-02T00:00:00Z")
    );
}

#[test]
fn rejects_nonexistent_and_ambiguous_dst_wall_times_rather_than_guessing() {
    let gap = start(Some("tomorrow 2:30am"), "2026-03-07T18:00:00Z", ZONE).unwrap_err();
    assert!(gap.contains("does not exist"));
    let ambiguous = start(Some("tomorrow 1:30am"), "2026-10-31T18:00:00Z", ZONE).unwrap_err();
    assert!(ambiguous.contains("occurs twice"));
}

// Command registration

#[tokio::test]
async fn registers_log_with_an_optional_winner_and_removes_only_won_in_each_scope() {
    for guild in [None, Some("333")] {
        let api = RecordingApi::with_commands(vec![
            RegisteredCommand {
                id: "123".into(),
                name: "won".into(),
            },
            RegisteredCommand {
                id: "456".into(),
                name: "other".into(),
            },
        ]);
        let description = command::register(&api, "888", guild).await.unwrap();
        assert!(description.contains("/log, /summary, and /newgame"));
        let calls = api.take();
        let guild = guild.map(str::to_owned);
        let Call::CreateCommand(scope, log) = &calls[0] else {
            panic!("{calls:?}")
        };
        assert_eq!(scope, &guild);
        assert_eq!(log.name, "log");
        let winner = log.options.iter().find(|o| o.name == "winner").unwrap();
        assert_eq!((winner.kind, winner.required), (OptionKind::User, false));
        let Call::CreateCommand(_, summary) = &calls[1] else {
            panic!()
        };
        assert_eq!(summary.name, "summary");
        let Call::CreateCommand(_, newgame) = &calls[2] else {
            panic!()
        };
        assert_eq!(newgame.name, "newgame");
        assert!(!newgame.dm_permission);
        let names: Vec<&str> = newgame.options.iter().map(|o| o.name.as_str()).collect();
        assert_eq!(names, ["start", "min_players", "title", "format"]);
        let min = &newgame.options[1];
        assert_eq!(
            (min.kind, min.min_value, min.max_value, min.required),
            (OptionKind::Integer, Some(2), Some(10), false)
        );
        assert_eq!(calls[3], Call::DeleteCommand(guild.clone(), "123".into()));
        assert_eq!(calls.len(), 4);
    }
}

#[tokio::test]
async fn does_not_start_when_the_bot_token_is_absent() {
    let app = support::TestApp::new().await;
    assert!(app.state.config.discord_bot.is_none());
    // Logs "Discord bot disabled" and spawns nothing.
    the_gathering::discord::start(&app.state);
}

#[test]
fn converts_twilight_interactions() {
    let interaction: twilight_model::application::interaction::Interaction =
        serde_json::from_value(json!({
            "id": "777",
            "application_id": "888",
            "type": 2,
            "token": "test-only-token",
            "version": 1,
            "guild_id": "333",
            "channel": {"id": "444", "type": 0},
            "member": {
                "user": {"id": "111", "username": "name", "discriminator": "0", "avatar": null},
                "nick": "Nick",
                "roles": [],
                "joined_at": "2025-01-01T00:00:00.000000+00:00",
                "deaf": false,
                "mute": false,
                "flags": 0,
                "permissions": "8"
            },
            "data": {
                "id": "1",
                "name": "log",
                "type": 1,
                "options": [{"name": "winner", "type": 6, "value": "222"}]
            },
            "entitlements": [],
            "authorizing_integration_owners": {}
        }))
        .unwrap();
    let interaction = Interaction::from(interaction);
    assert_eq!(interaction.user_id(), "111");
    assert_eq!(interaction.guild_id.as_deref(), Some("333"));
    assert_eq!(interaction.channel_id.as_deref(), Some("444"));
    assert!(interaction.administrator());
    assert_eq!(interaction.command_name(), Some("log"));
    assert_eq!(
        interaction.option("winner"),
        Some(&OptionValue::User("222".into()))
    );
    assert!(matches!(interaction.data, InteractionData::Command(_)));
}
