//! AI rulings: the Comprehensive Rules updater, the OpenRouter settings, the tool-calling
//! answer flow, and the monthly spend limit.

use serde_json::{Value, json};
use the_gathering::patch::Patch;
use the_gathering::rules;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::support::TestApp;

const RULES: &str = include_str!("../fixtures/comprehensive_rules_excerpt.txt");
const RULES_PAGE: &str = r#"<p><a class="cta" href="https://media.wizards.com/2026/downloads/MagicCompRules 20260925.pdf">PDF</a></p>
<p><a class="cta" href="/downloads/MagicCompRules 20260925.txt" target="_blank">TXT</a></p>"#;

async fn app(server: &MockServer) -> TestApp {
    let base = server.uri();
    TestApp::with_config(move |config| {
        config.rules_page_url = format!("{base}/en/rules");
        config.openrouter_api_base = format!("{base}/api/v1");
    })
    .await
}

async fn install_rules(app: &TestApp) {
    let document = rules::parse(RULES).unwrap();
    rules::install(app.pool(), "https://example.com/rules.txt", &document)
        .await
        .unwrap();
}

async fn configure(app: &TestApp, limit: Option<f64>) {
    the_gathering::ai::save_settings(
        app.pool(),
        &app.state.config.secret_key,
        &Patch::Set(Some("sk-or-test".to_owned())),
        Some("test/model"),
        limit,
    )
    .await
    .unwrap();
}

#[test]
fn parses_the_text_release() {
    let document = rules::parse(RULES).unwrap();
    assert_eq!(document.effective_on.unwrap().to_string(), "2026-09-25");
    let numbers: Vec<&str> = document
        .rules
        .iter()
        .map(|rule| rule.number.as_str())
        .collect();
    // The contents list is not mistaken for rules: each heading appears once.
    assert_eq!(numbers.iter().filter(|number| **number == "702").count(), 1);
    assert_eq!(&numbers[..5], ["1", "100", "100.1", "100.1a", "100.1b"]);
    let trample = document
        .rules
        .iter()
        .find(|rule| rule.number == "702.19")
        .unwrap();
    assert_eq!(trample.text, "Trample");
    // Examples stay with their subrule.
    let assignment = document
        .rules
        .iter()
        .find(|rule| rule.number == "702.19b")
        .unwrap();
    assert!(
        assignment
            .text
            .starts_with("The controller of an attacking creature with trample")
    );
    assert_eq!(assignment.text.matches("\nExample: ").count(), 2);
    assert_eq!(
        document
            .glossary
            .iter()
            .map(|entry| entry.term.as_str())
            .collect::<Vec<_>>(),
        ["Ability", "Commander Tax", "Deathtouch", "Trample"]
    );
    // Numbered meanings, separated by U+2028 in the release, become lines.
    assert!(
        document.glossary[0]
            .definition
            .contains("can do.\n2. An activated")
    );
    // Windows line endings and a byte-order mark parse the same.
    let windows = format!("\u{feff}{}", RULES.replace('\n', "\r\n"));
    assert_eq!(rules::parse(&windows), Some(document));
}

#[tokio::test]
async fn looks_up_and_searches_the_installed_rules() {
    let server = MockServer::start().await;
    let app = app(&server).await;
    install_rules(&app).await;
    let pool = app.pool();
    let numbers = |rules: Vec<rules::Rule>| {
        rules
            .into_iter()
            .map(|rule| rule.number)
            .collect::<Vec<_>>()
    };

    // A rule brings its subrules, not the rules sharing its prefix (702.1 vs 702.19).
    let rule = rules::lookup(pool, "CR 702.1.").await.unwrap().unwrap();
    assert_eq!(numbers(rule), ["702.1"]);
    let trample = rules::lookup(pool, "702.19").await.unwrap().unwrap();
    assert_eq!(numbers(trample).len(), 8);
    let subrule = rules::lookup(pool, "702.19c").await.unwrap().unwrap();
    assert_eq!(numbers(subrule), ["702.19c"]);
    // A chapter lists its rules without subrules; a section lists its chapters.
    let chapter = rules::lookup(pool, "702").await.unwrap().unwrap();
    assert_eq!(numbers(chapter), ["702", "702.1", "702.2", "702.19"]);
    let section = rules::lookup(pool, "9").await.unwrap().unwrap();
    assert_eq!(numbers(section), ["9", "903"]);
    assert_eq!(rules::lookup(pool, "trample").await.unwrap(), None);

    let hits = rules::search(pool, "creature tramples over a blocker", 3)
        .await
        .unwrap();
    // The glossary entry named by the query comes first, then the trample rules.
    assert_eq!(
        hits[0],
        rules::Hit::Glossary {
            term: "Trample".into(),
            definition: "A keyword ability that modifies how a creature assigns combat damage. See rule 702.19, “Trample.”".into(),
        }
    );
    assert!(
        hits[1..].iter().all(
            |hit| matches!(hit, rules::Hit::Rule { number, .. } if number.starts_with("702.19"))
        )
    );
    let tax = rules::search(pool, "commander tax", 1).await.unwrap();
    assert!(matches!(&tax[0], rules::Hit::Glossary { term, .. } if term == "Commander Tax"));
}

#[tokio::test]
async fn administrators_update_the_rules_from_the_wizards_page() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/en/rules"))
        .respond_with(ResponseTemplate::new(200).set_body_string(RULES_PAGE))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/downloads/MagicCompRules%2020260925.txt"))
        .respond_with(ResponseTemplate::new(200).set_body_string(RULES))
        .expect(1)
        .mount(&server)
        .await;
    let app = app(&server).await;
    let member = app.unique_member().await;
    app.log_in(&member).await;
    app.post("/api/admin/rules/sync", json!({}))
        .await
        .assert_json(403);

    let admin = app.unique_admin().await;
    app.log_in_sudo(&admin).await;
    let body = app
        .post("/api/admin/rules/sync", json!({}))
        .await
        .assert_json(200);
    assert_eq!(body["data"]["outcome"], "installed");
    let status = &body["data"]["status"];
    assert_eq!(status["effective_on"], "2026-09-25");
    assert_eq!(
        status["source_url"],
        format!("{}/downloads/MagicCompRules%2020260925.txt", server.uri())
    );
    assert_eq!(status["glossary_count"], 4);
    assert!(status["rule_count"].as_i64().unwrap() > 20);

    // The same release is not downloaded again.
    let body = app
        .post("/api/admin/rules/sync", json!({}))
        .await
        .assert_json(200);
    assert_eq!(body["data"]["outcome"], "current");
    let body = app.get("/api/admin/rules").await.assert_json(200);
    assert_eq!(body["data"]["last_error"], Value::Null);
    assert!(body["data"]["checked_at"].is_string());
}

#[tokio::test]
async fn a_failed_rules_check_keeps_the_installed_rules() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/en/rules"))
        .respond_with(ResponseTemplate::new(200).set_body_string("<p>Maintenance</p>"))
        .mount(&server)
        .await;
    let app = app(&server).await;
    install_rules(&app).await;
    let admin = app.unique_admin().await;
    app.log_in_sudo(&admin).await;
    let body = app
        .post("/api/admin/rules/sync", json!({}))
        .await
        .assert_json(502);
    assert!(
        body["errors"]["detail"]
            .as_str()
            .unwrap()
            .contains("links no Comprehensive Rules text file")
    );
    let status = rules::status(app.pool()).await.unwrap();
    assert!(status.rule_count > 20);
    assert!(
        status
            .last_error
            .unwrap()
            .contains("links no Comprehensive Rules")
    );
}

async fn openrouter_account(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/api/v1/key"))
        .and(header("authorization", "Bearer sk-or-good"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({ "data": { "label": "test" } })),
        )
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/key"))
        .respond_with(ResponseTemplate::new(401).set_body_json(
            json!({ "error": { "message": "No auth credentials found", "code": 401 } }),
        ))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/models"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "data": [
            { "id": "anthropic/claude-sonnet-5.5", "supported_parameters": ["tools", "max_tokens"] },
            { "id": "some/no-tools", "supported_parameters": ["max_tokens"] },
        ] })))
        .mount(server)
        .await;
}

#[tokio::test]
async fn administrators_configure_openrouter_after_it_confirms_the_key_and_model() {
    let server = MockServer::start().await;
    openrouter_account(&server).await;
    let app = app(&server).await;
    let admin = app.unique_admin().await;
    app.log_in_sudo(&admin).await;

    let body = app
        .patch(
            "/api/admin/ai-settings",
            json!({ "api_key": "sk-or-bad", "model": "anthropic/claude-sonnet-5.5" }),
        )
        .await
        .assert_json(422);
    assert_eq!(
        body["errors"]["api_key"],
        json!(["was rejected by OpenRouter"])
    );
    let body = app
        .patch(
            "/api/admin/ai-settings",
            json!({ "api_key": "sk-or-good", "model": "nobody/nothing" }),
        )
        .await
        .assert_json(422);
    assert_eq!(
        body["errors"]["model"],
        json!(["is not a model OpenRouter offers"])
    );
    let body = app
        .patch(
            "/api/admin/ai-settings",
            json!({ "api_key": "sk-or-good", "model": "some/no-tools" }),
        )
        .await
        .assert_json(422);
    assert!(
        body["errors"]["model"][0]
            .as_str()
            .unwrap()
            .contains("tool calling")
    );
    let body = app
        .patch("/api/admin/ai-settings", json!({ "monthly_limit_usd": -1 }))
        .await
        .assert_json(422);
    assert!(body["errors"]["monthly_limit_usd"].is_array());

    let body = app
        .patch(
            "/api/admin/ai-settings",
            json!({ "api_key": " sk-or-good ", "model": "anthropic/claude-sonnet-5.5", "monthly_limit_usd": 2.5 }),
        )
        .await
        .assert_json(200);
    assert_eq!(body["data"]["has_api_key"], true);
    assert_eq!(body["data"]["model"], "anthropic/claude-sonnet-5.5");
    assert_eq!(body["data"]["monthly_limit_usd"], 2.5);
    let shown = app.get("/api/admin/ai-settings").await;
    assert!(!shown.text().contains("sk-or-good"));
    let stored = sqlx::query_scalar::<_, String>("SELECT api_key FROM ai_settings")
        .fetch_one(app.pool())
        .await
        .unwrap();
    assert!(!stored.contains("sk-or-good"));
    let settings = the_gathering::ai::settings(app.pool(), &app.state.config.secret_key)
        .await
        .unwrap();
    assert_eq!(settings.api_key.as_deref(), Some("sk-or-good"));

    // Changing only the limit leaves the key alone and asks OpenRouter nothing.
    let checks = server.received_requests().await.unwrap().len();
    let body = app
        .patch(
            "/api/admin/ai-settings",
            json!({ "monthly_limit_usd": null }),
        )
        .await
        .assert_json(200);
    assert_eq!(body["data"]["has_api_key"], true);
    assert_eq!(body["data"]["monthly_limit_usd"], Value::Null);
    assert_eq!(server.received_requests().await.unwrap().len(), checks);
    // The audit log records that a key is set, never the key.
    let audit = sqlx::query_scalar::<_, String>(
        "SELECT group_concat(coalesce(before_json, '') || coalesce(after_json, ''), '') FROM audit_changes WHERE entity = 'ai_settings'",
    )
    .fetch_one(app.pool())
    .await
    .unwrap();
    assert!(audit.contains("\"api_key_set\":true"), "{audit}");
    assert!(!audit.contains("sk-or"), "{audit}");
}

fn completion(message: &Value, cost: f64) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(json!({
        "id": "gen-1",
        "model": "test/model-2026",
        "choices": [{ "message": message, "finish_reason": "stop" }],
        "usage": {
            "prompt_tokens": 1000,
            "completion_tokens": 100,
            "total_tokens": 1100,
            "cost": cost,
            "prompt_tokens_details": { "cached_tokens": 600, "cache_write_tokens": 300 },
        },
    }))
}

const ANSWER: &str = "**Yes: assign lethal damage to the blocker and the rest to the player.** With deathtouch, 1 damage is lethal (CR 702.19c, 702.2c), so [[Questing Beast]] assigns the rest to you. [[Not A Card]] has no say, and neither does Sol Ring or Llanowar Elves.";

async fn openrouter_answers(server: &MockServer) {
    Mock::given(method("POST"))
        .and(path("/api/v1/chat/completions"))
        .respond_with(completion(
            &json!({
                "role": "assistant",
                "content": null,
                "tool_calls": [
                    { "id": "call-1", "type": "function", "function": { "name": "get_rules", "arguments": "{\"numbers\":[\"702.19c\", \"999.9\"]}" } },
                    { "id": "call-2", "type": "function", "function": { "name": "lookup_cards", "arguments": "{\"names\":[\"questing beast\", \"Questing\"]}" } },
                ],
            }),
            0.0125,
        ))
        .up_to_n_times(1)
        .mount(server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v1/chat/completions"))
        .respond_with(completion(
            &json!({ "role": "assistant", "content": ANSWER }),
            0.0075,
        ))
        .mount(server)
        .await;
}

async fn questing_beast(app: &TestApp) {
    for (id, oracle_id, name) in [
        (
            "a5a4f3b7-1c7f-4a6e-9f0b-0b2f0f8e1c11",
            "6ad8011d-3471-4369-9d68-b264cc027487",
            "Sol Ring",
        ),
        (
            "0b1c2d3e-4f50-4617-8293-a4b5c6d7e8f9",
            "68954295-54e3-4303-a6bc-fc4547a4e3a3",
            "Lightning Bolt",
        ),
    ] {
        app.catalog_card(
            json!({ "id": id, "oracle_id": oracle_id, "name": name, "type_line": "Artifact" }),
        )
        .await;
    }
    app.catalog_card(json!({
        "id": "e41cf82d-3213-47ce-a015-6e51a8b07e4f",
        "oracle_id": "cb4ab6ae-3b2e-4b5e-9d67-2d5c8d2e0d8a",
        "name": "Questing Beast",
        "mana_cost": "{2}{G}{G}",
        "type_line": "Legendary Creature — Beast",
        "oracle_text": "Vigilance, deathtouch, haste\nCombat damage that would be dealt by creatures you control can't be prevented.",
        "color_identity": ["G"],
    }))
    .await;
    sqlx::query(
        "INSERT INTO card_rulings (oracle_id, source, published_at, comment) VALUES ('cb4ab6ae-3b2e-4b5e-9d67-2d5c8d2e0d8a', 'wotc', '2019-10-04', 'Questing Beast can attack the turn it comes under your control.')",
    )
    .execute(app.pool())
    .await
    .unwrap();
}

#[tokio::test]
async fn answers_with_rules_and_cards_the_model_looked_up() {
    let server = MockServer::start().await;
    openrouter_answers(&server).await;
    let app = app(&server).await;
    install_rules(&app).await;
    questing_beast(&app).await;
    configure(&app, None).await;
    let member = app.unique_member().await;
    app.log_in(&member).await;

    let body = app
        .post(
            "/api/ai-rulings",
            json!({
                "question": "My deathtouch trampler is blocked by a 5/5. How much gets through?",
                "history": [{ "question": "Earlier?", "answer": "Earlier answer." }],
                "cards": ["Questing Beast", "Sol Ring", "Lightning Bolt"],
            }),
        )
        .await
        .assert_json(200);
    let data = &body["data"];
    assert_eq!(data["answer"], ANSWER);
    assert_eq!(data["model"], "test/model-2026");
    // Only installed rules come back as citations, in citation order.
    let cited: Vec<&str> = data["rules"]
        .as_array()
        .unwrap()
        .iter()
        .map(|rule| rule["number"].as_str().unwrap())
        .collect();
    assert_eq!(cited, ["702.19c", "702.2c"]);
    assert!(
        data["rules"][0]["text"]
            .as_str()
            .unwrap()
            .starts_with("Trample over planeswalkers")
    );
    // Bracketed names and named cards in play resolve; unknown and unmentioned ones do not.
    assert_eq!(
        data["cards"],
        json!([
            { "name": "Questing Beast", "id": "e41cf82d-3213-47ce-a015-6e51a8b07e4f" },
            { "name": "Sol Ring", "id": "a5a4f3b7-1c7f-4a6e-9f0b-0b2f0f8e1c11" },
        ])
    );

    let requests = server.received_requests().await.unwrap();
    let first: Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(first["model"], "test/model");
    // The first round must look something up; later rounds may answer.
    assert_eq!(first["tool_choice"], "required");
    assert_eq!(requests[0].headers["authorization"], "Bearer sk-or-test");
    let messages = first["messages"].as_array().unwrap();
    // Cache breakpoints follow the system prompt and the newest message only.
    let ephemeral = json!({ "type": "ephemeral" });
    assert_eq!(messages[0]["content"][0]["cache_control"], ephemeral);
    let system = messages[0]["content"][0]["text"].as_str().unwrap();
    assert!(system.contains("effective as of 2026-09-25"));
    assert!(system.contains("\n702. Keyword Abilities\n"));
    assert_eq!(
        messages[1],
        json!({ "role": "user", "content": "Earlier?" })
    );
    assert_eq!(
        messages[2],
        json!({ "role": "assistant", "content": "Earlier answer." })
    );
    assert_eq!(messages[3]["content"][0]["cache_control"], ephemeral);
    let question = messages[3]["content"][0]["text"].as_str().unwrap();
    assert!(
        question
            .starts_with("Cards in play at the table: Questing Beast; Sol Ring; Lightning Bolt")
    );
    let tools: Vec<&str> = first["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["function"]["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        tools,
        ["search_rules", "get_rules", "lookup_cards", "search_cards"]
    );

    // The second request carries the tool results, in the same session, with the breakpoint
    // moved to the newest result.
    let second: Value = serde_json::from_slice(&requests[1].body).unwrap();
    let session = first["session_id"].as_str().unwrap();
    assert!(session.starts_with("the-gathering-ruling-"));
    assert_eq!(second["session_id"], session);
    assert!(second.get("tool_choice").is_none());
    let messages = second["messages"].as_array().unwrap();
    assert!(messages[3]["content"].is_string());
    assert!(messages[5]["content"].is_string());
    assert_eq!(messages[6]["content"][0]["cache_control"], ephemeral);
    assert_eq!(messages[4]["tool_calls"][0]["id"], "call-1");
    assert_eq!(messages[5]["tool_call_id"], "call-1");
    let rules_result: Value =
        serde_json::from_str(messages[5]["content"].as_str().unwrap()).unwrap();
    assert_eq!(rules_result["rules"][0]["number"], "702.19c");
    assert_eq!(rules_result["not_found"], json!(["999.9"]));
    let cards_result: Value =
        serde_json::from_str(messages[6]["content"][0]["text"].as_str().unwrap()).unwrap();
    let card = &cards_result["cards"][0];
    assert_eq!(card["name"], "Questing Beast");
    assert!(card["oracle_text"].as_str().unwrap().contains("deathtouch"));
    assert_eq!(
        card["rulings"][0]["text"],
        "Questing Beast can attack the turn it comes under your control."
    );
    assert_eq!(cards_result["not_found"][0]["name"], "Questing");
    assert_eq!(
        cards_result["not_found"][0]["similar"],
        json!(["Questing Beast"])
    );

    // Both requests' costs and cache use are recorded against the member.
    let cache = sqlx::query_as::<_, (i64, i64)>(
        "SELECT cached_tokens, cache_write_tokens FROM ai_ruling_usage",
    )
    .fetch_one(app.pool())
    .await
    .unwrap();
    assert_eq!(cache, (1200, 600));
    let status = app.get("/api/ai-rulings").await.assert_json(200);
    assert_eq!(status["data"]["enabled"], true);
    assert!((status["data"]["usage"]["spent_usd"].as_f64().unwrap() - 0.02).abs() < 1e-9);
    assert_eq!(status["data"]["usage"]["limit_usd"], Value::Null);

    // A follow-up stays in the conversation's session; a new conversation gets its own.
    let session_of_last = || async {
        let requests = server.received_requests().await.unwrap();
        let body: Value = serde_json::from_slice(&requests.last().unwrap().body).unwrap();
        body["session_id"].as_str().unwrap().to_owned()
    };
    let follow_up = json!({
        "question": "And with first strike?",
        "history": [
            { "question": "Earlier?", "answer": "Earlier answer." },
            { "question": "My deathtouch trampler is blocked by a 5/5. How much gets through?", "answer": ANSWER },
        ],
    });
    app.post("/api/ai-rulings", follow_up)
        .await
        .assert_json(200);
    assert_eq!(session_of_last().await, session);
    let fresh = json!({ "question": "Earlier?" });
    app.post("/api/ai-rulings", fresh).await.assert_json(200);
    assert_eq!(session_of_last().await, session, "same first question");
    app.post("/api/ai-rulings", json!({ "question": "Something new?" }))
        .await
        .assert_json(200);
    assert_ne!(session_of_last().await, session);
}

#[tokio::test]
async fn members_over_the_monthly_limit_are_refused() {
    let server = MockServer::start().await;
    openrouter_answers(&server).await;
    let app = app(&server).await;
    install_rules(&app).await;
    configure(&app, Some(0.02)).await;
    let member = app.unique_member().await;
    let other = app.unique_member().await;
    app.log_in(&member).await;
    let ask = json!({ "question": "Does trample work with deathtouch?" });
    app.post("/api/ai-rulings", ask.clone())
        .await
        .assert_json(200);
    let asked = server.received_requests().await.unwrap().len();

    let body = app
        .post("/api/ai-rulings", ask.clone())
        .await
        .assert_json(429);
    assert_eq!(body["errors"]["code"], "ai_budget_exhausted");
    assert!(body["errors"]["detail"].as_str().unwrap().contains("$0.02"));
    assert_eq!(server.received_requests().await.unwrap().len(), asked);

    // The limit is per member.
    app.log_in(&other).await;
    app.post("/api/ai-rulings", ask.clone())
        .await
        .assert_json(200);

    // Administrators have no limit, though their spending is recorded.
    let admin = app.unique_admin().await;
    app.log_in_sudo(&admin).await;
    for _ in 0..4 {
        app.post("/api/ai-rulings", ask.clone())
            .await
            .assert_json(200);
    }
    let status = app.get("/api/ai-rulings").await.assert_json(200);
    assert_eq!(status["data"]["usage"]["limit_usd"], Value::Null);
    assert!(status["data"]["usage"]["spent_usd"].as_f64().unwrap() > 0.02);
    let body = app.get("/api/admin/ai-settings").await.assert_json(200);
    let usage = body["data"]["usage"].as_array().unwrap();
    assert_eq!(usage.len(), 3);
    assert_eq!(usage[0]["user_id"], admin.id);
    assert_eq!(usage[0]["questions"], 4);
}

#[tokio::test]
async fn questions_need_a_configured_model() {
    let server = MockServer::start().await;
    let app = app(&server).await;
    let member = app.unique_member().await;
    app.log_in(&member).await;
    let body = app
        .post("/api/ai-rulings", json!({ "question": "  " }))
        .await
        .assert_json(422);
    assert_eq!(body["errors"]["question"], json!(["can't be blank"]));
    let body = app
        .post(
            "/api/ai-rulings",
            json!({ "question": "Can I respond to a land drop?" }),
        )
        .await
        .assert_json(503);
    assert!(
        body["errors"]["detail"]
            .as_str()
            .unwrap()
            .contains("not set up")
    );
    assert_eq!(
        app.get("/api/ai-rulings").await.assert_json(200)["data"]["enabled"],
        false
    );

    app.clear_cookies();
    app.post("/api/ai-rulings", json!({ "question": "Hi" }))
        .await
        .assert_json(401);
}
