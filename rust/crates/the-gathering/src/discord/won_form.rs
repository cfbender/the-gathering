//! The `/won` form's private messages and modals (`WonForm`).

use crate::games::WinCondition;
use crate::regex::compile;

use super::api::{
    AllowedMentions, Button, ButtonStyle, Component, EPHEMERAL, InteractionResponse,
    MessagePayload, Modal, ResponseKind, SelectMenu, SelectOption, TextInput, TextInputStyle,
    slice,
};
use super::card_choice::reportable_conditions;
use super::draft::{CardChoice, Role};
use super::report::ReportPlayer;
use super::won_report::{Loaded, kills_page};

/// Which modal to open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FormModal {
    /// Turns, duration, MVP, notes.
    Details,
    /// A page of kills.
    Kills(usize),
}

fn id(draft_id: &str, action: &str) -> String {
    format!("won:{draft_id}:{action}")
}

fn short(text: &str, limit: usize) -> String {
    compile(r"[`*_~]")
        .replace_all(&slice(text, limit), "")
        .into_owned()
}

fn display(value: Option<&str>) -> String {
    match value {
        None | Some("") => "—".into(),
        Some(value) => value.to_owned(),
    }
}

fn row(components: Vec<Component>) -> Component {
    Component::row(components)
}

fn input(
    name: &str,
    label: &str,
    style: TextInputStyle,
    max_length: u16,
    value: Option<&str>,
    placeholder: &str,
) -> Component {
    Component::TextInput(TextInput {
        custom_id: name.to_owned(),
        label: slice(label, 45),
        style,
        required: false,
        max_length,
        value: Some(value.unwrap_or_default().to_owned()),
        placeholder: placeholder.to_owned(),
    })
}

fn select(
    draft_id: &str,
    action: &str,
    placeholder: &str,
    options: Vec<(String, String)>,
    selected: Option<&str>,
) -> Component {
    row(vec![Component::SelectMenu(SelectMenu {
        custom_id: id(draft_id, action),
        placeholder: placeholder.to_owned(),
        min_values: 1,
        max_values: 1,
        options: options
            .into_iter()
            .map(|(value, label)| SelectOption {
                label: slice(&label, 100),
                default: selected == Some(value.as_str()),
                value,
            })
            .collect(),
    })])
}

fn button(draft_id: &str, action: &str, label: &str, style: ButtonStyle) -> Component {
    Component::Button(Button {
        custom_id: Some(id(draft_id, action)),
        label: label.to_owned(),
        style,
        url: None,
        disabled: None,
    })
}

fn modal_response(
    draft_id: &str,
    action: &str,
    title: &str,
    inputs: Vec<Component>,
) -> InteractionResponse {
    InteractionResponse::modal(Modal {
        custom_id: id(draft_id, action),
        title: title.to_owned(),
        components: inputs.into_iter().map(|input| row(vec![input])).collect(),
    })
}

fn response(
    content: String,
    components: Vec<Component>,
    kind: ResponseKind,
) -> InteractionResponse {
    InteractionResponse::message(
        kind,
        MessagePayload {
            content: Some(content),
            allowed_mentions: Some(AllowedMentions::none()),
            components: Some(components),
            // Updating an existing private message preserves its visibility.
            flags: (kind == ResponseKind::ChannelMessage).then_some(EPHEMERAL),
            ..MessagePayload::default()
        },
    )
}

/// A text-only message (private when new).
pub fn message(content: impl Into<String>, kind: ResponseKind) -> InteractionResponse {
    response(content.into(), Vec::new(), kind)
}

/// One of the draft's edit modals, prefilled from the draft.
pub fn modal(loaded: &Loaded, which: FormModal) -> InteractionResponse {
    let draft_id = &loaded.draft.id;
    let data = &loaded.data;
    match which {
        FormModal::Details => modal_response(
            draft_id,
            "details",
            "Game details",
            vec![
                input(
                    "turns",
                    "Turns (optional)",
                    TextInputStyle::Short,
                    10,
                    data.turns.as_deref(),
                    "Optional",
                ),
                input(
                    "duration",
                    "Duration in minutes (estimate; edit as needed)",
                    TextInputStyle::Short,
                    10,
                    data.duration.as_deref(),
                    "Optional",
                ),
                input(
                    "mvp",
                    "Winner's MVP card name (optional)",
                    TextInputStyle::Short,
                    150,
                    data.mvp.as_deref(),
                    "Optional",
                ),
                input(
                    "notes",
                    "Game notes (optional)",
                    TextInputStyle::Paragraph,
                    4000,
                    data.notes.as_deref(),
                    "Optional",
                ),
            ],
        ),
        FormModal::Kills(page) => {
            let inputs = kills_page(&loaded.players(), page)
                .iter()
                .map(|player| {
                    input(
                        &format!("kills_{}", player.discord_id),
                        &format!("{} — kills", player.display_name),
                        TextInputStyle::Short,
                        2,
                        data.kills.get(&player.discord_id).map(String::as_str),
                        "0–5; leave blank if unknown",
                    )
                })
                .collect();
            modal_response(
                draft_id,
                &format!("kills{page}"),
                &format!("Player kills · page {}", page + 1),
                inputs,
            )
        }
    }
}

/// The modal for a player's commander and partner, prefilled from the draft.
pub fn commander_modal(loaded: &Loaded, player: &ReportPlayer) -> InteractionResponse {
    let (commander, partner) = match loaded.data.commanders.get(&player.discord_id) {
        Some(choices) => (choices.commander.name.clone(), choices.partner.name.clone()),
        None => (
            player.commander_name.clone().unwrap_or_default(),
            String::new(),
        ),
    };
    modal_response(
        &loaded.draft.id,
        &format!("commander_{}", player.discord_id),
        &format!("{} · commander", short(&player.display_name, 30)),
        vec![
            input(
                "commander",
                "Commander (name or partial name)",
                TextInputStyle::Short,
                150,
                Some(&commander),
                "Optional",
            ),
            input(
                "partner",
                "Partner / Background (optional)",
                TextInputStyle::Short,
                150,
                Some(&partner),
                "Optional",
            ),
        ],
    )
}

fn choice_name(choice: &CardChoice) -> String {
    match choice.error {
        None => choice.name.clone(),
        Some(_) => format!("{} (unresolved)", choice.name),
    }
}

fn commander_line(loaded: &Loaded, player: &ReportPlayer) -> String {
    let names = match loaded.data.commanders.get(&player.discord_id) {
        Some(choices) => [Role::Commander, Role::Partner]
            .iter()
            .map(|role| choice_name(choices.get(*role)))
            .filter(|name| !name.is_empty())
            .collect::<Vec<_>>()
            .join(" + "),
        None => player.commander_name.clone().unwrap_or_default(),
    };
    let names = if names.is_empty() {
        "Not recorded".to_owned()
    } else {
        names
    };
    format!(
        "{}: {}",
        short(&player.display_name, 32),
        short(&names, 160)
    )
}

/// The commander panel, with candidate pickers for `player_id`.
pub fn commanders(
    loaded: &Loaded,
    kind: ResponseKind,
    player_id: Option<&str>,
) -> InteractionResponse {
    let draft_id = &loaded.draft.id;
    let players = loaded.players();
    let choices = player_id.and_then(|id| loaded.data.commanders.get(id));
    let mut lines = vec!["**Player commanders** — not saved yet".to_owned()];
    lines.extend(players.iter().map(|player| commander_line(loaded, player)));
    let roles = [Role::Commander, Role::Partner];
    if let Some(choices) = choices {
        lines.extend(
            roles
                .iter()
                .filter_map(|role| choices.get(*role).error.clone()),
        );
    }
    let mut components = vec![select(
        draft_id,
        "player",
        "Choose a player to enter or edit commanders",
        players
            .iter()
            .map(|player| (player.discord_id.clone(), player.display_name.clone()))
            .collect(),
        None,
    )];
    if let (Some(choices), Some(player_id)) = (choices, player_id) {
        for role in roles {
            let candidates = &choices.get(role).candidates;
            if !candidates.is_empty() {
                components.push(select(
                    draft_id,
                    &format!("{}_choice_{player_id}", role.as_str()),
                    &format!("Choose {}", role.as_str()),
                    candidates
                        .iter()
                        .map(|card| (card.id.clone(), card.name.clone()))
                        .collect(),
                    None,
                ));
            }
        }
    }
    components.push(row(vec![button(
        draft_id,
        "review",
        "Back to review",
        ButtonStyle::Secondary,
    )]));
    response(lines.join("\n"), components, kind)
}

/// The draft's review message and its controls, showing `error` when given.
pub fn review(loaded: &Loaded, kind: ResponseKind, error: Option<&str>) -> InteractionResponse {
    let draft_id = &loaded.draft.id;
    let data = &loaded.data;
    let players = loaded.players();
    let winner = players
        .iter()
        .find(|player| Some(&player.discord_id) == data.winner.as_ref());
    let kills = players
        .iter()
        .map(|player| {
            format!(
                "{}: {}",
                short(&player.display_name, 32),
                display(data.kills.get(&player.discord_id).map(String::as_str))
            )
        })
        .collect::<Vec<_>>()
        .join(" · ");
    let condition = data.win_condition.as_deref().and_then(WinCondition::parse);
    let external = loaded
        .pending
        .external_id
        .strip_prefix("spellbot:")
        .unwrap_or(&loaded.pending.external_id);
    let lines: Vec<String> = [
        Some(format!("**Review {external}** — not saved yet")),
        error.map(str::to_owned).or_else(|| data.mvp_error.clone()),
        Some(format!(
            "Winner: {} · {}",
            winner.map(|winner| short(&winner.display_name, 100)).unwrap_or_default(),
            WinCondition::label_of(condition)
        )),
        Some(format!(
            "Turns: {} · Minutes: {}",
            display(data.turns.as_deref()),
            display(data.duration.as_deref())
        )),
        Some(format!("MVP: {}", short(data.mvp.as_deref().unwrap_or_default(), 150))),
        Some(format!("Kills: {kills}")),
        Some(format!(
            "Commander entries: {} · use Commanders to review",
            data.commanders.len()
        )),
        Some(format!("Notes: {}", short(data.notes.as_deref().unwrap_or_default(), 500))),
        Some("Use the dropdowns and buttons to finish. Blank kills mean unknown, not zero. Draft expires after one hour.".to_owned()),
    ]
    .into_iter()
    .flatten()
    .collect();

    let winner_options = players
        .iter()
        .enumerate()
        .map(|(index, player)| {
            (
                player.discord_id.clone(),
                format!("{}. {}", index + 1, player.display_name),
            )
        })
        .collect();
    let conditions = reportable_conditions()
        .map(|condition| (condition.as_str().to_owned(), condition.label().to_owned()))
        .collect();
    let mut components = vec![
        select(
            draft_id,
            "winner",
            "Winner",
            winner_options,
            data.winner.as_deref(),
        ),
        select(
            draft_id,
            "condition",
            "Win condition",
            conditions,
            data.win_condition.as_deref(),
        ),
    ];
    if !data.mvp_candidates.is_empty() {
        components.push(select(
            draft_id,
            "mvp",
            "Choose the MVP card",
            data.mvp_candidates
                .iter()
                .map(|card| (card.id.clone(), card.name.clone()))
                .collect(),
            data.mvp_id.as_deref(),
        ));
    }
    let mut edit_buttons = vec![
        button(draft_id, "details", "Edit details", ButtonStyle::Secondary),
        button(draft_id, "kills0", "Player kills", ButtonStyle::Secondary),
        button(draft_id, "commanders", "Commanders", ButtonStyle::Secondary),
    ];
    if players.len() > 5 {
        edit_buttons.push(button(
            draft_id,
            "kills1",
            "More player kills",
            ButtonStyle::Secondary,
        ));
    }
    components.push(row(edit_buttons));
    components.push(row(vec![
        button(draft_id, "save", "Save game", ButtonStyle::Success),
        button(draft_id, "cancel", "Cancel", ButtonStyle::Danger),
    ]));
    response(lines.join("\n"), components, kind)
}
