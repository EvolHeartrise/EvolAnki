// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

use std::sync::LazyLock;

use rand::distr::Distribution;
use rand::distr::Uniform;
use regex::Regex;

#[cfg(test)]
use super::answering::CardAnswer;
use crate::card::Card;
use crate::card::CardId;
use crate::card::CardQueue;
use crate::card::CardType;
use crate::collection::Collection;
use crate::config::StringKey;
use crate::error::Result;
use crate::prelude::*;
use crate::scheduler::timing::is_unix_epoch_timestamp;

impl Card {
    /// Make card due in `days_from_today`.
    /// If card is not a review card, convert it into one.
    /// The interval becomes the time from the last review to the new due date,
    /// and stays 0 if the card has no history.
    fn set_due_date(&mut self, today: u32, next_day_start: i64, days_from_today: u32) {
        let new_due = (today + days_from_today) as i32;
        // Don't set the interval if the card is new or if the card has an interval of 0
        // (to prevent the interval changing if set due date is used several times in a
        // row on a new card.)
        let new_interval = if self.queue == CardQueue::New || self.interval == 0 {
            0
        } else if let Some(last_review_time) = self.last_review_time {
            let elapsed_days = TimestampSecs(next_day_start).elapsed_days_since(last_review_time);
            elapsed_days as u32 + days_from_today
        } else {
            let due = self.original_or_current_due();
            let due_diff = if is_unix_epoch_timestamp(due) {
                let offset = (due as i64 - next_day_start) / 86_400;
                let due = (today as i64 + offset) as i32;
                new_due - due
            } else {
                new_due - due
            };
            self.interval.saturating_add_signed(due_diff)
        };

        self.schedule_as_review(new_interval, new_due);
    }

    fn schedule_as_review(&mut self, interval: u32, due: i32) {
        self.original_position = self.last_position();
        self.remove_from_filtered_deck_before_reschedule();
        self.interval = interval;
        self.due = due;
        self.ctype = CardType::Review;
        self.queue = CardQueue::Review;
        // The revlog row written for this needs a non-zero factor, as a manual row with
        // 0 is what marks a reset.
        self.set_initial_factor_if_missing();
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct DueDateSpecifier {
    min: u32,
    max: u32,
}

pub fn parse_due_date_str(s: &str) -> Result<DueDateSpecifier> {
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?x)^
            # a number
            (?P<min>\d+)
            # an optional hyphen and another number
            (?:
                -
                (?P<max>\d+)
            )?
            # optional exclamation mark, from when it meant a reset of the interval;
            # it may still be in a saved value
            (?P<bang>!)?
            $
        ",
        )
        .unwrap()
    });
    let caps = RE.captures(s).or_invalid(s)?;
    let min: u32 = caps.name("min").unwrap().as_str().parse()?;
    let max = if let Some(max) = caps.name("max") {
        max.as_str().parse()?
    } else {
        min
    };
    Ok(DueDateSpecifier {
        min: min.min(max),
        max: max.max(min),
    })
}

impl Collection {
    /// `days` should be in a format parseable by `parse_due_date_str`.
    /// If `context` is provided, provided key will be updated with the new
    /// value of `days`.
    pub fn set_due_date(
        &mut self,
        cids: &[CardId],
        days: &str,
        context: Option<StringKey>,
    ) -> Result<OpOutput<()>> {
        let spec = parse_due_date_str(days)?;
        let today = self.timing_today()?.days_elapsed;
        let next_day_start = self.timing_today()?.next_day_at.0;
        let mut rng = rand::rng();
        let distribution = Uniform::new_inclusive(spec.min, spec.max).unwrap();
        self.transact(Op::SetDueDate, |col| {
            for mut card in col.all_cards_for_ids(cids, false)? {
                let original = card.clone();
                let days_from_today = distribution.sample(&mut rng);
                card.set_due_date(today, next_day_start, days_from_today);
                col.log_manually_scheduled_review(&card, original.interval)?;
                col.update_card_inner(&mut card, original)?;
            }
            if let Some(key) = context {
                col.set_config_string_inner(key, days)?;
            }
            Ok(())
        })
    }

    /// Answers each card as if from the reviewer, without it being queued. Used
    /// by tests to give cards a review history.
    #[cfg(test)]
    pub(crate) fn grade_now(&mut self, cids: &[CardId], rating: i32) -> Result<OpOutput<()>> {
        self.transact(Op::AnswerCard, |col| {
            for &card_id in cids {
                let states = col.get_scheduling_states(card_id)?;
                let new_state = match rating {
                    0 => states.again,
                    1 => states.hard,
                    2 => states.good,
                    3 => states.easy,
                    _ => invalid_input!("invalid rating"),
                };
                let mut answer: CardAnswer = anki_proto::scheduler::CardAnswer {
                    card_id: card_id.into(),
                    current_state: Some(states.current.into()),
                    new_state: Some(new_state.into()),
                    rating,
                    milliseconds_taken: 0,
                    answered_at_millis: TimestampMillis::now().into(),
                }
                .into();
                // Process the card without updating queues yet
                answer.from_queue = false;
                col.answer_card_inner(&mut answer)?;
            }
            Ok(())
        })
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::prelude::*;

    #[test]
    fn parse() -> Result<()> {
        type S = DueDateSpecifier;
        assert!(parse_due_date_str("").is_err());
        assert!(parse_due_date_str("x").is_err());
        assert!(parse_due_date_str("-5").is_err());
        assert_eq!(parse_due_date_str("5")?, S { min: 5, max: 5 });
        assert_eq!(parse_due_date_str("5!")?, S { min: 5, max: 5 });
        assert_eq!(parse_due_date_str("50-70")?, S { min: 50, max: 70 });
        assert_eq!(parse_due_date_str("70-50!")?, S { min: 50, max: 70 });
        Ok(())
    }

    #[test]
    fn due_date_of_a_new_card() {
        let mut c = Card::new(NoteId(0), 0, DeckId(0), 0);

        // it's converted, with no interval as there's no history, and given a factor
        c.set_due_date(5, 0, 2);
        assert_eq!(c.ctype, CardType::Review);
        assert_eq!(c.queue, CardQueue::Review);
        assert_eq!(c.due, 7);
        assert_eq!(c.interval, 0);
        assert_eq!(c.ease_factor, 2500);

        // setting it again doesn't invent an interval
        c.set_due_date(6, 0, 3);
        assert_eq!(c.due, 9);
        assert_eq!(c.interval, 0);
    }

    #[test]
    fn due_date_of_a_card_with_history() {
        let day = 86_400;
        let mut c = Card::new(NoteId(0), 0, DeckId(0), 0);
        c.ctype = CardType::Review;
        c.queue = CardQueue::Review;
        c.interval = 5;
        c.ease_factor = 1800;
        c.due = 8;

        // without a last review time, it's shifted by how far the due date moves
        c.set_due_date(6, 7 * day, 4);
        assert_eq!(c.due, 10);
        assert_eq!(c.interval, 7);
        // bringing it forward works too
        c.set_due_date(6, 7 * day, 1);
        assert_eq!(c.due, 7);
        assert_eq!(c.interval, 4);
        // a factor that's already there stays
        assert_eq!(c.ease_factor, 1800);

        // with one, it's the time from there to the due date
        c.last_review_time = Some(TimestampSecs(4 * day));
        c.set_due_date(6, 7 * day, 3);
        assert_eq!(c.due, 9);
        assert_eq!(c.interval, 6);

        // should work in a filtered deck, and a missing factor is filled in
        c.interval = 2;
        c.ease_factor = 0;
        c.original_due = 7;
        c.original_deck_id = DeckId(1);
        c.due = -10000;
        c.queue = CardQueue::New;
        c.set_due_date(6, 0, 1);
        assert_eq!(c.due, 7);
        assert_eq!(c.ease_factor, 2500);
        assert_eq!(c.queue, CardQueue::Review);
        assert_eq!(c.original_due, 0);
        assert_eq!(c.original_deck_id, DeckId(0));

        // relearning treated like review
        c.last_review_time = None;
        c.interval = 2;
        c.ctype = CardType::Relearn;
        c.due = 12;
        c.set_due_date(6, 0, 10);
        assert_eq!(c.due, 16);
        assert_eq!(c.interval, 6);
        assert_eq!(c.ctype, CardType::Review);
    }

    /// A manual revlog row with a factor of 0 marks a reset, which would make
    /// FSRS ignore all the history before it.
    #[test]
    fn set_due_on_a_new_card_writes_a_row_that_is_not_a_reset() -> Result<()> {
        let mut col = Collection::new();
        let nt = col.get_notetype_by_name("Basic")?.unwrap();
        let mut note = nt.new_note();
        col.add_note(&mut note, DeckId(1))?;
        let cid = col.get_first_card().id;
        assert_eq!(col.get_first_card().ease_factor, 0);

        col.set_due_date(&[cid], "7", None)?;

        let revlog = col.storage.get_revlog_entries_for_card(cid)?;
        assert_eq!(revlog.len(), 1);
        assert_eq!(revlog[0].ease_factor, 2500);
        assert!(!revlog[0].is_reset());

        Ok(())
    }
}
