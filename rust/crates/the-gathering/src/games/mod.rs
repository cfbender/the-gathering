//! Players, decks, and games (`TheGathering.Games`).

pub mod color_identity;
pub mod resolve_player;

/// Case-folds a player or deck name the way SQLite compares them (`Games.fold_name/1`).
///
/// The `players_name_nocase_index` and `decks_player_name_nocase_index` unique indexes use
/// `COLLATE NOCASE`, and `lower()` in queries is ASCII-only, so folding must be ASCII-only too:
/// `Éowyn` stays `Éowyn`.
pub fn fold_name(name: &str) -> String {
    name.trim().to_ascii_lowercase()
}
