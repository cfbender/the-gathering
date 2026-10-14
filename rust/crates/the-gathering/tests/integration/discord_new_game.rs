//! The Discord `/newgame` queue (with `support::discord::RecordingApi` standing in for
//! Discord).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use crate::support;

use std::sync::{Arc, Mutex};

use serde_json::json;
use support::TestApp;
use support::discord::{Call, Op, RecordingApi};
use the_gathering::config::DiscordBotConfig;
use the_gathering::db::UtcDateTime;
use the_gathering::discord::api::DiscordApi;
use the_gathering::discord::api::{
    Button, Component, Embed, EmbedField, InteractionResponse, MessagePayload, ResponseKind,
};
use the_gathering::discord::interaction::{
    CommandData, CommandOptionData, ComponentData, Interaction, InteractionData, InteractionMember,
    InteractionUser, ModalData, OptionValue,
};
use the_gathering::discord::new_game::{self, RespondError};
use the_gathering::discord::new_game_message;
use the_gathering::discord::rest::RestApi;
use the_gathering::discord::scheduled::{
    self, MAYBE_GRACE_SECONDS, NewQueue, QueueAction, QueueActor, QueueError, ScheduledGame, Status,
};
use the_gathering::discord::scheduler::NewGameScheduler;
use wiremock::matchers::{body_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const NOW: &str = "2026-09-23T18:00:00Z";
const ADMINISTRATOR: u64 = 8;
const MANAGE_GUILD: u64 = 32;

fn utc(value: &str) -> UtcDateTime {
    UtcDateTime::parse(value).unwrap()
}

fn now() -> UtcDateTime {
    utc(NOW)
}

fn plus(time: UtcDateTime, seconds: i64) -> UtcDateTime {
    time.plus(time::Duration::seconds(seconds))
}

struct Ctx {
    app: TestApp,
    api: Arc<RecordingApi>,
    clock: Arc<Mutex<UtcDateTime>>,
    scheduler: Arc<NewGameScheduler>,
}

impl Ctx {
    fn set_now(&self, time: UtcDateTime) {
        *self.clock.lock().unwrap() = time;
    }

    /// A freshly booted scheduler (its first sweep reloads pending work).
    async fn restart(&mut self) {
        self.scheduler = scheduler(&self.app, &self.api, &self.clock);
        self.scheduler.sweep().await;
    }

    async fn act(
        &self,
        id: i64,
        action: QueueAction,
        actor: &QueueActor,
    ) -> Result<ScheduledGame, QueueError> {
        self.scheduler.act(id, &action, actor).await
    }

    async fn respond(&self, event: &Interaction) -> Result<(), RespondError> {
        new_game::respond(
            &self.app.state,
            self.api.as_ref(),
            &self.scheduler,
            event,
            now(),
        )
        .await
    }

    async fn game(&self, id: i64) -> ScheduledGame {
        let mut conn = self.app.pool().acquire().await.unwrap();
        scheduled::get(&mut conn, id).await.unwrap().unwrap()
    }

    async fn only_game(&self) -> ScheduledGame {
        let games = scheduled::all(self.app.pool()).await.unwrap();
        assert_eq!(games.len(), 1);
        games[0].clone()
    }

    async fn queue(&self, queue: NewQueue) -> ScheduledGame {
        let game = scheduled::create(&self.app.state, &queue, &actor("111"))
            .await
            .unwrap();
        scheduled::attach_message(self.app.pool(), game.id, "555")
            .await
            .unwrap()
    }

    fn edits(&self) -> Vec<MessagePayload> {
        self.api
            .take()
            .into_iter()
            .filter_map(|call| match call {
                Call::Edit(channel, message, payload) => {
                    assert_eq!((channel.as_str(), message.as_str()), ("222", "555"));
                    Some(payload)
                }
                _ => None,
            })
            .collect()
    }

    fn creates(&self) -> Vec<MessagePayload> {
        self.api
            .take()
            .into_iter()
            .filter_map(|call| match call {
                Call::Create(channel, payload) => {
                    assert_eq!(channel, "222");
                    Some(payload)
                }
                _ => None,
            })
            .collect()
    }
}

fn scheduler(
    app: &TestApp,
    api: &Arc<RecordingApi>,
    clock: &Arc<Mutex<UtcDateTime>>,
) -> Arc<NewGameScheduler> {
    let clock = Arc::clone(clock);
    Arc::new(NewGameScheduler::with_clock(
        app.state.clone(),
        api.clone(),
        Arc::new(move || *clock.lock().unwrap()),
    ))
}

async fn setup() -> Ctx {
    setup_with_role(None).await
}

async fn setup_with_role(role: Option<&str>) -> Ctx {
    let app = TestApp::with_config(|config| {
        config.discord_bot = Some(DiscordBotConfig {
            token: "test-token".into(),
            guild_id: Some("333".into()),
            spellbot_user_id: "725510263251402832".into(),
        });
        config.discord_default_timezone = "America/New_York".into();
        config.discord_newgame_role_id = role.map(str::to_owned);
    })
    .await;
    let api = RecordingApi::new();
    let clock = Arc::new(Mutex::new(now()));
    let scheduler = scheduler(&app, &api, &clock);
    scheduler.sweep().await;
    Ctx {
        app,
        api,
        clock,
        scheduler,
    }
}

fn actor(id: &str) -> QueueActor {
    QueueActor {
        discord_id: id.into(),
        display_name: format!("Player {id}"),
        guild_id: "333".into(),
        channel_id: "222".into(),
        message_id: "555".into(),
        admin: false,
    }
}

fn event(data: InteractionData, user: &str, permissions: Option<u64>) -> Interaction {
    Interaction {
        id: "777".into(),
        application_id: "888".into(),
        token: "test-only-token".into(),
        guild_id: Some("333".into()),
        channel_id: Some("222".into()),
        user: Some(InteractionUser {
            id: user.into(),
            username: Some("Name".into()),
            global_name: None,
        }),
        member: Some(InteractionMember {
            nick: Some("Nickname".into()),
            permissions,
            roles: Vec::new(),
        }),
        message_id: Some("555".into()),
        data,
    }
}

fn newgame(options: Vec<(&str, OptionValue)>) -> Interaction {
    event(
        InteractionData::Command(CommandData {
            name: "newgame".into(),
            options: options
                .into_iter()
                .map(|(name, value)| CommandOptionData {
                    name: name.into(),
                    value,
                })
                .collect(),
        }),
        "111",
        None,
    )
}

fn button(id: i64, action: &str, user: &str, permissions: Option<u64>) -> Interaction {
    event(
        InteractionData::Component(ComponentData {
            custom_id: format!("newgame:{id}:{action}"),
            values: Vec::new(),
        }),
        user,
        permissions,
    )
}

fn time_submit(id: i64, user: &str, value: &str, permissions: Option<u64>) -> Interaction {
    event(
        InteractionData::Modal(ModalData {
            custom_id: format!("newgame:{id}:time"),
            fields: vec![("start".into(), value.into())],
        }),
        user,
        permissions,
    )
}

fn text(value: &str) -> OptionValue {
    OptionValue::String(value.into())
}

fn embed(payload: &MessagePayload) -> &Embed {
    &payload.embeds.as_ref().unwrap()[0]
}

fn buttons(payload: &MessagePayload) -> Vec<Button> {
    payload
        .row_components()
        .into_iter()
        .filter_map(|component| match component {
            Component::Button(button) => Some(button.clone()),
            _ => None,
        })
        .collect()
}

fn keys(roster: &scheduled::Roster) -> Vec<&str> {
    roster.keys().map(String::as_str).collect()
}

fn mentions(payload: &MessagePayload) -> serde_json::Value {
    serde_json::to_value(&payload.allowed_mentions).unwrap()
}

fn response(call: Call) -> InteractionResponse {
    match call {
        Call::Response(response) => response,
        other => panic!("expected a response, got {other:?}"),
    }
}

fn edit_response(call: Call) -> MessagePayload {
    match call {
        Call::EditResponse(payload) => payload,
        other => panic!("expected an edit_response, got {other:?}"),
    }
}

#[tokio::test]
async fn public_command_persists_defaults_time_title_format_and_message_identity() {
    let ctx = setup().await;
    // Without thread permissions the `/newgame` reply itself is the queue.
    ctx.api.refuse(&[Op::StartThread]);
    let interaction = newgame(vec![
        ("start", text("tomorrow 7pm")),
        ("title", text("Wednesday pod")),
        ("format", text("Pauper")),
    ]);
    ctx.respond(&interaction).await.unwrap();
    let ack = response(ctx.api.next());
    assert_eq!(serde_json::to_value(&ack).unwrap(), json!({"type": 5}));
    let placeholder = edit_response(ctx.api.next());
    assert_eq!(placeholder.content.as_deref(), Some("Preparing your game…"));
    assert!(placeholder.components.is_none());
    let edits = ctx.edits();
    let edit = &edits[0];
    assert_eq!(edit.content.as_deref(), Some(""));
    assert_eq!(mentions(edit), json!({"parse": []}));
    let embed = embed(edit);
    assert_eq!(embed.title, "Wednesday pod");
    assert_eq!(embed.fields[0].value, "<t:1790290800:F> (<t:1790290800:R>)");
    assert_eq!(embed.fields[2].value, "Pauper");
    let game = ctx.only_game().await;
    assert_eq!(
        (
            game.guild_id.as_str(),
            game.channel_id.as_str(),
            game.message_id.as_deref(),
            game.host_discord_id.as_str()
        ),
        ("333", "222", Some("555"), "111")
    );
    assert_eq!(game.start_at, Some(utc("2026-09-24T23:00:00Z")));
    assert_eq!(game.min_players, 3);
    assert!(game.players.is_empty());
}

#[tokio::test]
async fn default_command_succeeds_and_database_failures_are_not_reported_as_invalid_options() {
    let (_guard, logs) = support::capture_logs();
    let ctx = setup().await;
    ctx.respond(&newgame(vec![])).await.unwrap();
    assert_eq!(
        response(ctx.api.next()).kind,
        ResponseKind::DeferredChannelMessage
    );
    let game = ctx.only_game().await;
    assert_eq!(game.title, "Commander game");
    assert_eq!(game.min_players, 3);
    assert_eq!(game.format, None);
    assert_eq!(game.start_at, None);
    ctx.api.take();

    sqlx::raw_sql(
        "CREATE TRIGGER fail_newgame BEFORE INSERT ON discord_scheduled_games
         BEGIN SELECT RAISE(ABORT, 'test storage failure'); END;",
    )
    .execute(ctx.app.pool())
    .await
    .unwrap();
    logs.clear();
    ctx.respond(&newgame(vec![])).await.unwrap();
    let reply = response(ctx.api.next());
    assert_eq!(reply.kind, ResponseKind::ChannelMessage);
    let message = reply.message_data().unwrap();
    assert_eq!(message.flags, Some(64));
    assert_eq!(
        message.content.as_deref(),
        Some("The game could not be saved because of a server error. Please try again later.")
    );
    let log = logs.contents();
    assert!(
        log.contains("Discord newgame database operation failed"),
        "{log}"
    );
    assert!(log.contains("test storage failure"), "{log}");
    assert!(!log.contains("test-only-token"));
    assert_eq!(ctx.only_game().await.id, game.id);
    assert!(ctx.api.is_idle());
}

#[tokio::test]
async fn invalid_time_bounds_dms_and_foreign_guilds_fail_privately_without_creating_queues() {
    let ctx = setup().await;
    let mut dm = newgame(vec![]);
    dm.guild_id = None;
    let mut foreign = newgame(vec![]);
    foreign.guild_id = Some("999".into());
    let invalid = [
        newgame(vec![("start", text("yesterday"))]),
        newgame(vec![("start", text("<t:1>"))]),
        newgame(vec![("min_players", OptionValue::Integer(11))]),
        newgame(vec![("min_players", OptionValue::Integer(1))]),
        dm,
        foreign,
    ];
    for event in &invalid {
        ctx.respond(event).await.unwrap();
        let reply = response(ctx.api.next());
        assert_eq!(reply.kind, ResponseKind::ChannelMessage);
        assert_eq!(reply.message_data().unwrap().flags, Some(64));
    }
    assert!(scheduled::all(ctx.app.pool()).await.unwrap().is_empty());
    assert!(ctx.api.is_idle());
}

#[tokio::test]
async fn join_and_leave_edit_original_roster_repeat_join_updates_name_without_duplication() {
    let ctx = setup().await;
    let game = ctx
        .queue(NewQueue {
            min_players: Some(4),
            ..NewQueue::default()
        })
        .await;
    let first = ctx
        .act(game.id, QueueAction::Join, &actor("11"))
        .await
        .unwrap();
    let edits = ctx.edits();
    assert_eq!(
        embed(&edits[0]).fields.last().unwrap(),
        &EmbedField::new("Players (1/10)", "<@11>")
    );
    ctx.set_now(plus(now(), 30));
    let renamed = QueueActor {
        display_name: "New name".into(),
        ..actor("11")
    };
    let second = ctx.act(game.id, QueueAction::Join, &renamed).await.unwrap();
    assert_eq!(second.players.len(), 1);
    assert_eq!(
        second.players["11"].joined_at,
        first.players["11"].joined_at
    );
    assert_eq!(second.players["11"].display_name, "New name");
    ctx.act(game.id, QueueAction::Join, &actor("12"))
        .await
        .unwrap();
    let left = ctx
        .act(game.id, QueueAction::Leave, &actor("11"))
        .await
        .unwrap();
    assert_eq!(keys(&left.players), ["12"]);
    let again = ctx
        .act(game.id, QueueAction::Leave, &actor("11"))
        .await
        .unwrap();
    assert_eq!(
        (&again.players, &again.maybe, again.status),
        (&left.players, &left.maybe, left.status)
    );
    let edits = ctx.edits();
    assert_eq!(embed(edits.last().unwrap()).fields[3].value, "<@12>");
}

#[tokio::test]
async fn ten_player_cap_permits_repeat_joins_and_frees_a_seat_on_leave() {
    let ctx = setup().await;
    let game = ctx
        .queue(NewQueue {
            start_at: Some(plus(now(), 3600)),
            ..NewQueue::default()
        })
        .await;
    for id in 1..=10 {
        ctx.act(game.id, QueueAction::Join, &actor(&id.to_string()))
            .await
            .unwrap();
    }
    assert!(matches!(
        ctx.act(game.id, QueueAction::Join, &actor("11")).await,
        Err(QueueError::Full)
    ));
    let full = ctx
        .act(game.id, QueueAction::Join, &actor("3"))
        .await
        .unwrap();
    assert_eq!(full.players.len(), 10);
    assert_eq!(full.status, Status::Open);
    ctx.act(game.id, QueueAction::Leave, &actor("3"))
        .await
        .unwrap();
    let full = ctx
        .act(game.id, QueueAction::Join, &actor("11"))
        .await
        .unwrap();
    assert_eq!(full.players.len(), 10);
}

#[tokio::test]
async fn minimum_met_join_starts_immediately_once_and_mentions_only_the_joined_players() {
    let ctx = setup().await;
    let game = ctx
        .queue(NewQueue {
            min_players: Some(2),
            ..NewQueue::default()
        })
        .await;
    let open = ctx
        .act(game.id, QueueAction::Join, &actor("11"))
        .await
        .unwrap();
    assert_eq!(open.status, Status::Open);
    assert!(ctx.creates().is_empty());
    let started = ctx
        .act(game.id, QueueAction::Join, &actor("12"))
        .await
        .unwrap();
    assert_eq!(started.status, Status::Started);
    let room = started.room_id.clone().unwrap();
    assert!(uuid::Uuid::parse_str(&room).is_ok());
    let calls = ctx.api.take();
    let Call::Create(channel, payload) = &calls[0] else {
        panic!("{calls:?}")
    };
    assert_eq!(channel, "222");
    assert!(payload.content.as_deref().unwrap().contains(&format!(
        "{}/table/{room}",
        ctx.app.state.config.public_url()
    )));
    assert_eq!(
        mentions(payload),
        json!({"parse": [], "users": ["11", "12"]})
    );
    assert_eq!(payload.enforce_nonce, Some(true));
    assert_eq!(payload.nonce, Some(format!("newgame:{}", game.id)));
    let Call::Edit(_, _, edit) = &calls[1] else {
        panic!("{calls:?}")
    };
    assert!(embed(edit).description.starts_with("Your game is ready!"));
    assert!(
        buttons(edit)
            .iter()
            .all(|button| button.disabled == Some(true))
    );

    let repeated = ctx
        .act(game.id, QueueAction::Leave, &actor("11"))
        .await
        .unwrap();
    assert_eq!(repeated.room_id, started.room_id);
    assert_eq!(repeated.players, started.players);
    ctx.scheduler.sweep().await;
    assert!(ctx.creates().is_empty());
}

#[tokio::test]
async fn scheduled_queue_starts_at_the_boundary_but_not_a_second_earlier() {
    let ctx = setup().await;
    let due = plus(now(), 60);
    let game = ctx
        .queue(NewQueue {
            start_at: Some(due),
            min_players: Some(2),
            ..NewQueue::default()
        })
        .await;
    for id in ["11", "12"] {
        ctx.act(game.id, QueueAction::Join, &actor(id))
            .await
            .unwrap();
    }
    ctx.set_now(plus(due, -1));
    ctx.scheduler.sweep().await;
    assert_eq!(ctx.game(game.id).await.status, Status::Open);
    assert!(ctx.creates().is_empty());
    ctx.set_now(due);
    ctx.scheduler.sweep().await;
    assert_eq!(ctx.game(game.id).await.status, Status::Started);
    assert_eq!(ctx.creates().len(), 1);
}

#[tokio::test]
async fn underfilled_queues_expire_at_deadline_and_cannot_accept_a_late_final_join() {
    let ctx = setup().await;
    let due = plus(now(), 60);
    let game = ctx
        .queue(NewQueue {
            start_at: Some(due),
            min_players: Some(2),
            ..NewQueue::default()
        })
        .await;
    ctx.act(game.id, QueueAction::Join, &actor("11"))
        .await
        .unwrap();
    ctx.api.take();
    ctx.set_now(due);
    let expired = ctx
        .act(game.id, QueueAction::Join, &actor("12"))
        .await
        .unwrap();
    assert_eq!(expired.status, Status::Expired);
    assert_eq!(keys(&expired.players), ["11"]);
    assert_eq!(expired.room_id, None);
    let calls = ctx.api.take();
    assert_eq!(calls.len(), 1);
    let Call::Edit(_, _, edit) = &calls[0] else {
        panic!("{calls:?}")
    };
    assert!(
        embed(edit)
            .description
            .starts_with("This game did not fill")
    );
}

#[tokio::test]
async fn maybe_does_not_count_toward_the_minimum_and_moves_between_lists() {
    let ctx = setup().await;
    let game = ctx
        .queue(NewQueue {
            min_players: Some(2),
            ..NewQueue::default()
        })
        .await;
    ctx.act(game.id, QueueAction::Join, &actor("11"))
        .await
        .unwrap();
    let maybe = ctx
        .act(game.id, QueueAction::Maybe, &actor("12"))
        .await
        .unwrap();
    assert_eq!(maybe.status, Status::Open);
    assert_eq!(keys(&maybe.players), ["11"]);
    assert_eq!(keys(&maybe.maybe), ["12"]);
    let edits = ctx.edits();
    assert_eq!(edits.len(), 2);
    assert_eq!(
        embed(&edits[1]).fields.last().unwrap(),
        &EmbedField::new("Maybe (1) — not counted", "<@12>")
    );

    let switched = ctx
        .act(game.id, QueueAction::Maybe, &actor("11"))
        .await
        .unwrap();
    assert!(switched.players.is_empty());
    assert_eq!(keys(&switched.maybe), ["11", "12"]);
    let left = ctx
        .act(game.id, QueueAction::Leave, &actor("11"))
        .await
        .unwrap();
    assert_eq!(keys(&left.maybe), ["12"]);
    let joined = ctx
        .act(game.id, QueueAction::Join, &actor("12"))
        .await
        .unwrap();
    assert!(joined.maybe.is_empty());
    assert_eq!(joined.status, Status::Open);
    for id in 1..=10 {
        ctx.act(game.id, QueueAction::Maybe, &actor(&format!("m{id}")))
            .await
            .unwrap();
    }
    assert!(matches!(
        ctx.act(game.id, QueueAction::Maybe, &actor("m11")).await,
        Err(QueueError::MaybeFull)
    ));
}

#[tokio::test]
async fn underfilled_game_pings_maybes_once_at_start_then_starts_when_a_maybe_joins() {
    let ctx = setup().await;
    let due = plus(now(), 60);
    let game = ctx
        .queue(NewQueue {
            start_at: Some(due),
            min_players: Some(2),
            title: Some("Friday pod".into()),
            ..NewQueue::default()
        })
        .await;
    ctx.act(game.id, QueueAction::Join, &actor("11"))
        .await
        .unwrap();
    ctx.act(game.id, QueueAction::Maybe, &actor("13"))
        .await
        .unwrap();
    ctx.act(game.id, QueueAction::Maybe, &actor("12"))
        .await
        .unwrap();
    ctx.api.take();
    ctx.set_now(due);
    ctx.scheduler.sweep().await;

    let pinged = ctx.game(game.id).await;
    assert_eq!(pinged.status, Status::Open);
    assert_eq!(pinged.maybe_pinged_at, Some(due));
    assert_eq!(pinged.maybe_ping_id.as_deref(), Some("999"));
    assert!(!pinged.message_dirty);

    let calls = ctx.api.take();
    let Call::Create(_, payload) = &calls[0] else {
        panic!("{calls:?}")
    };
    assert_eq!(
        mentions(payload),
        json!({"parse": [], "users": ["12", "13"]})
    );
    let content = payload.content.as_deref().unwrap();
    assert!(content.contains("<@12> <@13> **Friday pod** is 1 player short"));
    assert!(content.contains("https://discord.com/channels/333/222/555"));
    let nonce = payload.nonce.clone().unwrap();
    assert_eq!(nonce, format!("ngm:{}:{}", game.id, due.unix()));
    assert!(nonce.chars().count() <= 25);
    let Call::Edit(_, _, edit) = &calls[1] else {
        panic!("{calls:?}")
    };
    assert!(embed(edit).description.starts_with("Short of the minimum"));

    ctx.set_now(plus(due, 60));
    ctx.scheduler.sweep().await;
    assert!(ctx.creates().is_empty());

    let started = ctx
        .act(game.id, QueueAction::Join, &actor("12"))
        .await
        .unwrap();
    assert_eq!(started.status, Status::Started);
    let creates = ctx.creates();
    assert_eq!(
        mentions(&creates[0]),
        json!({"parse": [], "users": ["11", "12"]})
    );
}

#[tokio::test]
async fn maybe_grace_period_expires_an_unfilled_game_changing_time_re_arms_the_ping() {
    let ctx = setup().await;
    let due = plus(now(), 60);
    let game = ctx
        .queue(NewQueue {
            start_at: Some(due),
            min_players: Some(2),
            ..NewQueue::default()
        })
        .await;
    ctx.act(game.id, QueueAction::Maybe, &actor("12"))
        .await
        .unwrap();
    ctx.set_now(due);
    ctx.scheduler.sweep().await;
    assert_eq!(ctx.creates().len(), 1);

    let later = plus(due, 3600);
    let moved = ctx
        .act(game.id, QueueAction::Time(Some(later)), &actor("111"))
        .await
        .unwrap();
    assert_eq!((moved.maybe_pinged_at, moved.maybe_ping_id), (None, None));

    ctx.set_now(later);
    ctx.scheduler.sweep().await;
    let creates = ctx.creates();
    assert_eq!(
        creates[0].nonce,
        Some(format!("ngm:{}:{}", game.id, later.unix()))
    );

    ctx.set_now(plus(later, MAYBE_GRACE_SECONDS - 1));
    ctx.scheduler.sweep().await;
    assert_eq!(ctx.game(game.id).await.status, Status::Open);

    ctx.set_now(plus(later, MAYBE_GRACE_SECONDS));
    ctx.scheduler.sweep().await;
    assert_eq!(ctx.game(game.id).await.status, Status::Expired);
    let calls = ctx.api.take();
    assert!(!calls.iter().any(|call| matches!(call, Call::Create(..))));
    let Some(Call::Edit(_, _, edit)) = calls.last() else {
        panic!("{calls:?}")
    };
    assert!(
        embed(edit)
            .description
            .starts_with("This game did not fill")
    );
}

#[tokio::test]
async fn maybe_button_confirms_privately() {
    let ctx = setup().await;
    let game = ctx
        .queue(NewQueue {
            start_at: Some(plus(now(), 3600)),
            ..NewQueue::default()
        })
        .await;
    ctx.respond(&button(game.id, "maybe", "111", None))
        .await
        .unwrap();
    assert_eq!(
        response(ctx.api.next()),
        InteractionResponse::deferred(true)
    );
    let calls = ctx.api.take();
    let confirmation = calls
        .into_iter()
        .find_map(|call| match call {
            Call::EditResponse(payload) => Some(payload),
            _ => None,
        })
        .unwrap();
    assert!(
        confirmation
            .content
            .unwrap()
            .starts_with("You are on the maybe list. You'll be pinged")
    );
    assert_eq!(keys(&ctx.game(game.id).await.maybe), ["111"]);
}

#[tokio::test]
async fn boot_reloads_due_work_from_db_and_expires_underfilled_queues() {
    let mut ctx = setup().await;
    let game = ctx
        .queue(NewQueue {
            start_at: Some(plus(now(), -1)),
            ..NewQueue::default()
        })
        .await;
    ctx.restart().await;
    assert_eq!(ctx.game(game.id).await.status, Status::Expired);
    let edits = ctx.edits();
    assert!(
        buttons(&edits[0])
            .iter()
            .all(|button| button.disabled == Some(true))
    );
}

#[tokio::test]
async fn atomic_status_guard_does_not_replace_the_uuid_when_given_the_same_stale_open_row() {
    let ctx = setup().await;
    let game = ctx
        .queue(NewQueue {
            start_at: Some(now()),
            min_players: Some(2),
            ..NewQueue::default()
        })
        .await;
    let players = json!({
        "11": {"display_name": "Player 11", "joined_at": NOW},
        "12": {"display_name": "Player 12", "joined_at": NOW},
    });
    sqlx::query("UPDATE discord_scheduled_games SET players = ? WHERE id = ?")
        .bind(players.to_string())
        .bind(game.id)
        .execute(ctx.app.pool())
        .await
        .unwrap();
    let stale = ctx.game(game.id).await;
    let mut conn = ctx.app.pool().acquire().await.unwrap();
    let first = scheduled::settle(&mut conn, stale.clone(), now())
        .await
        .unwrap();
    let second = scheduled::settle(&mut conn, stale, now()).await.unwrap();
    drop(conn);
    assert_eq!(first.status, Status::Started);
    assert_eq!(second.room_id, first.room_id);
    assert_eq!(ctx.game(game.id).await.room_id, first.room_id);
}

#[tokio::test]
async fn host_cancellation_disables_buttons_and_never_starts_others_cannot_cancel() {
    let ctx = setup().await;
    let game = ctx.queue(NewQueue::default()).await;
    assert!(matches!(
        ctx.act(game.id, QueueAction::Cancel, &actor("12")).await,
        Err(QueueError::Forbidden)
    ));
    let cancelled = ctx
        .act(game.id, QueueAction::Cancel, &actor("111"))
        .await
        .unwrap();
    assert_eq!(cancelled.status, Status::Cancelled);
    let edits = ctx.edits();
    assert!(
        buttons(&edits[0])
            .iter()
            .all(|button| button.disabled == Some(true))
    );
    let unchanged = ctx
        .act(game.id, QueueAction::Join, &actor("12"))
        .await
        .unwrap();
    assert!(unchanged.players.is_empty());
    assert!(ctx.creates().is_empty());
}

#[tokio::test]
async fn administrator_permission_can_cancel_manage_guild_alone_cannot() {
    let ctx = setup().await;
    let game = ctx.queue(NewQueue::default()).await;
    for (permissions, expected) in [
        (MANAGE_GUILD, Status::Open),
        (ADMINISTRATOR, Status::Cancelled),
    ] {
        ctx.respond(&button(game.id, "cancel", "12", Some(permissions)))
            .await
            .unwrap();
        assert_eq!(ctx.game(game.id).await.status, expected);
        assert_eq!(
            response(ctx.api.next()),
            InteractionResponse::deferred(true)
        );
        let calls = ctx.api.take();
        let confirmation = calls
            .into_iter()
            .find_map(|call| match call {
                Call::EditResponse(payload) => Some(payload),
                _ => None,
            })
            .unwrap();
        assert_eq!(confirmation.flags, Some(64));
    }
}

#[tokio::test]
async fn button_acknowledgements_are_private_and_mismatched_message_channel_guild_are_rejected() {
    let ctx = setup().await;
    let game = ctx.queue(NewQueue::default()).await;
    ctx.respond(&button(game.id, "join", "111", None))
        .await
        .unwrap();
    assert_eq!(
        response(ctx.api.next()),
        InteractionResponse::deferred(true)
    );
    let confirmation = ctx
        .api
        .take()
        .into_iter()
        .find_map(|call| match call {
            Call::EditResponse(payload) => Some(payload),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        confirmation.content.as_deref(),
        Some("You are on the roster.")
    );
    assert_eq!(confirmation.flags, Some(64));

    for actor in [
        QueueActor {
            guild_id: "999".into(),
            ..actor("12")
        },
        QueueActor {
            channel_id: "999".into(),
            ..actor("12")
        },
        QueueActor {
            message_id: "999".into(),
            ..actor("12")
        },
    ] {
        assert!(matches!(
            ctx.act(game.id, QueueAction::Join, &actor).await,
            Err(QueueError::Forbidden)
        ));
    }
    assert_eq!(keys(&ctx.game(game.id).await.players), ["111"]);
}

#[tokio::test]
async fn failed_acknowledgement_is_not_retried_or_published() {
    let ctx = setup().await;
    ctx.api.fail(&[Op::Response]);
    assert!(matches!(
        ctx.respond(&newgame(vec![])).await,
        Err(RespondError::DeliveryFailed)
    ));
    assert_eq!(ctx.only_game().await.status, Status::Cancelled);
    let calls = ctx.api.take();
    assert_eq!(calls.len(), 1);
    assert_eq!(
        response(calls[0].clone()).kind,
        ResponseKind::DeferredChannelMessage
    );
}

#[tokio::test]
async fn restart_retries_failed_notifications_with_same_room_and_records_announcement_before_edit()
{
    let mut ctx = setup().await;
    let game = ctx
        .queue(NewQueue {
            min_players: Some(2),
            ..NewQueue::default()
        })
        .await;
    ctx.act(game.id, QueueAction::Join, &actor("11"))
        .await
        .unwrap();
    ctx.api.take();
    ctx.api.fail(&[Op::Create]);
    ctx.act(game.id, QueueAction::Join, &actor("12"))
        .await
        .unwrap();
    let first = ctx.creates().remove(0);
    let room = ctx.game(game.id).await.room_id;

    ctx.api.fail(&[Op::Edit]);
    ctx.restart().await;
    assert_eq!(ctx.creates(), [first]);
    let stored = ctx.game(game.id).await;
    assert_eq!(stored.announcement_id.as_deref(), Some("999"));
    assert!(stored.message_dirty);

    ctx.scheduler.sweep().await;
    assert!(ctx.creates().is_empty());
    let last = ctx.game(game.id).await;
    assert_eq!(last.room_id, room);
    assert!(!last.message_dirty);
}

#[tokio::test]
async fn queue_rendering_suppresses_free_text_mentions() {
    let ctx = setup().await;
    let game = ctx
        .queue(NewQueue {
            title: Some("@everyone".into()),
            format: Some("<@&444>".into()),
            ..NewQueue::default()
        })
        .await;
    let payload = new_game_message::render(ctx.app.state.config.public_url(), &game);
    assert_eq!(mentions(&payload), json!({"parse": []}));
}

#[tokio::test]
async fn first_public_queue_edit_can_be_recovered_after_restart() {
    let mut ctx = setup().await;
    // Without thread permissions the `/newgame` reply itself is the queue.
    ctx.api.refuse(&[Op::StartThread]);
    ctx.api.fail(&[Op::Edit]);
    ctx.respond(&newgame(vec![])).await.unwrap();
    let game = ctx.only_game().await;
    assert_eq!(game.message_id.as_deref(), Some("555"));
    assert!(game.message_dirty);
    assert_eq!(ctx.edits().len(), 1);
    ctx.restart().await;
    assert!(!ctx.game(game.id).await.message_dirty);
    let edits = ctx.edits();
    let buttons = buttons(&edits[0]);
    let labels: Vec<&str> = buttons.iter().map(|button| button.label.as_str()).collect();
    assert_eq!(labels, ["Join", "Maybe", "Leave", "Change time", "Cancel"]);
    assert!(buttons.iter().all(|button| button.disabled == Some(false)));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_joins_cannot_overfill_a_scheduled_queue() {
    let ctx = setup().await;
    let game = ctx
        .queue(NewQueue {
            start_at: Some(plus(now(), 3600)),
            ..NewQueue::default()
        })
        .await;
    let tasks: Vec<_> = (1..=11)
        .map(|id| {
            let scheduler = Arc::clone(&ctx.scheduler);
            tokio::spawn(async move {
                scheduler
                    .act(game.id, &QueueAction::Join, &actor(&id.to_string()))
                    .await
            })
        })
        .collect();
    let mut results = Vec::new();
    for task in tasks {
        results.push(task.await.unwrap());
    }
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 10);
    assert_eq!(
        results
            .iter()
            .filter(|result| matches!(result, Err(QueueError::Full)))
            .count(),
        1
    );
    assert_eq!(ctx.game(game.id).await.players.len(), 10);
}

#[tokio::test]
async fn host_changes_the_start_time_through_a_modal_others_cannot_open_it() {
    let ctx = setup().await;
    let game = ctx
        .queue(NewQueue {
            start_at: Some(plus(now(), 3600)),
            ..NewQueue::default()
        })
        .await;
    ctx.respond(&button(game.id, "time", "12", None))
        .await
        .unwrap();
    let refused = response(ctx.api.next());
    assert_eq!(refused.kind, ResponseKind::ChannelMessage);
    assert_eq!(refused.message_data().unwrap().flags, Some(64));
    assert!(refused.content().starts_with("Use this game's"));

    ctx.respond(&button(game.id, "time", "111", None))
        .await
        .unwrap();
    let opened = response(ctx.api.next());
    assert_eq!(opened.kind, ResponseKind::Modal);
    assert_eq!(
        opened.modal_data().unwrap().custom_id,
        format!("newgame:{}:time", game.id)
    );

    ctx.respond(&time_submit(game.id, "111", "tomorrow 7pm", None))
        .await
        .unwrap();
    assert_eq!(
        response(ctx.api.next()),
        InteractionResponse::deferred(true)
    );
    let calls = ctx.api.take();
    let confirmation = calls
        .iter()
        .find_map(|call| match call {
            Call::EditResponse(payload) => Some(payload.clone()),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        confirmation.content.as_deref(),
        Some("The game now starts <t:1790290800:F>.")
    );
    assert_eq!(
        ctx.game(game.id).await.start_at,
        Some(utc("2026-09-24T23:00:00Z"))
    );
    let edit = calls
        .iter()
        .find_map(|call| match call {
            Call::Edit(_, _, payload) => Some(payload.clone()),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        embed(&edit).fields[0].value,
        "<t:1790290800:F> (<t:1790290800:R>)"
    );

    ctx.respond(&time_submit(game.id, "111", "yesterday", None))
        .await
        .unwrap();
    let rejected = response(ctx.api.next());
    assert_eq!(rejected.kind, ResponseKind::ChannelMessage);
    assert_eq!(rejected.message_data().unwrap().flags, Some(64));

    assert!(matches!(
        ctx.act(game.id, QueueAction::Time(None), &actor("12"))
            .await,
        Err(QueueError::Forbidden)
    ));
    assert_eq!(
        ctx.game(game.id).await.start_at,
        Some(utc("2026-09-24T23:00:00Z"))
    );
}

/// Discord computes every permission for the guild owner.
const OWNER: u64 = u64::MAX;

#[tokio::test]
async fn clearing_the_start_time_starts_a_filled_queue_immediately() {
    let ctx = setup().await;
    let game = ctx
        .queue(NewQueue {
            start_at: Some(plus(now(), 3600)),
            min_players: Some(2),
            ..NewQueue::default()
        })
        .await;
    for id in ["11", "12"] {
        ctx.act(game.id, QueueAction::Join, &actor(id))
            .await
            .unwrap();
    }
    ctx.api.take();
    ctx.respond(&time_submit(game.id, "42", " ", Some(OWNER)))
        .await
        .unwrap();
    let calls = ctx.api.take();
    let confirmation = calls
        .iter()
        .find_map(|call| match call {
            Call::EditResponse(payload) => Some(payload.clone()),
            _ => None,
        })
        .unwrap();
    assert!(
        confirmation
            .content
            .unwrap()
            .starts_with("Your game is ready!")
    );
    let stored = ctx.game(game.id).await;
    assert_eq!((stored.status, stored.start_at), (Status::Started, None));
    assert!(calls.iter().any(|call| matches!(call, Call::Create(..))));
}

#[tokio::test]
async fn guild_owner_can_cancel_without_an_explicit_administrator_role() {
    let ctx = setup().await;
    let game = ctx.queue(NewQueue::default()).await;
    ctx.respond(&button(game.id, "cancel", "42", Some(OWNER)))
        .await
        .unwrap();
    assert_eq!(ctx.game(game.id).await.status, Status::Cancelled);
}

#[tokio::test]
async fn invalid_buttons_are_rejected_privately() {
    let ctx = setup().await;
    let mut event = button(1, "join", "111", None);
    event.data = InteractionData::Component(ComponentData {
        custom_id: "newgame:0:join".into(),
        values: Vec::new(),
    });
    ctx.respond(&event).await.unwrap();
    assert_eq!(
        response(ctx.api.next()).content(),
        "This game button is invalid."
    );
}

fn close_in(channel: &str, user: &str, permissions: Option<u64>) -> Interaction {
    let mut event = event(
        InteractionData::Command(CommandData {
            name: "close".into(),
            options: Vec::new(),
        }),
        user,
        permissions,
    );
    event.channel_id = Some(channel.into());
    event.message_id = None;
    event
}

fn thread_members(calls: &[Call]) -> Vec<(&str, &str)> {
    calls
        .iter()
        .filter_map(|call| match call {
            Call::AddThreadMember(thread, user) => Some((thread.as_str(), user.as_str())),
            _ => None,
        })
        .collect()
}

fn post_channels(calls: &[Call]) -> Vec<&str> {
    calls
        .iter()
        .filter_map(|call| match call {
            Call::Create(channel, _) => Some(channel.as_str()),
            _ => None,
        })
        .collect()
}

/// A player clicking the queue in its thread (thread 555, queue message 999).
fn in_thread(id: &str) -> QueueActor {
    QueueActor {
        channel_id: "555".into(),
        message_id: "999".into(),
        ..actor(id)
    }
}

#[tokio::test]
async fn newgame_moves_its_queue_into_a_thread_on_the_reply_where_players_sign_up() {
    let ctx = setup().await;
    // 10pm in New York on September 24 is already September 25 in UTC.
    ctx.respond(&newgame(vec![
        ("title", text("Friday pod")),
        ("start", text("<t:1790301600:F>")),
        ("min_players", OptionValue::Integer(2)),
    ]))
    .await
    .unwrap();
    let calls = ctx.api.take();
    assert_eq!(calls.len(), 7, "{calls:?}");
    assert_eq!(
        edit_response(calls[1].clone()).content.as_deref(),
        Some("Preparing your game…")
    );
    assert_eq!(
        calls[2],
        Call::StartThread("222".into(), "555".into(), "Friday pod · Sep 24".into())
    );
    let Call::Create(thread, placeholder) = &calls[3] else {
        panic!("{calls:?}")
    };
    assert_eq!(thread, "555");
    assert!(placeholder.components.is_none());
    assert_eq!(calls[4], Call::AddThreadMember("555".into(), "111".into()));
    let Call::Edit(channel, message, queue) = &calls[5] else {
        panic!("{calls:?}")
    };
    assert_eq!((channel.as_str(), message.as_str()), ("555", "999"));
    assert_eq!(buttons(queue).len(), 5);
    let Call::Edit(channel, message, pointer) = &calls[6] else {
        panic!("{calls:?}")
    };
    assert_eq!((channel.as_str(), message.as_str()), ("222", "555"));
    assert_eq!(
        pointer.content.as_deref(),
        Some("**Friday pod**: sign up in the thread below.")
    );
    assert!(pointer.components.is_none());
    let game = ctx.only_game().await;
    assert_eq!(
        (game.thread_id.as_deref(), game.message_id.as_deref()),
        (Some("555"), Some("999"))
    );

    // Buttons are clicked in the thread; the channel reply has none to click.
    let mut click = button(game.id, "maybe", "11", None);
    click.channel_id = Some("555".into());
    click.message_id = Some("999".into());
    ctx.respond(&click).await.unwrap();
    assert_eq!(keys(&ctx.game(game.id).await.maybe), ["11"]);
    assert!(matches!(
        ctx.act(game.id, QueueAction::Join, &actor("11")).await,
        Err(QueueError::Forbidden)
    ));
    ctx.act(game.id, QueueAction::Leave, &in_thread("11"))
        .await
        .unwrap();
    ctx.act(game.id, QueueAction::Join, &in_thread("12"))
        .await
        .unwrap();
    assert_eq!(
        thread_members(&ctx.api.take()),
        [("555", "11"), ("555", "12")]
    );

    ctx.act(game.id, QueueAction::Join, &in_thread("13"))
        .await
        .unwrap();
    ctx.api.take();
    ctx.set_now(utc("2026-09-25T02:00:00Z"));
    ctx.scheduler.sweep().await;
    assert_eq!(ctx.game(game.id).await.status, Status::Started);
    let calls = ctx.api.take();
    assert_eq!(post_channels(&calls), ["555"]);
    assert!(calls.iter().any(
        |call| matches!(call, Call::Edit(channel, message, _) if channel == "555" && message == "999")
    ));
}

#[tokio::test]
async fn queues_work_without_a_thread_and_refused_thread_posts_fall_back_to_the_channel() {
    let ctx = setup().await;
    ctx.api.refuse(&[Op::StartThread]);
    ctx.respond(&newgame(vec![("min_players", OptionValue::Integer(2))]))
        .await
        .unwrap();
    let calls = ctx.api.take();
    assert!(thread_members(&calls).is_empty());
    let game = ctx.only_game().await;
    assert_eq!(game.thread_id, None);
    assert!(!game.message_dirty);
    for id in ["11", "12"] {
        ctx.act(game.id, QueueAction::Join, &actor(id))
            .await
            .unwrap();
    }
    let calls = ctx.api.take();
    assert!(thread_members(&calls).is_empty());
    assert_eq!(post_channels(&calls), ["222"]);

    // A thread a moderator deleted or locked refuses the ping; the channel gets it once.
    let game = ctx
        .queue(NewQueue {
            min_players: Some(2),
            ..NewQueue::default()
        })
        .await;
    scheduled::set_thread(ctx.app.pool(), game.id, "555")
        .await
        .unwrap();
    scheduled::attach_message(ctx.app.pool(), game.id, "999")
        .await
        .unwrap();
    ctx.act(game.id, QueueAction::Join, &in_thread("11"))
        .await
        .unwrap();
    ctx.api.take();
    ctx.api.refuse(&[Op::Create]);
    ctx.act(game.id, QueueAction::Join, &in_thread("12"))
        .await
        .unwrap();
    assert_eq!(post_channels(&ctx.api.take()), ["555", "222"]);
    let stored = ctx.game(game.id).await;
    assert_eq!(stored.announcement_id.as_deref(), Some("999"));
    assert!(!stored.message_dirty);
}

#[tokio::test]
async fn close_archives_the_game_thread_for_its_host_or_an_administrator_and_cancels_an_open_game()
{
    let ctx = setup().await;
    ctx.respond(&newgame(vec![])).await.unwrap();
    let game = ctx.only_game().await;
    ctx.api.take();

    for (event, reply) in [
        (
            close_in("222", "111", None),
            "Use /close inside a /newgame game thread.",
        ),
        (
            close_in("555", "12", Some(MANAGE_GUILD)),
            "Only this thread's latest game host or a Discord Administrator can close it.",
        ),
    ] {
        new_game::close(&ctx.app.state, ctx.api.as_ref(), &ctx.scheduler, &event)
            .await
            .unwrap();
        let calls = ctx.api.take();
        assert_eq!(calls.len(), 1, "{calls:?}");
        let refused = response(calls[0].clone());
        assert_eq!(refused.message_data().unwrap().flags, Some(64));
        assert_eq!(refused.content(), reply);
    }
    assert_eq!(ctx.game(game.id).await.status, Status::Open);

    ctx.api.fail(&[Op::CloseThread]);
    new_game::close(
        &ctx.app.state,
        ctx.api.as_ref(),
        &ctx.scheduler,
        &close_in("555", "111", None),
    )
    .await
    .unwrap();
    let calls = ctx.api.take();
    assert_eq!(
        response(calls[0].clone()),
        InteractionResponse::deferred(false)
    );
    let closed = calls
        .iter()
        .find_map(|call| match call {
            Call::EditResponse(payload) => payload.content.clone(),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        closed,
        "<@111> closed this game thread. Its open game is cancelled."
    );
    let Some(Call::Edit(_, _, edit)) = calls.iter().find(|call| matches!(call, Call::Edit(..)))
    else {
        panic!("{calls:?}")
    };
    assert_eq!(embed(edit).description, "This game was cancelled.");
    assert!(calls.contains(&Call::CloseThread("555".into())));
    let Some(Call::EditResponse(failed)) = calls.last() else {
        panic!("{calls:?}")
    };
    assert!(
        failed
            .content
            .as_deref()
            .unwrap()
            .contains("Manage Threads")
    );
    assert_eq!(ctx.game(game.id).await.status, Status::Cancelled);

    // An administrator can retry; the game is already cancelled.
    new_game::close(
        &ctx.app.state,
        ctx.api.as_ref(),
        &ctx.scheduler,
        &close_in("555", "42", Some(ADMINISTRATOR)),
    )
    .await
    .unwrap();
    let calls = ctx.api.take();
    assert_eq!(
        edit_response(calls[1].clone()).content.as_deref(),
        Some("<@42> closed this game thread.")
    );
    assert_eq!(calls.last(), Some(&Call::CloseThread("555".into())));
}

#[tokio::test]
async fn rest_client_uses_discords_thread_and_role_routes() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v10/channels/222/messages/555/threads"))
        .and(header("authorization", "Bot bot-token"))
        .and(body_json(
            json!({"name": "Friday pod", "auto_archive_duration": 10_080}),
        ))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({"id": "555"})))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/api/v10/channels/555/thread-members/111"))
        .and(header("content-length", "0"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PATCH"))
        .and(path("/api/v10/channels/555"))
        .and(body_json(json!({"archived": true})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id": "555"})))
        .expect(1)
        .mount(&server)
        .await;
    let api = RestApi::with_base(
        "bot-token",
        &format!("{}/api/v10", server.uri()),
        std::time::Duration::from_secs(5),
    )
    .unwrap();
    for verb in ["PUT", "DELETE"] {
        Mock::given(method(verb))
            .and(path("/api/v10/guilds/333/members/111/roles/4242"))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&server)
            .await;
    }
    api.set_member_role("333", "111", "4242", true)
        .await
        .unwrap();
    api.set_member_role("333", "111", "4242", false)
        .await
        .unwrap();
    api.start_thread("222", "555", "Friday pod").await.unwrap();
    api.add_thread_member("555", "111").await.unwrap();
    api.close_thread("555").await.unwrap();
}

#[tokio::test]
async fn queues_that_started_their_thread_stay_on_the_channel_message() {
    let ctx = setup().await;
    let game = ctx.queue(NewQueue::default()).await;
    scheduled::set_thread(ctx.app.pool(), game.id, "555")
        .await
        .unwrap();
    ctx.api.take();
    ctx.act(game.id, QueueAction::Join, &actor("11"))
        .await
        .unwrap();
    assert_eq!(ctx.edits().len(), 1);
    assert!(matches!(
        ctx.act(game.id, QueueAction::Join, &in_thread("12")).await,
        Err(QueueError::Forbidden)
    ));
}

#[tokio::test]
async fn a_new_day_renames_the_thread_and_newgame_in_a_game_thread_adds_its_queue_there() {
    let ctx = setup().await;
    ctx.respond(&newgame(vec![
        ("title", text("Pod")),
        ("start", text("<t:1790301600>")),
    ]))
    .await
    .unwrap();
    let first = ctx.only_game().await;
    ctx.api.take();

    // 7pm on the same New York day keeps the name; a later day renames the thread.
    let same_day = QueueAction::Time(Some(utc("2026-09-24T23:00:00Z")));
    ctx.act(first.id, same_day, &in_thread("111"))
        .await
        .unwrap();
    let next_day = QueueAction::Time(Some(utc("2026-09-26T23:00:00Z")));
    ctx.act(first.id, next_day, &in_thread("111"))
        .await
        .unwrap();
    let renames: Vec<Call> = ctx
        .api
        .take()
        .into_iter()
        .filter(|call| matches!(call, Call::RenameThread(..)))
        .collect();
    assert_eq!(
        renames,
        [Call::RenameThread("555".into(), "Pod · Sep 26".into())]
    );

    // `/newgame` inside the thread: the reply is the new queue, in the same thread.
    let mut rematch = newgame(vec![
        ("title", text("Rematch")),
        ("start", text("<t:1790550000>")),
    ]);
    rematch.channel_id = Some("555".into());
    rematch.user.as_mut().unwrap().id = "12".into();
    ctx.api
        .set_edit_response_result(Ok(the_gathering::discord::api::SentMessage {
            id: "1001".into(),
        }));
    ctx.respond(&rematch).await.unwrap();
    let calls = ctx.api.take();
    assert!(
        !calls
            .iter()
            .any(|call| matches!(call, Call::StartThread(..) | Call::Create(..)))
    );
    assert_eq!(thread_members(&calls), [("555", "12")]);
    let edits: Vec<(&str, &str)> = calls
        .iter()
        .filter_map(|call| match call {
            Call::Edit(channel, message, _) => Some((channel.as_str(), message.as_str())),
            _ => None,
        })
        .collect();
    assert_eq!(edits, [("555", "1001")]);
    assert_eq!(
        calls.last(),
        Some(&Call::RenameThread("555".into(), "Rematch · Sep 27".into()))
    );
    let games = scheduled::all(ctx.app.pool()).await.unwrap();
    let second = &games[1];
    assert_eq!(
        (
            second.channel_id.as_str(),
            second.thread_id.as_deref(),
            second.message_id.as_deref(),
            second.host_discord_id.as_str()
        ),
        ("222", Some("555"), Some("1001"), "12")
    );
    let joiner = QueueActor {
        message_id: "1001".into(),
        ..in_thread("13")
    };
    ctx.act(second.id, QueueAction::Join, &joiner)
        .await
        .unwrap();
    assert_eq!(keys(&ctx.game(second.id).await.players), ["13"]);

    // The newest game's host closes the thread, cancelling both open games.
    new_game::close(
        &ctx.app.state,
        ctx.api.as_ref(),
        &ctx.scheduler,
        &close_in("555", "12", None),
    )
    .await
    .unwrap();
    let closed = ctx
        .api
        .take()
        .into_iter()
        .find_map(|call| match call {
            Call::EditResponse(payload) => payload.content,
            _ => None,
        })
        .unwrap();
    assert_eq!(
        closed,
        "<@12> closed this game thread. Its open games are cancelled."
    );
    for id in [first.id, second.id] {
        assert_eq!(ctx.game(id).await.status, Status::Cancelled);
    }
}

#[tokio::test]
async fn the_opt_in_role_is_mentioned_in_each_game_thread_and_nowhere_else() {
    let ctx = setup_with_role(Some("4242")).await;
    ctx.respond(&newgame(vec![
        ("title", text("Pod")),
        ("start", text("<t:1790301600>")),
    ]))
    .await
    .unwrap();
    let calls = ctx.api.take();
    let posts: Vec<(&str, &MessagePayload)> = calls
        .iter()
        .filter_map(|call| match call {
            Call::Create(channel, payload) => Some((channel.as_str(), payload)),
            _ => None,
        })
        .collect();
    // The queue's placeholder, then the role mention, both in the thread.
    assert_eq!(posts.len(), 2, "{calls:?}");
    let (channel, ping) = posts[1];
    assert_eq!(channel, "555");
    assert_eq!(
        ping.content.as_deref(),
        Some("<@&4242> New game: **Pod**, starting <t:1790301600:F>.")
    );
    assert_eq!(mentions(ping), json!({"parse": [], "roles": ["4242"]}));

    // A second game in the thread mentions the role again.
    let mut rematch = newgame(vec![("title", text("Rematch"))]);
    rematch.channel_id = Some("555".into());
    ctx.api
        .set_edit_response_result(Ok(the_gathering::discord::api::SentMessage {
            id: "1001".into(),
        }));
    ctx.respond(&rematch).await.unwrap();
    let calls = ctx.api.take();
    let Some(Call::Create(channel, ping)) =
        calls.iter().find(|call| matches!(call, Call::Create(..)))
    else {
        panic!("{calls:?}")
    };
    assert_eq!(channel, "555");
    assert_eq!(
        ping.content.as_deref(),
        Some("<@&4242> New game: **Rematch**, starting as soon as the minimum joins.")
    );

    // Without a thread there is nothing to add the role to, so it is not pinged.
    let ctx = setup_with_role(Some("4242")).await;
    ctx.api.refuse(&[Op::StartThread]);
    ctx.respond(&newgame(vec![])).await.unwrap();
    assert!(post_channels(&ctx.api.take()).is_empty());
}

fn alerts(user: &str, roles: &[&str]) -> Interaction {
    let mut event = event(
        InteractionData::Command(CommandData {
            name: "alerts".into(),
            options: Vec::new(),
        }),
        user,
        None,
    );
    event.message_id = None;
    event.member.as_mut().unwrap().roles = roles.iter().map(|role| (*role).to_owned()).collect();
    event
}

#[tokio::test]
async fn alerts_toggles_the_new_game_role_for_the_member() {
    let ctx = setup_with_role(Some("4242")).await;
    let respond = |event: Interaction| {
        let ctx = &ctx;
        async move {
            the_gathering::discord::alerts::respond(&ctx.app.state, ctx.api.as_ref(), &event)
                .await
                .unwrap();
            ctx.api.take()
        }
    };

    let calls = respond(alerts("11", &["1"])).await;
    assert_eq!(
        response(calls[0].clone()),
        InteractionResponse::deferred(true)
    );
    assert_eq!(
        calls[1],
        Call::SetMemberRole("333".into(), "11".into(), "4242".into(), true)
    );
    let reply = edit_response(calls[2].clone());
    assert_eq!(reply.flags, Some(64));
    assert!(reply.content.unwrap().starts_with("Game alerts are on"));

    let calls = respond(alerts("11", &["1", "4242"])).await;
    assert_eq!(
        calls[1],
        Call::SetMemberRole("333".into(), "11".into(), "4242".into(), false)
    );
    assert!(
        edit_response(calls[2].clone())
            .content
            .unwrap()
            .starts_with("Game alerts are off")
    );

    ctx.api.refuse(&[Op::SetMemberRole]);
    let calls = respond(alerts("11", &[])).await;
    assert!(
        edit_response(calls[2].clone())
            .content
            .unwrap()
            .contains("Manage Roles")
    );

    let unset = setup().await;
    the_gathering::discord::alerts::respond(
        &unset.app.state,
        unset.api.as_ref(),
        &alerts("11", &[]),
    )
    .await
    .unwrap();
    let calls = unset.api.take();
    assert_eq!(calls.len(), 1, "{calls:?}");
    let reply = response(calls[0].clone());
    assert_eq!(reply.message_data().unwrap().flags, Some(64));
    assert_eq!(
        reply.content(),
        "Game alerts are not set up on this server."
    );
}
