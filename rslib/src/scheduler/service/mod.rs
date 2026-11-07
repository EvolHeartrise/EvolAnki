// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

mod answering;
mod states;

use anki_proto::generic;
use anki_proto::scheduler;
use anki_proto::scheduler::ComputeOptimalRetentionResponse;
use anki_proto::scheduler::SimulateFsrsReviewRequest;
use anki_proto::scheduler::SimulateFsrsReviewResponse;
use anki_proto::scheduler::SimulateFsrsWorkloadResponse;

use crate::prelude::*;
use crate::scheduler::fsrs::params::ComputeParamsRequest;

impl crate::services::SchedulerService for Collection {
    fn sched_timing_today(&mut self) -> Result<scheduler::SchedTimingTodayResponse> {
        Ok(self.timing_today()?.into())
    }

    /// Fetch data from DB and return rendered string.
    fn studied_today(&mut self) -> Result<generic::String> {
        self.studied_today().map(Into::into)
    }

    fn unsuspend_cards(
        &mut self,
        input: anki_proto::cards::CardIds,
    ) -> Result<anki_proto::collection::OpChanges> {
        let cids: Vec<_> = input.cids.into_iter().map(CardId).collect();
        self.unsuspend_cards(&cids).map(Into::into)
    }

    fn suspend_cards(
        &mut self,
        input: scheduler::SuspendCardsRequest,
    ) -> Result<anki_proto::collection::OpChangesWithCount> {
        let cids = if input.card_ids.is_empty() {
            self.storage
                .card_ids_of_notes(&input.note_ids.into_newtype(NoteId))?
        } else {
            input.card_ids.into_newtype(CardId)
        };
        self.suspend_cards(&cids).map(Into::into)
    }

    fn empty_filtered_deck(
        &mut self,
        input: anki_proto::decks::DeckId,
    ) -> Result<anki_proto::collection::OpChanges> {
        self.empty_filtered_deck(input.did.into()).map(Into::into)
    }

    fn rebuild_filtered_deck(
        &mut self,
        input: anki_proto::decks::DeckId,
    ) -> Result<anki_proto::collection::OpChangesWithCount> {
        self.rebuild_filtered_deck(input.did.into()).map(Into::into)
    }

    fn set_due_date(
        &mut self,
        input: scheduler::SetDueDateRequest,
    ) -> Result<anki_proto::collection::OpChanges> {
        let cids = input.card_ids.into_newtype(CardId);
        self.set_due_date(&cids, &input.days).map(Into::into)
    }

    fn answer_card(
        &mut self,
        input: scheduler::CardAnswer,
    ) -> Result<anki_proto::collection::OpChanges> {
        self.answer_card(&mut input.into()).map(Into::into)
    }

    fn get_queued_cards(
        &mut self,
        input: scheduler::GetQueuedCardsRequest,
    ) -> Result<scheduler::QueuedCards> {
        self.get_queued_cards(input.fetch_limit as usize, input.intraday_learning_only)
            .map(Into::into)
    }

    fn custom_study(
        &mut self,
        input: scheduler::CustomStudyRequest,
    ) -> Result<anki_proto::collection::OpChanges> {
        self.custom_study(input).map(Into::into)
    }

    fn compute_fsrs_params(
        &mut self,
        input: scheduler::ComputeFsrsParamsRequest,
    ) -> Result<scheduler::ComputeFsrsParamsResponse> {
        self.compute_params(ComputeParamsRequest {
            search: &input.search,
            ignore_revlogs_before_ms: input.ignore_revlogs_before_ms.into(),
            current_deck: 1,
            total_decks: 1,
            current_params: &input.current_params,
            num_of_relearning_steps: input.num_of_relearning_steps as usize,
            health_check: input.health_check,
        })
    }

    fn simulate_fsrs_review(
        &mut self,
        input: SimulateFsrsReviewRequest,
    ) -> Result<SimulateFsrsReviewResponse> {
        self.simulate_review(input)
    }

    fn simulate_fsrs_workload(
        &mut self,
        input: SimulateFsrsReviewRequest,
    ) -> Result<SimulateFsrsWorkloadResponse> {
        self.simulate_workload(input)
    }

    fn compute_optimal_retention(
        &mut self,
        input: SimulateFsrsReviewRequest,
    ) -> Result<ComputeOptimalRetentionResponse> {
        Ok(ComputeOptimalRetentionResponse {
            optimal_retention: self.compute_optimal_retention(input)?,
        })
    }

    fn evaluate_params_legacy(
        &mut self,
        input: scheduler::EvaluateParamsLegacyRequest,
    ) -> Result<scheduler::EvaluateParamsResponse> {
        let ret = self.evaluate_params_legacy(
            &input.params,
            &input.search,
            input.ignore_revlogs_before_ms.into(),
        )?;
        Ok(scheduler::EvaluateParamsResponse {
            log_loss: ret.log_loss,
            rmse_bins: ret.rmse_bins,
        })
    }
}

impl From<crate::scheduler::timing::SchedTimingToday> for scheduler::SchedTimingTodayResponse {
    fn from(t: crate::scheduler::timing::SchedTimingToday) -> Self {
        scheduler::SchedTimingTodayResponse {
            days_elapsed: t.days_elapsed,
            next_day_at: t.next_day_at.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use anki_proto::scheduler::card_answer::Rating;
    use anki_proto::scheduler::CardAnswer;
    use anki_proto::scheduler::GetQueuedCardsRequest;

    use crate::card::CardQueue;
    use crate::card::CardType;
    use crate::prelude::*;
    use crate::services::SchedulerService;
    use crate::tests::NoteAdder;

    /// The default deck's (new, review) counts studied today.
    fn studied_in_default_deck(col: &mut Collection) -> (i32, i32) {
        let today = col.timing_today().unwrap().days_elapsed;
        let deck = col.storage.get_deck(DeckId(1)).unwrap().unwrap();
        deck.new_rev_counts(today)
    }

    fn add_basic_card(col: &mut Collection) -> CardId {
        let note = NoteAdder::basic(col).add(col);
        col.storage.card_ids_of_notes(&[note.id]).unwrap()[0]
    }

    /// Answers the top queued card through the SchedulerService trait with the
    /// given rating, returning the answered card's id. Building the request
    /// from `get_queued_cards` mirrors what the front-end does.
    fn answer_top_card(col: &mut Collection, rating: Rating) -> i64 {
        let queued = SchedulerService::get_queued_cards(
            col,
            GetQueuedCardsRequest {
                fetch_limit: 1,
                intraday_learning_only: false,
            },
        )
        .unwrap();
        let card = queued.cards[0].card.as_ref().unwrap();
        let _ = SchedulerService::answer_card(
            col,
            CardAnswer {
                card_id: card.id,
                rating: rating as i32,
                answered_at_millis: TimestampMillis::now().0,
                milliseconds_taken: 0,
                card_mtime_secs: card.mtime_secs,
                card_reps: card.reps,
            },
        )
        .unwrap();
        card.id
    }

    #[test]
    fn custom_study_extends_new_limit() {
        let mut col = Collection::new();

        let _ = SchedulerService::custom_study(
            &mut col,
            anki_proto::scheduler::CustomStudyRequest {
                deck_id: 1,
                value: Some(anki_proto::scheduler::custom_study_request::Value::NewLimitDelta(5)),
            },
        )
        .unwrap();

        let (new, _) = studied_in_default_deck(&mut col);
        assert_eq!(new, -5, "new limit was extended by the delta");
    }

    #[test]
    fn extend_limits_subtracts_from_studied_counts() {
        let mut col = Collection::new();

        // Pretend 5 new / 5 review cards were already studied today.
        let today = col.timing_today().unwrap().days_elapsed;
        col.update_deck_stats(
            today,
            anki_proto::scheduler::UpdateStatsRequest {
                deck_id: 1,
                new_delta: 5,
                review_delta: 5,
                millisecond_delta: 0,
            },
        )
        .unwrap();

        // Extending the limits lowers the studied counts, freeing up more cards.
        col.transact_no_undo(|col| col.extend_limits(today, DeckId(1), 2, 1))
            .unwrap();

        let (new, review) = studied_in_default_deck(&mut col);
        assert_eq!(new, 3, "5 studied minus a limit extension of 2");
        assert_eq!(review, 4, "5 studied minus a limit extension of 1");
    }

    #[test]
    fn answer_again_moves_new_card_to_learn_queue() {
        let mut col = Collection::new();
        NoteAdder::basic(&mut col).add(&mut col);

        let card_id = answer_top_card(&mut col, Rating::Again);

        let card = col.storage.get_card(CardId(card_id)).unwrap().unwrap();
        assert_eq!(
            card.queue,
            CardQueue::Learn,
            "Again should send the card to learning"
        );
        assert_eq!(card.ctype, CardType::Learn);
    }

    #[test]
    fn answer_hard_keeps_new_card_in_learning_with_longer_delay_than_again() {
        // Hard stays on the same learning step but applies a longer delay than
        // Again: for the default steps [1min, 10min], Again uses step-1 (60s)
        // while Hard uses the average of step-1 and step-2 (330s).
        let (due_again, queue_again, ctype_again) = {
            let mut col = Collection::new();
            NoteAdder::basic(&mut col).add(&mut col);
            let cid = CardId(answer_top_card(&mut col, Rating::Again));
            let card = col.storage.get_card(cid).unwrap().unwrap();
            (card.due, card.queue, card.ctype)
        };

        let (due_hard, queue_hard, ctype_hard) = {
            let mut col = Collection::new();
            NoteAdder::basic(&mut col).add(&mut col);
            let cid = CardId(answer_top_card(&mut col, Rating::Hard));
            let card = col.storage.get_card(cid).unwrap().unwrap();
            (card.due, card.queue, card.ctype)
        };

        assert_eq!(queue_again, CardQueue::Learn);
        assert_eq!(ctype_again, CardType::Learn);
        assert_eq!(queue_hard, CardQueue::Learn);
        assert_eq!(ctype_hard, CardType::Learn);
        assert!(
            due_hard > due_again,
            "Hard delay ({due_hard}s) should be longer than Again delay ({due_again}s)"
        );
    }

    #[test]
    fn answer_good_repeatedly_graduates_card_through_learning_steps() {
        let mut col = Collection::new();
        NoteAdder::basic(&mut col).add(&mut col);

        // First Good: new -> learning (advances one step, stays in Learn).
        let card_id = answer_top_card(&mut col, Rating::Good);
        let card = col.storage.get_card(CardId(card_id)).unwrap().unwrap();
        assert_eq!(
            card.queue,
            CardQueue::Learn,
            "first Good keeps the card in learning"
        );
        assert_eq!(card.ctype, CardType::Learn);

        // Force the learning card to be due now so it re-enters the queue,
        // then answer Good again to graduate it.
        col.storage
            .db
            .execute_batch("UPDATE cards SET due = 0")
            .unwrap();
        col.clear_study_queues();

        answer_top_card(&mut col, Rating::Good);
        let card = col.storage.get_card(CardId(card_id)).unwrap().unwrap();
        assert_eq!(
            card.queue,
            CardQueue::Review,
            "second Good graduates the card"
        );
        assert_eq!(card.ctype, CardType::Review);
    }

    #[test]
    fn answer_easy_graduates_new_card_to_review_queue() {
        let mut col = Collection::new();
        NoteAdder::basic(&mut col).add(&mut col);

        let card_id = answer_top_card(&mut col, Rating::Easy);

        let card = col.storage.get_card(CardId(card_id)).unwrap().unwrap();
        assert_eq!(
            card.queue,
            CardQueue::Review,
            "Easy on a new card should graduate it straight to review"
        );
        assert_eq!(card.ctype, CardType::Review);
        assert!(
            card.interval >= 1,
            "graduated card should have a review interval"
        );
    }

    #[test]
    fn new_card_appears_in_new_queue() {
        let mut col = Collection::new();
        let note = NoteAdder::basic(&mut col).add(&mut col);

        let queued = SchedulerService::get_queued_cards(
            &mut col,
            GetQueuedCardsRequest {
                fetch_limit: 10,
                intraday_learning_only: false,
            },
        )
        .unwrap();

        assert_eq!(
            queued.new_count, 1,
            "the freshly added card should be counted as new"
        );
        assert_eq!(queued.learning_count, 0);
        assert_eq!(queued.review_count, 0);
        assert_eq!(queued.cards.len(), 1);
        // the queued card belongs to the note we just added
        let queued_card = queued.cards[0].card.as_ref().unwrap();
        let expected_cid = col.storage.card_ids_of_notes(&[note.id]).unwrap()[0];
        assert_eq!(queued_card.id, expected_cid.0);
    }

    #[test]
    fn unsuspend_brings_card_back_to_queue() {
        let mut col = Collection::new();
        let cid = add_basic_card(&mut col);

        // suspend first
        let _ = SchedulerService::suspend_cards(
            &mut col,
            anki_proto::scheduler::SuspendCardsRequest {
                card_ids: vec![cid.0],
                note_ids: vec![],
            },
        )
        .unwrap();
        assert_eq!(col.counts(), [0, 0, 0], "precondition: card is suspended");

        // restore it
        let _ = SchedulerService::unsuspend_cards(
            &mut col,
            anki_proto::cards::CardIds { cids: vec![cid.0] },
        )
        .unwrap();

        let card = col.storage.get_card(cid).unwrap().unwrap();
        assert_eq!(
            card.queue,
            CardQueue::New,
            "restored card returns to the new queue"
        );
        assert_eq!(col.counts(), [1, 0, 0], "card is queryable again");
    }

    #[test]
    fn suspend_marks_card_as_suspended_and_removes_from_queue() {
        let mut col = Collection::new();
        let cid = add_basic_card(&mut col);

        let out = SchedulerService::suspend_cards(
            &mut col,
            anki_proto::scheduler::SuspendCardsRequest {
                card_ids: vec![cid.0],
                note_ids: vec![],
            },
        )
        .unwrap();
        assert_eq!(out.count, 1);

        let card = col.storage.get_card(cid).unwrap().unwrap();
        assert_eq!(card.queue, CardQueue::Suspended);
        assert_eq!(col.counts(), [0, 0, 0], "suspended card leaves the queue");
    }

    #[test]
    fn suspend_via_note_ids_resolves_cards_from_notes() {
        let mut col = Collection::new();
        let note = NoteAdder::basic(&mut col).add(&mut col);
        let cid = col.storage.card_ids_of_notes(&[note.id]).unwrap()[0];

        // pass note_ids and leave card_ids empty to exercise the
        // card_ids_of_notes resolution branch in the service method
        let out = SchedulerService::suspend_cards(
            &mut col,
            anki_proto::scheduler::SuspendCardsRequest {
                card_ids: vec![],
                note_ids: vec![note.id.0],
            },
        )
        .unwrap();
        assert_eq!(out.count, 1, "the note's card should be suspended");

        let card = col.storage.get_card(cid).unwrap().unwrap();
        assert_eq!(card.queue, CardQueue::Suspended);
    }

    #[test]
    fn set_due_date_moves_card_to_review_with_given_offset() {
        let mut col = Collection::new();
        let cid = add_basic_card(&mut col);

        // schedule the (new) card to be due in 3 days
        let _ = SchedulerService::set_due_date(
            &mut col,
            anki_proto::scheduler::SetDueDateRequest {
                card_ids: vec![cid.0],
                days: "3".to_string(),
            },
        )
        .unwrap();

        let card = col.storage.get_card(cid).unwrap().unwrap();
        assert_eq!(
            card.ctype,
            CardType::Review,
            "set_due_date turns the card into a review card"
        );
        assert_eq!(card.queue, CardQueue::Review);
        // a new card has no review to measure an interval from
        assert_eq!(
            card.interval, 0,
            "FSRS keeps the interval of new cards at 0"
        );
        let today = col.timing_today().unwrap().days_elapsed as i32;
        assert_eq!(card.due, today + 3, "due should match the requested offset");
    }

    #[test]
    fn queued_cards_carry_one_label_per_button() {
        let mut col = Collection::new();
        add_basic_card(&mut col);

        let queued = SchedulerService::get_queued_cards(
            &mut col,
            GetQueuedCardsRequest {
                fetch_limit: 1,
                intraday_learning_only: false,
            },
        )
        .unwrap();

        let labels = &queued.cards[0].labels;
        assert_eq!(labels.len(), 4);
        assert!(labels.iter().all(|s| !s.is_empty()));
    }

    #[test]
    fn sched_timing_today_reports_no_elapsed_days_and_future_rollover() {
        let mut col = Collection::new();

        let timing = SchedulerService::sched_timing_today(&mut col).unwrap();

        // A collection created today has not rolled over to a new day yet.
        assert_eq!(timing.days_elapsed, 0);
        // The next day-rollover must be scheduled in the future.
        assert!(timing.next_day_at > TimestampSecs::now().0);
    }

    #[test]
    fn studied_today_reports_nothing_studied_for_fresh_collection() {
        let mut col = Collection::new();

        let msg = SchedulerService::studied_today(&mut col).unwrap().val;

        assert_eq!(
            msg.replace('\n', " "),
            "Studied 0 cards in 0 seconds today (0s/card)"
        );
    }

    #[test]
    fn studied_counts_track_new_cards_studied() {
        let mut col = Collection::new();
        NoteAdder::basic(&mut col).add(&mut col);

        // Nothing studied yet in the default deck.
        assert_eq!(studied_in_default_deck(&mut col), (0, 0));

        // Answering the new card counts it as one new card studied today.
        answer_top_card(&mut col, Rating::Good);

        let (new, _) = studied_in_default_deck(&mut col);
        assert_eq!(new, 1, "studying a new card increments the new count");
    }

    /// Adds a learning card that the default filtered-deck search will gather.
    fn add_gatherable_card(col: &mut Collection) -> Card {
        let note = NoteAdder::basic(col).add(col);
        let cid = col.storage.card_ids_of_notes(&[note.id]).unwrap()[0];
        let mut card = col.storage.get_card(cid).unwrap().unwrap();
        card.ctype = CardType::Learn;
        card.queue = CardQueue::DayLearn;
        card.remaining_steps = 1;
        card.due = 0;
        col.storage.update_card(&card).unwrap();
        card
    }

    #[test]
    fn rebuild_filtered_deck_gathers_matching_cards() {
        let mut col = Collection::new();
        let card = add_gatherable_card(&mut col);

        let mut filtered = Deck::new_filtered();
        col.add_or_update_deck(&mut filtered).unwrap();

        let out = SchedulerService::rebuild_filtered_deck(
            &mut col,
            anki_proto::decks::DeckId { did: filtered.id.0 },
        )
        .unwrap();

        assert_eq!(out.count, 1, "the matching card is gathered");
        let card = col.storage.get_card(card.id).unwrap().unwrap();
        assert_eq!(
            card.deck_id, filtered.id,
            "card now lives in the filtered deck"
        );
    }

    #[test]
    fn empty_filtered_deck_returns_cards_to_home_deck() {
        let mut col = Collection::new();
        let card = add_gatherable_card(&mut col);

        let mut filtered = Deck::new_filtered();
        col.add_or_update_deck(&mut filtered).unwrap();
        col.rebuild_filtered_deck(filtered.id).unwrap();
        // sanity: the card was pulled into the filtered deck
        assert_eq!(
            col.storage.get_card(card.id).unwrap().unwrap().deck_id,
            filtered.id
        );

        let _ = SchedulerService::empty_filtered_deck(
            &mut col,
            anki_proto::decks::DeckId { did: filtered.id.0 },
        )
        .unwrap();

        let card = col.storage.get_card(card.id).unwrap().unwrap();
        assert_eq!(card.deck_id, DeckId(1), "card is returned to its home deck");
    }
}
