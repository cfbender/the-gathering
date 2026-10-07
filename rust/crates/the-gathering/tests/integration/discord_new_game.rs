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
use the_gathering::discord::api::{
    Button, Component, Embed, EmbedField, InteractionResponse, MessagePayload, ResponseKind,
};
use the_gathering::discord::interaction::{
    CommandData, CommandOptionData, ComponentData, Interaction, InteractionData, InteractionMember,
    InteractionUser, ModalData, OptionValue,
};
use the_gathering::discord::new_game::{self, RespondError};
use the_gathering::discord::new_game_message;
use the_gathering::discord::scheduled::{
    self, MAYBE_GRACE_SECONDS, NewQueue, QueueAction, QueueActor, QueueError, ScheduledGame, Status,
};
use the_gathering::discord::scheduler::NewGameScheduler;

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
    let app = TestApp::with_config(|config| {
        config.discord_bot = Some(DiscordBotConfig {
            token: "test-token".into(),
            guild_id: Some("333".into()),
            spellbot_user_id: "725510263251402832".into(),
        });
        config.discord_default_timezone = "America/New_York".into();
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
    let payload = new_game_message::render(&ctx.app.state.config.public_url(), &game);
    assert_eq!(mentions(&payload), json!({"parse": []}));
}

#[tokio::test]
async fn first_public_queue_edit_can_be_recovered_after_restart() {
    let mut ctx = setup().await;
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
