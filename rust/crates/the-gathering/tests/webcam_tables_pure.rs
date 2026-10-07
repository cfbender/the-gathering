//! Ported from `test/the_gathering/webcam_tables/{timer,turns,log}_test.exs` and
//! `test/the_gathering_web/channel_rate_limit_test.exs`.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::collections::BTreeMap;
use std::time::Duration;

use serde_json::json;
use the_gathering::config::BucketLimit;
use the_gathering::rate_limit::TokenBucket;
use the_gathering::webcam::Mode;
use the_gathering::webcam::log::{self, LifeChange, LogEntry, RollKind, RollResult};
use the_gathering::webcam::seat::{CombatEffect, CustomCounter, Holder, Seat};
use the_gathering::webcam::timer::{Action, Timer};
use the_gathering::webcam::turns::{self, Turns, Unit};

// ---- Timer ----

#[test]
fn timer_transitions_account_for_multiple_unequal_pauses_without_resetting() {
    let timer = Timer::new();
    assert_eq!(timer.update(Action::Resume, 10), timer);
    assert!(!timer.awaiting_start());
    let timer = timer.update(Action::Start, 1000);
    assert_eq!(
        timer,
        Timer {
            started_at: Some(1000),
            paused_at: Some(1000),
            paused_ms: 0
        }
    );
    assert!(timer.awaiting_start());
    assert_eq!(timer.elapsed(5000), 0);
    assert_eq!(timer.update(Action::Pause, 5000), timer);
    let timer = timer.update(Action::Resume, 5000);
    assert!(!timer.awaiting_start());
    assert_eq!(timer.elapsed(5000), 0);
    let timer = timer.update(Action::Pause, 13_000);
    assert_eq!(timer.update(Action::Pause, 20_000), timer);
    let timer = timer.update(Action::Resume, 22_000);
    assert_eq!(timer.paused_ms, 13_000);
    let timer = timer
        .update(Action::Pause, 41_000)
        .update(Action::Resume, 46_000);
    assert_eq!(
        timer,
        Timer {
            started_at: Some(1000),
            paused_at: None,
            paused_ms: 18_000
        }
    );
    assert_eq!(timer.update(Action::Start, 50_000), timer);
}

// ---- Turns ----

fn unit(id: i64, eliminated: bool) -> Unit {
    Unit {
        player_id: id,
        eliminated,
        departed: false,
    }
}

fn seat(id: i64) -> Unit {
    unit(id, false)
}

fn map(pairs: &[(i64, i64)]) -> BTreeMap<i64, i64> {
    pairs.iter().copied().collect()
}

const COMMANDER: Mode = Mode::Commander;

#[test]
fn starts_counts_on_entry_skips_out_and_departed_seats_and_wraps() {
    let seats = [
        seat(1),
        unit(2, true),
        Unit {
            departed: true,
            ..seat(3)
        },
        seat(4),
    ];
    let turns = turns::reconcile(&Turns::default(), &seats, 0, COMMANDER);
    assert_eq!(turns.active_player_id, Some(1));
    assert_eq!(turns.counts, map(&[(1, 1)]));
    let turns = turns::pass(&turns, &seats, 12_000, COMMANDER);
    assert_eq!(turns.active_player_id, Some(4));
    assert_eq!(turns.counts, map(&[(1, 1), (4, 1)]));
    let turns = turns::pass(&turns, &seats, 31_000, COMMANDER);
    assert_eq!(turns.active_player_id, Some(1));
    assert_eq!(turns.counts, map(&[(1, 2), (4, 1)]));
    assert_eq!(turns.elapsed_ms, map(&[(1, 12_000), (4, 19_000)]));
    assert_eq!(turns.started_elapsed_ms, 31_000);
}

#[test]
fn turn_times_exclude_pauses_including_passing_while_paused() {
    let seats = [seat(1), seat(2)];
    let timer = Timer::new()
        .update(Action::Start, 500)
        .update(Action::Resume, 1000);
    let turns = turns::pass(&Turns::default(), &seats, 0, COMMANDER);
    let timer = timer.update(Action::Pause, 13_000);
    let turns = turns::pass(&turns, &seats, timer.elapsed(19_000), COMMANDER);
    assert_eq!(turns.elapsed_ms, map(&[(1, 12_000)]));
    let timer = timer.update(Action::Resume, 22_000);
    let turns = turns::pass(&turns, &seats, timer.elapsed(41_000), COMMANDER);
    assert_eq!(turns.elapsed_ms, map(&[(1, 12_000), (2, 19_000)]));
    assert_eq!(turns.started_elapsed_ms, 31_000);
    assert_eq!(turns.counts, map(&[(1, 2), (2, 1)]));
}

#[test]
fn elimination_advances_once_no_survivors_clears_the_turn_and_restoration_starts_it() {
    let seats = [seat(1), seat(2)];
    let turns = turns::pass(&Turns::default(), &seats, 0, COMMANDER);
    let turns = turns::reconcile(&turns, &[unit(1, true), seat(2)], 7000, COMMANDER);
    assert_eq!(turns.active_player_id, Some(2));
    assert_eq!(turns.elapsed_ms, map(&[(1, 7000)]));
    assert_eq!(
        turns::reconcile(&turns, &[unit(1, true), seat(2)], 9000, COMMANDER),
        turns
    );
    let turns = turns::reconcile(&turns, &[unit(1, true), unit(2, true)], 11_000, COMMANDER);
    assert_eq!(turns.active_player_id, None);
    assert_eq!(turns.elapsed_ms, map(&[(1, 7000), (2, 4000)]));
    assert_eq!(
        turns::reconcile(&turns, &[unit(1, true), unit(2, true)], 15_000, COMMANDER),
        turns
    );
    let turns = turns::reconcile(&turns, &[seat(1), unit(2, true)], 20_000, COMMANDER);
    assert_eq!(turns.active_player_id, Some(1));
    assert_eq!(turns.counts, map(&[(1, 2), (2, 1)]));
    assert_eq!(turns.started_elapsed_ms, 20_000);
}

#[test]
fn corrections_clamp_at_zero_and_999_without_changing_turn_timing() {
    let turns = turns::pass(&Turns::default(), &[seat(1)], 0, COMMANDER);
    let adjusted = turns::adjust(&turns::adjust(&turns, 1, -1), 1, -1);
    assert_eq!(
        adjusted,
        Turns {
            counts: map(&[(1, 0)]),
            ..turns.clone()
        }
    );
    let maxed = Turns {
        counts: map(&[(1, 999)]),
        ..turns
    };
    assert_eq!(turns::adjust(&maxed, 1, 1).counts, map(&[(1, 999)]));
}

#[test]
fn two_headed_giant_groups_before_filtering_accounts_once_per_team_and_skips_eliminated_teams() {
    let mode = Mode::TwoHeadedGiant;
    let seats = [
        seat(8),
        seat(3),
        unit(17, true),
        unit(5, true),
        seat(2),
        seat(11),
    ];
    let turns = turns::reconcile(&Turns::default(), &seats, 0, mode);
    assert_eq!(turns.active_player_id, Some(8));
    assert_eq!(turns::next_player(&seats, Some(3), mode), Some(2));
    let turns = turns::pass(&turns, &seats, 7000, mode);
    assert_eq!(turns.counts, map(&[(8, 1), (2, 1)]));
    assert_eq!(turns.elapsed_ms, map(&[(8, 7000)]));
    let turns = turns::pass(&turns, &seats, 19_000, mode);
    assert_eq!(turns.active_player_id, Some(8));
    assert_eq!(turns.elapsed_ms, map(&[(8, 7000), (2, 12_000)]));
    assert_eq!(turns::turn_id(&seats, 11, mode), 2);
    assert_eq!(
        turns::adjust(&turns, turns::turn_id(&seats, 3, mode), 1).counts,
        map(&[(8, 3), (2, 1)])
    );
    let all_out: Vec<Unit> = seats
        .iter()
        .map(|seat| Unit {
            eliminated: true,
            ..*seat
        })
        .collect();
    assert_eq!(
        turns::reconcile(&turns, &all_out, 21_000, mode).active_player_id,
        None
    );
}

#[test]
fn unpass_resumes_the_previous_turn_removing_the_banked_time_and_the_extra_count() {
    let seats = [seat(1), seat(2), seat(3)];
    let started = turns::reconcile(&Turns::default(), &seats, 0, COMMANDER);
    let first = turns::pass(&started, &seats, 10_000, COMMANDER);
    let second = turns::pass(&first, &seats, 25_000, COMMANDER);
    assert_eq!(second.active_player_id, Some(3));

    let undone = turns::unpass(&second, &seats, COMMANDER).unwrap();
    assert_eq!(undone.active_player_id, Some(2));
    assert_eq!(undone.counts, map(&[(1, 1), (2, 1), (3, 0)]));
    assert_eq!(undone.elapsed_ms, map(&[(1, 10_000), (2, 0)]));
    // Time since the mistaken pass keeps counting toward player 2's resumed turn.
    assert_eq!(undone.started_elapsed_ms, 10_000);
    assert_eq!(undone.revision, second.revision + 1);

    let back = turns::unpass(&undone, &seats, COMMANDER).unwrap();
    assert_eq!(back.active_player_id, Some(1));
    assert_eq!(back.elapsed_ms, map(&[(1, 0), (2, 0)]));
    assert_eq!(back.started_elapsed_ms, 0);
    assert_eq!(turns::unpass(&back, &seats, COMMANDER), None);
}

#[test]
fn unpass_refuses_when_the_previous_player_is_out_or_the_turn_restarted_from_nobody() {
    let seats = [seat(1), seat(2)];
    let turns = turns::pass(
        &turns::reconcile(&Turns::default(), &seats, 0, COMMANDER),
        &seats,
        5000,
        COMMANDER,
    );
    assert_eq!(
        turns::unpass(&turns, &[unit(1, true), seat(2)], COMMANDER),
        None
    );

    let cleared = turns::reconcile(&turns, &[unit(1, true), unit(2, true)], 8000, COMMANDER);
    let restarted = turns::reconcile(&cleared, &[seat(1), unit(2, true)], 9000, COMMANDER);
    assert_eq!(restarted.active_player_id, Some(1));
    assert_eq!(turns::unpass(&restarted, &seats, COMMANDER), None);
}

#[test]
fn pass_history_is_bounded() {
    let seats = [seat(1), seat(2)];
    let turns = (1..=30).fold(
        turns::reconcile(&Turns::default(), &seats, 0, COMMANDER),
        |turns, n| turns::pass(&turns, &seats, n * 1000, COMMANDER),
    );
    assert_eq!(turns.history.len(), 20);
}

// ---- Log ----

fn log_seat() -> Seat {
    Seat::new("a".into(), 1, "Alice".into(), 0)
}

fn life(from: i64, to: i64, actor: &str, name: &str) -> LogEntry {
    LogEntry {
        text: format!("{name}: {from} → {to} life"),
        actor: Some(actor.into()),
        kind: Some("life".into()),
        life: Some(LifeChange {
            name: name.into(),
            from,
            to,
        }),
        ..LogEntry::default()
    }
}

fn alice_life(from: i64, to: i64) -> LogEntry {
    life(from, to, "a", "Alice")
}

fn texts(entries: &[LogEntry]) -> Vec<&str> {
    entries.iter().map(|entry| entry.text.as_str()).collect()
}

fn text(line: &str) -> LogEntry {
    LogEntry {
        text: line.into(),
        ..LogEntry::default()
    }
}

#[test]
fn coalesces_rapid_life_changes_from_the_original_total_through_the_final_total() {
    let log = log::append(&[], alice_life(40, 39), 1000);
    let log = log::append(&log, alice_life(39, 38), 1500);
    let log = log::append(&log, alice_life(38, 37), 2000);
    assert_eq!(log.len(), 1);
    assert_eq!(
        (log[0].id, log[0].text.as_str(), log[0].count, log[0].at),
        (1, "Alice: 40 → 37 life", Some(3), 2000)
    );
}

#[test]
fn merges_at_the_window_boundary_but_not_beyond_it_backwards_in_time_or_across_table_events() {
    let first = log::append(&[], alice_life(40, 39), 1000);
    assert_eq!(log::append(&first, alice_life(39, 35), 6000).len(), 1);
    assert_eq!(log::append(&first, alice_life(39, 35), 6001).len(), 2);
    assert_eq!(log::append(&first, alice_life(39, 35), 999).len(), 2);

    let log = log::append(&first, text("Seat order randomized"), 1100);
    let ids: Vec<i64> = log::append(&log, alice_life(39, 37), 1200)
        .iter()
        .map(|entry| entry.id)
        .collect();
    assert_eq!(ids, [3, 2, 1]);
}

#[test]
fn keeps_one_line_per_player_when_several_change_life_at_once() {
    let changes = [
        (life(40, 37, "a", "Alice"), 1000),
        (life(40, 37, "b", "Bob"), 1100),
        (life(40, 37, "c", "Cara"), 1200),
        (life(37, 34, "a", "Alice"), 1300),
        (life(37, 34, "b", "Bob"), 1400),
        (life(34, 31, "a", "Alice"), 1500),
        (life(37, 34, "c", "Cara"), 1600),
    ];
    let log = changes.into_iter().fold(Vec::new(), |log, (content, at)| {
        log::append(&log, content, at)
    });
    let summary: Vec<(i64, &str, Option<i64>, i64)> = log
        .iter()
        .map(|entry| (entry.id, entry.text.as_str(), entry.count, entry.at))
        .collect();
    assert_eq!(
        summary,
        [
            (3, "Cara: 40 → 34 life", Some(2), 1600),
            (2, "Bob: 40 → 34 life", Some(2), 1400),
            (1, "Alice: 40 → 31 life", Some(3), 1500),
        ]
    );

    // The window runs from each player's latest change.
    let log = log::append(&log, alice_life(31, 28), 6500);
    assert_eq!(
        log.iter().map(|entry| entry.id).collect::<Vec<_>>(),
        [3, 2, 1]
    );
    assert_eq!(
        (log[2].text.as_str(), log[2].count),
        ("Alice: 40 → 28 life", Some(4))
    );
}

#[test]
fn coalesces_counters_per_player_alongside_the_life_changes_they_come_with() {
    let bob = Seat::new("b".into(), 2, "Bob".into(), 0);
    let hit = |from: i64, to: i64| {
        let with = |damage: i64| {
            let mut seat = log_seat();
            seat.life = 40 - damage;
            seat.commander_damage =
                [("2".to_owned(), [("Kangee".to_owned(), damage)].into())].into();
            seat
        };
        log::seat_changes(&with(from), &with(to), std::slice::from_ref(&bob))
    };
    let mut log = Vec::new();
    for (index, contents) in [hit(0, 1), hit(1, 2), hit(2, 3)].into_iter().enumerate() {
        for content in contents {
            log = log::append(&log, content, 1000 + i64::try_from(index).unwrap() * 100);
        }
    }
    let summary: Vec<(&str, Option<i64>)> = log
        .iter()
        .map(|entry| (entry.text.as_str(), entry.count))
        .collect();
    assert_eq!(
        summary,
        [
            ("Alice damage from Bob's Kangee: 0 → 3", Some(3)),
            ("Alice: 40 → 37 life", Some(3))
        ]
    );
}

#[test]
fn preserves_every_roll_result_and_caps_the_history() {
    let roll = |result: i64| {
        log::roll(
            RollKind::Dice(20),
            &RollResult::Number(result),
            "a",
            "Alice",
        )
    };
    let log = log::append(&log::append(&[], roll(17), 1000), roll(3), 1100);
    assert_eq!(log.len(), 1);
    assert_eq!(
        (log[0].text.as_str(), log[0].count),
        ("Alice rolled a d20: 17, 3", Some(2))
    );

    let log = (1..=250).fold(Vec::new(), |log, n| {
        log::append(&log, text(&format!("line {n}")), n)
    });
    assert_eq!(log.len(), 200);
    assert_eq!(
        log[0],
        LogEntry {
            id: 250,
            at: 250,
            ..text("line 250")
        }
    );
    assert_eq!(
        serde_json::to_value(&log[0]).unwrap(),
        json!({ "id": 250, "at": 250, "text": "line 250" })
    );
}

#[test]
fn describes_every_changed_seat_fact_and_nothing_else() {
    let before = log_seat();
    let mut after = log_seat();
    after.life = 37;
    after.deck_id = Some(4);
    after.deck_name = Some("Birds".into());
    after.camera_off = true;
    after.poison = 10;
    after.commander_casts = [("Tymna".to_owned(), 2)].into();
    after.commander_damage = [("2".to_owned(), [("Kangee".to_owned(), 21)].into())].into();
    let seats = [after.clone(), Seat::new("b".into(), 2, "Bob".into(), 0)];

    assert_eq!(
        texts(&log::seat_changes(&before, &after, &seats)),
        [
            "Alice chose Birds",
            "Alice: 40 → 37 life",
            "Alice turned their camera off",
            "Alice poison: 0 → 10",
            "Alice Tymna commander tax: 0 → 4",
            "Alice damage from Bob's Kangee: 0 → 21",
        ]
    );
    assert!(log::seat_changes(&after, &after, &seats).is_empty());

    let mut damaged = log_seat();
    damaged.commander_damage = [("9".to_owned(), [("Kangee".to_owned(), 21)].into())].into();
    assert!(
        texts(&log::seat_changes(&damaged, &before, &[]))
            .contains(&"Alice damage from player 9's Kangee: 21 → 0")
    );
}

#[test]
fn logs_shared_custom_counters_by_id_ignoring_renames_removals_and_combat_buffs() {
    let counter = |id: &str, label: &str, value: i64| CustomCounter {
        id: id.into(),
        label: label.into(),
        value,
    };
    let mut before = log_seat();
    before.custom_counters = vec![counter("c1", "Lands", 6), counter("c2", "Storm", 3)];
    // A seat saved before custom counters existed has none.
    let legacy = log_seat();
    let mut after = log_seat();
    after.custom_counters = vec![counter("c1", "Lands", 7), counter("c2", "Storm count", 3)];
    after.combat_effects = vec![CombatEffect {
        id: "e1".into(),
        name: "Anthem".into(),
        power: 1,
        toughness: 1,
        conditions: Vec::new(),
        keywords: Vec::new(),
    }];

    assert_eq!(
        texts(&log::seat_changes(&before, &after, &[])),
        ["Alice Lands: 6 → 7"]
    );
    assert_eq!(
        texts(&log::seat_changes(&legacy, &after, &[])),
        ["Alice Lands: 0 → 7", "Alice Storm count: 0 → 3"]
    );
    assert!(log::seat_changes(&after, &log_seat(), &[]).is_empty());
    assert!(log::seat_changes(&after, &legacy, &[]).is_empty());
}

#[test]
fn names_joins_leaves_eliminations_the_monarch_and_seat_order() {
    assert_eq!(log::joined("Alice").text, "Alice joined the table");
    assert_eq!(log::left("Alice").text, "Alice left the table");
    assert_eq!(
        log::elimination(&log_seat(), true).text,
        "Alice was eliminated"
    );
    assert_eq!(
        log::elimination(&log_seat(), false).text,
        "Alice was restored to the game"
    );
    let alice = Holder {
        peer_id: "a".into(),
        player_name: "Alice".into(),
    };
    let bob = Holder {
        peer_id: "b".into(),
        player_name: "Bob".into(),
    };
    assert_eq!(log::monarch(&alice, &alice).text, "Alice took the monarch");
    assert_eq!(
        log::monarch(&alice, &bob).text,
        "Bob gave Alice the monarch"
    );
    assert_eq!(log::seat_order(true, false).text, "Seat order randomized");
    assert_eq!(
        log::seat_order(false, false).text,
        "Game started in seat order"
    );
    assert_eq!(log::seat_order(false, true).text, "Seat order changed");
}

// ---- ChannelRateLimit ----

/// 20 events per second with a burst of 60, the production event bucket.
fn bucket() -> TokenBucket {
    TokenBucket::new(BucketLimit {
        capacity: 60.0,
        refill_per_second: 20.0,
    })
}

#[test]
fn allows_a_burst_up_to_capacity_then_refuses_until_tokens_refill() {
    let mut bucket = bucket();
    let start = bucket.at();
    for _ in 0..60 {
        assert!(bucket.take_at(start));
    }
    assert!(!bucket.take_at(start));
    assert!(!bucket.take_at(start + Duration::from_millis(49)));
    assert!(bucket.take_at(start + Duration::from_millis(50)));
    assert!(!bucket.take_at(start + Duration::from_millis(50)));
}

#[test]
fn sustained_rapid_life_tapping_never_runs_dry() {
    // Ten minutes of tapping at 15 per second.
    let mut bucket = bucket();
    let start = bucket.at();
    for tap in 1..=9_000u64 {
        assert!(bucket.take_at(start + Duration::from_millis(tap * 1000 / 15)));
    }
    assert!(bucket.tokens() > 50.0);
}

#[test]
fn idle_time_never_refills_beyond_capacity() {
    let mut bucket = bucket();
    let start = bucket.at();
    for _ in 0..60 {
        assert!(bucket.take_at(start));
    }
    assert!(bucket.take_at(start + Duration::from_secs(3600)));
    assert!((bucket.tokens() - 59.0).abs() < f64::EPSILON);
}
