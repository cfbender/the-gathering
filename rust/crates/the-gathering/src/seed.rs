//! Development demo data: five players, ten decks, and forty games (`the-gathering seed`).
//!
//! Re-running is harmless: players, decks, and games are found by name or by their
//! `demo-stats-N` external id before anything is created.

use std::collections::HashMap;

use anyhow::Context;
use time::macros::datetime;

use crate::db::UtcDateTime;
use crate::games::{Deck, DeckInput, GameInput, Games, PlayerInput, SeatInput};

const PLAYERS: [&str; 5] = ["Cody", "Mara", "Theo", "Jules", "Ren"];

const DECKS: [(&str, &str, &str, &str); 10] = [
    ("Cody", "Birds of a Feather", "Kangee, Sky Warden", "WU"),
    (
        "Cody",
        "Grave Intentions",
        "Muldrotha, the Gravetide",
        "UBG",
    ),
    ("Mara", "Goblin Mode", "Krenko, Mob Boss", "R"),
    ("Mara", "Court Intrigue", "Queen Marchesa", "WBR"),
    ("Theo", "Elf Service", "Lathril, Blade of the Elves", "BG"),
    ("Theo", "Deep Thoughts", "Aesi, Tyrant of Gyre Strait", "UG"),
    ("Jules", "Cat Pact", "Arahbo, Roar of the World", "WG"),
    ("Jules", "Artifact Hours", "Urza, Lord High Artificer", "U"),
    ("Ren", "Dragon Weather", "Miirym, Sentinel Wyrm", "URG"),
    (
        "Ren",
        "Everybody Hurts",
        "Kambal, Consul of Allocation",
        "WB",
    ),
];

const WINNERS: [&str; 8] = [
    "Cody", "Mara", "Cody", "Theo", "Jules", "Cody", "Ren", "Mara",
];

const MVPS: [&str; 4] = [
    "Sol Ring",
    "Rhystic Study",
    "Swords to Plowshares",
    "Heroic Intervention",
];

/// Number of demo games.
pub const GAMES: usize = 40;

/// Seeds the demo data and returns a one-line summary.
pub async fn run(games: &Games) -> anyhow::Result<String> {
    let mut player_ids = HashMap::new();
    for name in PLAYERS {
        let player = games
            .find_or_create_player_by_name(name, &PlayerInput::default())
            .await
            .map_err(|error| anyhow::anyhow!("player {name}: {error:?}"))?;
        player_ids.insert(name, player.id);
    }

    let mut decks_by_player: HashMap<&str, Vec<Deck>> = HashMap::new();
    for (owner, name, commander, colors) in DECKS {
        let owner_id = *player_ids.get(owner).context("deck owner")?;
        let deck = games
            .find_or_create_deck(
                owner_id,
                name,
                &DeckInput {
                    commander_name: Some(commander.to_owned()).into(),
                    color_identity: Some(colors.to_owned()).into(),
                    ..DeckInput::default()
                },
            )
            .await
            .map_err(|error| anyhow::anyhow!("deck {name}: {error:?}"))?;
        decks_by_player.entry(owner).or_default().push(deck);
    }
    for decks in decks_by_player.values_mut() {
        decks.sort_by(|a, b| a.name.cmp(&b.name));
    }

    let base = UtcDateTime::from_offset(datetime!(2025-11-01 19:00:00 UTC));
    for index in 0..GAMES {
        let input = game(index, base, &player_ids, &decks_by_player)?;
        let external_id = format!("demo-stats-{}", index + 1);
        games
            .find_or_create_game_by_external_id("csv", &external_id, &input)
            .await
            .map_err(|error| anyhow::anyhow!("game {external_id}: {error:?}"))?;
    }

    Ok(format!(
        "Seeded {} players, {} decks, and {GAMES} demo games.",
        PLAYERS.len(),
        DECKS.len()
    ))
}

/// Demo game `index`: four of the five players, rotating seats, two draws.
fn game(
    index: usize,
    base: UtcDateTime,
    player_ids: &HashMap<&str, i64>,
    decks_by_player: &HashMap<&str, Vec<Deck>>,
) -> anyhow::Result<GameInput> {
    let absent = PLAYERS
        .get(index % PLAYERS.len())
        .context("absent player")?;
    let table: Vec<&str> = PLAYERS.into_iter().filter(|name| name != absent).collect();
    let mut rotated = table.clone();
    rotated.rotate_left((index * 3) % 4);
    let draw = matches!(index, 11 | 29);
    let mut winner = *WINNERS.get(index % WINNERS.len()).context("winner")?;
    if !table.contains(&winner) {
        winner = table.get(index % 4).context("fallback winner")?;
    }

    let mut seats = Vec::new();
    for (seat, name) in (1_usize..).zip(&rotated) {
        let decks = decks_by_player.get(name).context("player decks")?;
        let deck = decks.get((index / 2 + seat) % 2).context("player deck")?;
        let won = *name == winner && !draw;
        let result = if draw {
            "draw"
        } else if won {
            "win"
        } else {
            "loss"
        };
        let mvp = if won {
            MVPS.get(index % MVPS.len()).copied()
        } else {
            None
        };
        seats.push(SeatInput {
            player_id: Some(*player_ids.get(name).context("player id")?).into(),
            deck_id: Some(deck.id).into(),
            seat: Some(i64::try_from(seat)?).into(),
            result: Some(result.to_owned()).into(),
            mvp_card_name: mvp.map(str::to_owned).into(),
            ..SeatInput::default()
        });
    }

    let days = i64::try_from(index * 7)?;
    Ok(GameInput {
        played_at: Some(base.plus(time::Duration::days(days))).into(),
        duration_minutes: Some(i64::try_from(52 + (index * 17) % 71)?).into(),
        turns: Some(i64::try_from(7 + (index * 5) % 9)?).into(),
        seats: Some(seats).into(),
        ..GameInput::default()
    })
}
