//! Deck chooser scripts through the API: saving, picking with a script, falling back, the
//! preview's script inputs, the shipped examples, and the field reference.

use std::collections::BTreeSet;
use std::path::PathBuf;

use serde_json::{Value, json};
use the_gathering::db::UtcDateTime;
use the_gathering::games::{Deck, Player};

use crate::support::TestApp;

struct Table {
    app: TestApp,
    krenko: Deck,
    atraxa: Deck,
}

/// A member whose player lost quickly with Krenko on a Saturday night in New York
/// (2026-09-20 01:30 UTC) and won a long game with Atraxa just now.
async fn table() -> Table {
    let app = TestApp::new().await;
    let user = app.unique_member().await;
    let player: Player = app
        .player_with(json!({"name": "Chooser"}), Some(user.id))
        .await;
    app.log_in(&user).await;
    let rival = app.player("Rival").await;
    let krenko = app
        .deck_with(json!({"player_id": player.id, "name": "Krenko", "commander_name": "Krenko, Mob Boss", "color_identity": "R"}))
        .await;
    let atraxa = app
        .deck_with(json!({"player_id": player.id, "name": "Atraxa", "commander_name": "Atraxa, Praetors' Voice", "color_identity": "GWUB"}))
        .await;
    let rival_deck = app
        .deck_with(json!({"player_id": rival.id, "name": "Yuriko", "commander_name": "Yuriko", "color_identity": "BU"}))
        .await;
    app.game(
        json!({"played_at": "2026-09-20T01:30:00Z", "duration_minutes": 25, "turns": 6, "seats": [
            {"player_id": player.id, "deck_id": krenko.id, "seat": 1, "result": "loss"},
            {"player_id": rival.id, "deck_id": rival_deck.id, "seat": 2, "result": "win"},
        ]}),
        None,
    )
    .await;
    app.game(
        json!({"played_at": UtcDateTime::now().to_string(), "duration_minutes": 120, "turns": 12, "seats": [
            {"player_id": player.id, "deck_id": atraxa.id, "seat": 1, "result": "win"},
            {"player_id": rival.id, "seat": 2, "result": "loss"},
        ]}),
        None,
    )
    .await;
    Table {
        app,
        krenko,
        atraxa,
    }
}

async fn preview(app: &TestApp, body: Value) -> Value {
    app.post("/api/deck-chooser/preview", body)
        .await
        .assert_json(200)["data"]
        .clone()
}

fn deck<'a>(preview: &'a Value, name: &str) -> &'a Value {
    preview["decks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|deck| deck["name"] == name)
        .unwrap_or_else(|| panic!("{name} missing from {preview}"))
}

#[tokio::test]
async fn preview_shows_the_tables_a_script_receives_in_the_browser_zone() {
    let Table { app, .. } = table().await;
    let data = preview(&app, json!({"tz": "America/New_York"})).await;
    assert_eq!(data["source"], "built_in");
    assert_eq!(data["error"], Value::Null);

    let ctx = &data["context"];
    assert_eq!(ctx["timezone"], "America/New_York");
    assert_eq!(ctx["player"]["name"], "Chooser");
    assert_eq!(ctx["games_today"], 1);
    assert_eq!(ctx["session_games"], 1);
    assert_eq!(ctx["last_game"]["deck_name"], "Atraxa");
    assert_eq!(
        ctx["stats"],
        json!({"games": 2, "wins": 1, "losses": 1, "draws": 0, "win_rate": 0.5, "decks": 2})
    );
    assert_eq!(
        ctx["recent_colors"],
        json!({"W": 1, "U": 1, "B": 1, "R": 1, "G": 1, "C": 0})
    );
    assert!(ctx.get("decks").is_none(), "decks are listed per row");

    let krenko = &deck(&data, "Krenko")["input"];
    assert_eq!(
        (&krenko["plays"], &krenko["wins"], &krenko["losses"]),
        (&json!(1), &json!(0), &json!(1))
    );
    assert_eq!(krenko["win_rate"], 0.0);
    assert_eq!(krenko["streak"], json!({"result": "loss", "length": 1}));
    assert_eq!(krenko["plays_today"], 0);
    assert_eq!(krenko["color_set"], json!({"R": true}));
    let last = &krenko["last_game"];
    // 01:30 UTC on the 20th is 21:30 on Saturday the 19th in New York.
    assert_eq!(
        last["local"],
        json!({"unix": 1_789_867_800, "date": "2026-09-19", "year": 2026, "month": 9, "day": 19,
               "hour": 21, "minute": 30, "weekday": 7, "weekday_name": "saturday", "weekend": true})
    );
    assert_eq!(
        (&last["duration_minutes"], &last["turns"], &last["lost"]),
        (&json!(25), &json!(6), &json!(true))
    );
    assert_eq!(last["winners"], json!(["Rival"]));
    let opponent = &last["opponents"][0];
    assert_eq!(
        (&opponent["name"], &opponent["won"], &opponent["color_name"]),
        (&json!("Rival"), &json!(true), &json!("Dimir"))
    );
    assert_eq!(last["in_session"], false);

    let atraxa = &deck(&data, "Atraxa")["input"];
    assert_eq!(atraxa["color_identity"], "WUBG");
    assert_eq!(atraxa["colors"], json!(["W", "U", "B", "G"]));
    assert_eq!(atraxa["color_name"], "Witch");
    assert_eq!(
        (&atraxa["plays_today"], &atraxa["plays_this_session"]),
        (&json!(1), &json!(1))
    );
    assert_eq!(atraxa["avg_duration_minutes"], 120.0);

    let total: f64 = data["decks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|deck| deck["chance"].as_f64().unwrap())
        .sum();
    assert!((total - 1.0).abs() < 1e-9);
    for row in data["decks"].as_array().unwrap() {
        assert_eq!(row["weight"], row["base_weight"]);
    }

    let utc = preview(&app, json!({})).await;
    assert_eq!(utc["context"]["timezone"], "Etc/UTC");
    assert_eq!(
        deck(&utc, "Krenko")["input"]["last_game"]["local"]["date"],
        "2026-09-20"
    );
}

#[tokio::test]
async fn preview_runs_a_draft_with_notes_logs_and_errors() {
    let Table { app, krenko, .. } = table().await;
    let script = r#"
        function weight(deck, ctx)
          print(deck.name, deck.plays)
          if deck.last_game and deck.last_game.lost then
            return deck.base_weight * 3, "rematch"
          end
          return 1
        end
    "#;
    let data = preview(&app, json!({"script": script})).await;
    assert_eq!(data["source"], "script");
    assert_eq!(data["logs"], json!(["Atraxa\t1", "Krenko\t1"]));
    assert_eq!(deck(&data, "Krenko")["note"], "rematch");
    assert_eq!(deck(&data, "Atraxa")["weight"], 1.0);
    assert_eq!(
        deck(&data, "Krenko")["weight"].as_f64().unwrap(),
        deck(&data, "Krenko")["base_weight"].as_f64().unwrap() * 3.0
    );

    let excluded = preview(&app, json!({"script": script, "exclude_id": krenko.id})).await;
    assert_eq!(excluded["context"]["excluded_deck_id"], krenko.id);
    assert_eq!(excluded["decks"].as_array().unwrap().len(), 1);

    let broken = preview(
        &app,
        json!({"script": "function weight(deck)\n  return deck.nope.nope\nend"}),
    )
    .await;
    assert_eq!(broken["source"], "built_in");
    assert_eq!(broken["error"]["line"], 2);
    for row in broken["decks"].as_array().unwrap() {
        assert_eq!(row["weight"], row["base_weight"]);
    }
}

#[tokio::test]
async fn saves_validates_and_clears_the_members_script() {
    let Table { app, .. } = table().await;
    assert_eq!(
        app.get("/api/deck-chooser/script").await.assert_json(200),
        json!({"data": {"script": null}})
    );
    let invalid = app
        .put(
            "/api/deck-chooser/script",
            json!({"script": "function weight(deck)\n  return (\nend"}),
        )
        .await
        .assert_json(422);
    let message = invalid["errors"]["script"][0].as_str().unwrap();
    assert!(message.starts_with("line 3: "), "{message}");
    assert_eq!(
        app.put("/api/deck-chooser/script", json!({"script": "x = 1"}))
            .await
            .assert_json(422),
        json!({"errors": {"script": ["the script must define a function named weight(deck, ctx)"]}})
    );

    let script = "function weight(deck) return 1 end";
    app.put("/api/deck-chooser/script", json!({"script": script}))
        .await
        .assert_json(200);
    assert_eq!(
        app.get("/api/deck-chooser/script").await.assert_json(200)["data"]["script"],
        script
    );
    assert_eq!(
        app.put("/api/deck-chooser/script", json!({"script": "  \n"}))
            .await
            .assert_json(200),
        json!({"data": {"script": null}})
    );
    assert_eq!(
        app.get("/api/deck-chooser/script").await.assert_json(200)["data"]["script"],
        Value::Null
    );
}

#[tokio::test]
async fn picks_with_the_saved_script_and_falls_back_when_it_fails() {
    let Table {
        app,
        krenko,
        atraxa,
    } = table().await;
    let script = r#"
        function weight(deck)
          if deck.name == "Atraxa" then return 0 end
          return 5, "only " .. deck.name
        end
    "#;
    app.put("/api/deck-chooser/script", json!({"script": script}))
        .await
        .assert_json(200);
    for _ in 0..8 {
        let pick = app.get("/api/deck-chooser").await.assert_json(200)["data"].clone();
        assert_eq!(pick["deck"]["id"], krenko.id);
        assert_eq!(pick["chance"], 1.0);
        assert_eq!(pick["note"], "only Krenko");
        assert_eq!(pick["script_error"], Value::Null);
    }

    // Skipping Krenko leaves only Atraxa, which the script weighs 0: the pick falls back.
    let pick = app
        .get(&format!("/api/deck-chooser?exclude_id={}", krenko.id))
        .await
        .assert_json(200)["data"]
        .clone();
    assert_eq!(pick["deck"]["id"], atraxa.id);
    assert_eq!(
        pick["script_error"],
        json!({"message": "weight() returned 0 for every deck", "line": null})
    );
    assert_eq!(pick["note"], Value::Null);

    app.put(
        "/api/deck-chooser/script",
        json!({"script": "function weight(deck)\n  return deck.missing + 1\nend"}),
    )
    .await
    .assert_json(200);
    let pick = app.get("/api/deck-chooser").await.assert_json(200)["data"].clone();
    assert!(pick["deck"]["id"].is_i64());
    assert_eq!(pick["script_error"]["line"], 2);
}

fn examples_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../assets/react/src/features/decks/deck-scripts")
}

#[tokio::test]
async fn every_shipped_example_runs_without_errors() {
    let Table { app, .. } = table().await;
    let mut ran = 0;
    for entry in std::fs::read_dir(examples_dir()).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|extension| extension != "lua") {
            continue;
        }
        let script = std::fs::read_to_string(&path).unwrap();
        let data = preview(&app, json!({"script": script, "tz": "America/New_York"})).await;
        assert_eq!(
            (&data["source"], &data["error"]),
            (&json!("script"), &Value::Null),
            "{}",
            path.display()
        );
        app.put("/api/deck-chooser/script", json!({"script": script}))
            .await
            .assert_json(200);
        ran += 1;
    }
    assert!(ran >= 10, "found {ran} examples");
}

/// Every field path the script sees, as `docs/deck-chooser-scripts.md` names it.
fn field_paths(value: &Value, prefix: &str, paths: &mut BTreeSet<String>) {
    let Value::Object(map) = value else { return };
    for (key, child) in map {
        let path = format!("{prefix}.{key}");
        paths.insert(path.clone());
        match (key.as_str(), child) {
            ("games" | "today" | "session" | "recent_games", Value::Array(games)) => {
                for game in games {
                    field_paths(game, "game", paths);
                }
            }
            ("opponents", Value::Array(opponents)) => {
                for opponent in opponents {
                    field_paths(opponent, "opponent", paths);
                }
            }
            ("last_game", _) => field_paths(child, "game", paths),
            ("local" | "now", _) => field_paths(child, "time", paths),
            ("streak" | "player" | "stats", _) => field_paths(child, &path, paths),
            _ => {}
        }
    }
}

#[tokio::test]
async fn the_field_reference_documents_every_field() {
    let Table { app, krenko, .. } = table().await;
    let data = preview(&app, json!({"exclude_id": krenko.id})).await;
    let mut paths = BTreeSet::from(["ctx.decks".to_owned()]);
    field_paths(&data["context"], "ctx", &mut paths);
    let all = preview(&app, json!({})).await;
    for row in all["decks"].as_array().unwrap() {
        field_paths(&row["input"], "deck", &mut paths);
    }
    let docs = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../docs/deck-chooser-scripts.md"),
    )
    .unwrap();
    let missing: Vec<&String> = paths
        .iter()
        .filter(|path| !docs.contains(&format!("| `{path}` |")))
        .collect();
    assert!(missing.is_empty(), "undocumented fields: {missing:?}");
    assert!(paths.len() > 100, "walked {} fields", paths.len());
}
