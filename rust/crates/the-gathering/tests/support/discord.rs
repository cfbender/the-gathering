//! Discord test doubles (`test/support/discord_new_game_api.ex` and the stub modules the
//! Elixir Discord tests passed as `api`): a recording [`DiscordApi`] whose operations can
//! be made to fail once, interaction builders, and report fixtures.

use std::collections::VecDeque;
use std::io::Write;
use std::sync::{Arc, Mutex};

use serde_json::Map;
use the_gathering::db::UtcDateTime;
use the_gathering::discord::api::{
    ApiFuture, CommandDefinition, DiscordApi, DiscordError, InteractionResponse, InteractionTarget,
    MessagePayload, RegisteredCommand, SentMessage,
};
use the_gathering::discord::interaction::{
    CommandData, CommandOptionData, ComponentData, Interaction, InteractionData, InteractionMember,
    InteractionUser, ModalData, OptionValue,
};
use the_gathering::discord::{GameReport, ReportPlayer};

/// A recorded call.
#[derive(Clone, Debug, PartialEq)]
pub enum Call {
    /// `create_response`.
    Response(InteractionResponse),
    /// `edit_response`.
    EditResponse(MessagePayload),
    /// `create_message(channel, payload)`.
    Create(String, MessagePayload),
    /// `edit_message(channel, message, payload)`.
    Edit(String, String, MessagePayload),
    /// `create_command(guild, command)`.
    CreateCommand(Option<String>, CommandDefinition),
    /// `delete_command(guild, id)`.
    DeleteCommand(Option<String>, String),
}

/// Operations that can be made to fail.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    /// `create_response`.
    Response,
    /// `edit_response`.
    EditResponse,
    /// `create_message`.
    Create,
    /// `edit_message`.
    Edit,
}

/// The recording fake.
#[derive(Default)]
pub struct RecordingApi {
    calls: Mutex<VecDeque<Call>>,
    failures: Mutex<Vec<Op>>,
    edit_response_result: Mutex<Option<Result<SentMessage, DiscordError>>>,
    /// Commands `list_commands` returns.
    pub registered: Vec<RegisteredCommand>,
}

impl RecordingApi {
    /// A shared fake.
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// A fake whose `list_commands` returns `registered`.
    pub fn with_commands(registered: Vec<RegisteredCommand>) -> Self {
        Self {
            registered,
            ..Self::default()
        }
    }

    /// Makes each operation fail once with a network error (`DiscordNewGameAPI.fail/1`).
    pub fn fail(&self, operations: &[Op]) {
        self.failures.lock().unwrap().extend_from_slice(operations);
    }

    /// The result every `edit_response` returns from now on.
    pub fn set_edit_response_result(&self, result: Result<SentMessage, DiscordError>) {
        *self.edit_response_result.lock().unwrap() = Some(result);
    }

    /// Removes and returns every call so far.
    pub fn take(&self) -> Vec<Call> {
        self.calls.lock().unwrap().drain(..).collect()
    }

    /// Removes and returns the next call (`assert_receive`).
    #[track_caller]
    pub fn next(&self) -> Call {
        self.calls
            .lock()
            .unwrap()
            .pop_front()
            .expect("expected a Discord call")
    }

    /// Whether no call is waiting (`refute_receive`).
    pub fn is_idle(&self) -> bool {
        self.calls.lock().unwrap().is_empty()
    }

    /// Removes the calls and returns the interaction responses.
    pub fn responses(&self) -> Vec<InteractionResponse> {
        self.take()
            .into_iter()
            .filter_map(|call| match call {
                Call::Response(response) => Some(response),
                _ => None,
            })
            .collect()
    }

    fn record<T>(&self, op: Option<Op>, call: Call, success: T) -> Result<T, DiscordError> {
        self.calls.lock().unwrap().push_back(call);
        let mut failures = self.failures.lock().unwrap();
        match op.and_then(|op| failures.iter().position(|failing| *failing == op)) {
            Some(index) => {
                failures.remove(index);
                Err(DiscordError::Network)
            }
            None => Ok(success),
        }
    }
}

fn sent(id: &str) -> SentMessage {
    SentMessage { id: id.to_owned() }
}

impl DiscordApi for RecordingApi {
    fn create_response<'a>(
        &'a self,
        _interaction: &'a InteractionTarget,
        response: &'a InteractionResponse,
    ) -> ApiFuture<'a, ()> {
        let result = self.record(Some(Op::Response), Call::Response(response.clone()), ());
        Box::pin(async move { result })
    }

    fn edit_response<'a>(
        &'a self,
        _interaction: &'a InteractionTarget,
        message: &'a MessagePayload,
    ) -> ApiFuture<'a, SentMessage> {
        let configured = self.edit_response_result.lock().unwrap().clone();
        let result = self
            .record(
                Some(Op::EditResponse),
                Call::EditResponse(message.clone()),
                sent("555"),
            )
            .and_then(|default| configured.unwrap_or(Ok(default)));
        Box::pin(async move { result })
    }

    fn create_message<'a>(
        &'a self,
        channel_id: &'a str,
        message: &'a MessagePayload,
    ) -> ApiFuture<'a, SentMessage> {
        let result = self.record(
            Some(Op::Create),
            Call::Create(channel_id.to_owned(), message.clone()),
            sent("999"),
        );
        Box::pin(async move { result })
    }

    fn edit_message<'a>(
        &'a self,
        channel_id: &'a str,
        message_id: &'a str,
        message: &'a MessagePayload,
    ) -> ApiFuture<'a, SentMessage> {
        let result = self.record(
            Some(Op::Edit),
            Call::Edit(
                channel_id.to_owned(),
                message_id.to_owned(),
                message.clone(),
            ),
            sent(message_id),
        );
        Box::pin(async move { result })
    }

    fn create_command<'a>(
        &'a self,
        _application_id: &'a str,
        guild_id: Option<&'a str>,
        command: &'a CommandDefinition,
    ) -> ApiFuture<'a, ()> {
        let result = self.record(
            None,
            Call::CreateCommand(guild_id.map(str::to_owned), command.clone()),
            (),
        );
        Box::pin(async move { result })
    }

    fn list_commands<'a>(
        &'a self,
        _application_id: &'a str,
        _guild_id: Option<&'a str>,
    ) -> ApiFuture<'a, Vec<RegisteredCommand>> {
        let commands = self.registered.clone();
        Box::pin(async move { Ok(commands) })
    }

    fn delete_command<'a>(
        &'a self,
        _application_id: &'a str,
        guild_id: Option<&'a str>,
        command_id: &'a str,
    ) -> ApiFuture<'a, ()> {
        let result = self.record(
            None,
            Call::DeleteCommand(guild_id.map(str::to_owned), command_id.to_owned()),
            (),
        );
        Box::pin(async move { result })
    }
}

/// An interaction in guild 333 invoked by `user`.
pub fn interaction(channel: &str, user: &str, data: InteractionData) -> Interaction {
    Interaction {
        id: "777".into(),
        application_id: "888".into(),
        token: "test-only-token".into(),
        guild_id: Some("333".into()),
        channel_id: Some(channel.into()),
        user: Some(InteractionUser {
            id: user.into(),
            username: Some("Name".into()),
            global_name: None,
        }),
        member: Some(InteractionMember {
            nick: None,
            permissions: None,
        }),
        message_id: None,
        data,
    }
}

/// Slash command data.
pub fn command(name: &str, options: Vec<(&str, OptionValue)>) -> InteractionData {
    InteractionData::Command(CommandData {
        name: name.into(),
        options: options
            .into_iter()
            .map(|(name, value)| CommandOptionData {
                name: name.into(),
                value,
            })
            .collect(),
    })
}

/// A string option.
pub fn string(value: &str) -> OptionValue {
    OptionValue::String(value.into())
}

/// Component data with an optional selected value.
pub fn component(custom_id: &str, value: Option<&str>) -> InteractionData {
    InteractionData::Component(ComponentData {
        custom_id: custom_id.into(),
        values: value.map(str::to_owned).into_iter().collect(),
    })
}

/// Modal submission data.
pub fn modal(custom_id: &str, fields: &[(&str, &str)]) -> InteractionData {
    InteractionData::Modal(ModalData {
        custom_id: custom_id.into(),
        fields: fields
            .iter()
            .map(|(id, value)| ((*id).to_owned(), (*value).to_owned()))
            .collect(),
    })
}

/// A player.
pub fn player(id: &str, name: &str, commander: Option<&str>) -> ReportPlayer {
    ReportPlayer {
        discord_id: id.into(),
        display_name: name.into(),
        commander_name: commander.map(str::to_owned),
    }
}

/// A winnerless report of `spellbot:SB12345` in guild 333, channel 444.
pub fn report(played_at: UtcDateTime, players: Vec<ReportPlayer>) -> GameReport {
    GameReport {
        external_id: "spellbot:SB12345".into(),
        source: "discord".into(),
        played_at,
        guild_id: "333".into(),
        channel_id: "444".into(),
        players,
        winner_discord_ids: Vec::new(),
        raw: Map::new(),
        details: None,
    }
}

/// Captured log output (`ExUnit.CaptureLog`).
#[derive(Clone, Default)]
pub struct LogCapture(Arc<Mutex<Vec<u8>>>);

impl LogCapture {
    /// Captures `info` and above for the whole test binary. A thread-local subscriber
    /// would race with other tests over tracing's cached callsite interest, so test files
    /// that inspect logs keep them in one test.
    pub fn global() -> Self {
        static CAPTURE: std::sync::OnceLock<LogCapture> = std::sync::OnceLock::new();
        CAPTURE
            .get_or_init(|| {
                let capture = Self::default();
                let subscriber = tracing_subscriber::fmt()
                    .with_max_level(tracing::Level::INFO)
                    .with_ansi(false)
                    .with_writer(capture.clone())
                    .finish();
                tracing::subscriber::set_global_default(subscriber).expect("one global subscriber");
                capture
            })
            .clone()
    }

    /// Forgets what was logged so far.
    pub fn clear(&self) {
        self.0.lock().unwrap().clear();
    }

    /// Everything logged so far.
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().unwrap()).into_owned()
    }
}

impl Write for LogCapture {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for LogCapture {
    type Writer = LogCapture;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}
