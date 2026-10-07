//! The development seed.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

use crate::support;

use support::TestApp;

#[tokio::test]
async fn seeds_demo_data_once() {
    let app = TestApp::new().await;
    let summary = the_gathering::seed::run(&app.state.games).await.unwrap();
    assert_eq!(summary, "Seeded 5 players, 10 decks, and 40 demo games.");
    the_gathering::seed::run(&app.state.games).await.unwrap();

    let counts = sqlx::query_as::<_, (i64, i64, i64, i64, i64)>(
        "SELECT (SELECT count(*) FROM players), (SELECT count(*) FROM decks),
                (SELECT count(*) FROM games), (SELECT count(*) FROM game_players),
                (SELECT count(*) FROM game_players WHERE result = 'draw')",
    )
    .fetch_one(app.pool())
    .await
    .unwrap();
    assert_eq!(counts, (5, 10, 40, 160, 8));
}
