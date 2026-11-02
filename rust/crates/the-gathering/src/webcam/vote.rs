//! Table votes: a seat asks a question with two or more choices, and every player still in the
//! game who is at the table (or only the asker's opponents) answers once. Nobody sees a choice
//! until the vote closes, when everyone gets the totals. A secret vote never reveals who chose
//! what; an open one lists each voter's choice with the results.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// The most choices a vote may offer.
pub const MAX_CHOICES: usize = 20;
const MAX_QUESTION_CHARS: usize = 200;
const MAX_CHOICE_CHARS: usize = 100;

/// Who answers a vote.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Audience {
    /// Every player, the asker included.
    Everyone,
    /// Everyone but the asker.
    Opponents,
}

/// A `start_vote` request.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VoteRequest {
    /// The question.
    pub question: String,
    /// The answers to choose from, in order.
    pub choices: Vec<String>,
    /// Hide who chose what.
    pub secret: bool,
    /// Who answers.
    pub audience: Audience,
}

impl VoteRequest {
    /// The request with its text trimmed, or `None` when a field is missing, unknown or out of
    /// range, or two choices are the same.
    pub fn parse(payload: &Value) -> Option<Self> {
        let request = serde_json::from_value::<Self>(payload.clone()).ok()?;
        let fits = |text: &str, max: usize| (1..=max).contains(&text.chars().count());
        let question = request.question.trim().to_owned();
        let choices: Vec<String> = request
            .choices
            .iter()
            .map(|choice| choice.trim().to_owned())
            .collect();
        let distinct = choices
            .iter()
            .enumerate()
            .all(|(index, choice)| !choices.iter().take(index).any(|other| other == choice));
        (fits(&question, MAX_QUESTION_CHARS)
            && (2..=MAX_CHOICES).contains(&choices.len())
            && choices.iter().all(|choice| fits(choice, MAX_CHOICE_CHARS))
            && distinct)
            .then_some(Self {
                question,
                choices,
                ..request
            })
    }
}

/// A player taking part in a vote.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Voter {
    /// Player id (stable across reconnects, unlike the peer id).
    pub player_id: i64,
    /// Player name.
    pub player_name: String,
}

/// The table's current or latest vote, saved with the room.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Vote {
    /// A UUID, so answers cannot land in a later vote.
    pub id: String,
    /// The question.
    pub question: String,
    /// The answers, in order.
    pub choices: Vec<String>,
    /// Hide who chose what.
    pub secret: bool,
    /// Who answers.
    pub audience: Audience,
    /// Who asked.
    pub creator: Voter,
    /// Who answers, in seat order, fixed when the vote starts.
    pub voters: Vec<Voter>,
    /// Choice index by player id. Only totals leave the server until an open vote closes.
    pub ballots: BTreeMap<i64, usize>,
    /// Every voter answered, or the asker or host ended it.
    pub closed: bool,
}

impl Vote {
    /// A new open vote.
    pub fn new(request: VoteRequest, creator: Voter, voters: Vec<Voter>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            question: request.question,
            choices: request.choices,
            secret: request.secret,
            audience: request.audience,
            creator,
            voters,
            ballots: BTreeMap::new(),
            closed: false,
        }
    }

    /// Records `player_id`'s one answer; the last answer closes the vote.
    pub fn cast(&mut self, player_id: i64, choice: usize) -> Result<(), &'static str> {
        if self.closed {
            return Err("this vote has ended");
        }
        if !self.voters.iter().any(|voter| voter.player_id == player_id) {
            return Err("you are not voting in this vote");
        }
        if self.ballots.contains_key(&player_id) {
            return Err("you already voted");
        }
        if choice >= self.choices.len() {
            return Err("invalid choice");
        }
        self.ballots.insert(player_id, choice);
        self.closed = self.ballots.len() == self.voters.len();
        Ok(())
    }

    /// Answers per choice, in choice order.
    pub fn tally(&self) -> Vec<usize> {
        let mut tally = vec![0; self.choices.len()];
        for choice in self.ballots.values() {
            if let Some(count) = tally.get_mut(*choice) {
                *count += 1;
            }
        }
        tally
    }

    /// What every seat is sent: who has answered, then the totals once closed, and each
    /// voter's choice only when the vote is closed and not secret.
    pub fn view(&self) -> Value {
        let reveal = self.closed && !self.secret;
        let voters: Vec<Value> = self
            .voters
            .iter()
            .map(|voter| {
                let choice = self.ballots.get(&voter.player_id);
                json!({
                    "player_id": voter.player_id,
                    "player_name": voter.player_name,
                    "voted": choice.is_some(),
                    "choice": if reveal { choice } else { None },
                })
            })
            .collect();
        json!({
            "id": self.id,
            "question": self.question,
            "choices": self.choices,
            "secret": self.secret,
            "audience": self.audience,
            "creator": self.creator,
            "voters": voters,
            "closed": self.closed,
            "tally": self.closed.then(|| self.tally()),
        })
    }
}
