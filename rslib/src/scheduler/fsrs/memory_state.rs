// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

use std::collections::HashMap;

use anki_proto::scheduler::ComputeMemoryStateResponse;
use fsrs::FSRSItem;
use fsrs::MemoryState;
use fsrs::FSRS;
use fsrs::FSRS5_DEFAULT_DECAY;
use fsrs::FSRS6_DEFAULT_DECAY;
use itertools::Either;
use itertools::Itertools;
use tracing::error;

use super::params::ignore_revlogs_before_ms_from_config;
use super::rescheduler::Rescheduler;
use crate::card::CardQueue;
use crate::card::CardType;
use crate::prelude::*;
use crate::revlog::RevlogEntry;
use crate::scheduler::answering::get_fuzz_seed;
use crate::scheduler::fsrs::params::reviews_for_fsrs;
use crate::scheduler::fsrs::params::Params;
use crate::scheduler::states::fuzz::minimum_review_fuzz_interval;
use crate::scheduler::states::fuzz::with_review_fuzz;
use crate::search::Negated;
use crate::search::Node;
use crate::search::SearchNode;
use crate::search::StateKind;

#[derive(Debug, Clone, Copy, Default)]
pub struct ComputeMemoryProgress {
    pub current_cards: u32,
    pub total_cards: u32,
}

/// Helper function to determine the appropriate decay value based on FSRS
/// parameters
pub(crate) fn get_decay_from_params(params: &[f32]) -> f32 {
    if params.is_empty() {
        FSRS6_DEFAULT_DECAY // default decay for FSRS-6
    } else if params.len() < 21 {
        FSRS5_DEFAULT_DECAY // default decay for FSRS-4.5 and FSRS-5
    } else {
        params[20]
    }
}

#[derive(Debug)]
pub(crate) struct UpdateMemoryStateRequest {
    pub params: Params,
    pub desired_retention: f32,
    pub historical_retention: f32,
    pub max_interval: u32,
    pub reschedule: bool,
}

pub(crate) struct UpdateMemoryStateEntry {
    pub req: UpdateMemoryStateRequest,
    pub search: Node,
    pub ignore_before: TimestampMillis,
}

trait ChunkIntoVecs<T> {
    fn chunk_into_vecs(&mut self, chunk_size: usize) -> impl Iterator<Item = Vec<T>>;
}

impl<T> ChunkIntoVecs<T> for Vec<T> {
    fn chunk_into_vecs(&mut self, chunk_size: usize) -> impl Iterator<Item = Vec<T>> {
        std::iter::from_fn(move || {
            (!self.is_empty()).then(|| self.drain(..chunk_size.min(self.len())).collect())
        })
    }
}

impl Collection {
    /// For each provided set of params, locate cards with the provided search,
    /// and update their memory state.
    /// Should be called inside a transaction.
    pub(crate) fn update_memory_state(
        &mut self,
        entries: Vec<UpdateMemoryStateEntry>,
    ) -> Result<()> {
        let timing = self.timing_today()?;
        for UpdateMemoryStateEntry {
            req,
            search,
            ignore_before,
        } in entries
        {
            let search = SearchBuilder::all([search, SearchNode::State(StateKind::New).negated()]);
            let revlog = self.revlog_for_srs(search)?;

            let fsrs = FSRS::new(&req.params[..])?;
            let last_revlog_info = req.reschedule.then(|| get_last_revlog_info(&revlog));

            let items = fsrs_items_for_memory_states(
                &fsrs,
                revlog,
                timing.next_day_at,
                req.historical_retention,
                ignore_before,
            )?;

            let mut on_updated_card = self.create_progress_closure(items.len())?;

            let (items, cards_without_items): (Vec<(CardId, FsrsItemForMemoryState)>, Vec<CardId>) =
                items.into_iter().partition_map(|(card_id, item)| {
                    if let Some(item) = item {
                        Either::Left((card_id, item))
                    } else {
                        Either::Right(card_id)
                    }
                });

            let decay = get_decay_from_params(&req.params);

            // Store decay and desired retention in the card so that add-ons, card info,
            // stats and browser search/sorts don't need to access the deck config.
            // Unlike memory states, scheduler doesn't use decay and dr stored in the card.
            let set_decay_and_desired_retention = move |card: &mut Card| {
                card.desired_retention = Some(req.desired_retention);
                card.decay = Some(decay);
            };

            self.update_memory_state_for_itemless_cards(
                cards_without_items,
                set_decay_and_desired_retention,
                &mut on_updated_card,
            )?;

            let mut rescheduler =
                if req.reschedule && self.get_config_bool(BoolKey::LoadBalancerEnabled) {
                    Some(Rescheduler::new(self)?)
                } else {
                    None
                };

            let reschedule =
                move |card: &mut Card, collection: &mut Self, fsrs: &FSRS| -> Result<()> {
                    // we are rescheduling
                    let Some(last_revlog_info) = &last_revlog_info else {
                        return Ok(());
                    };

                    // we have a last review time for the card
                    let Some(last_info) = last_revlog_info.get(&card.id) else {
                        return Ok(());
                    };
                    let Some(last_review) = &last_info.last_reviewed_at else {
                        return Ok(());
                    };
                    // the card isn't in (re)learning or suspended
                    if !(card.ctype == CardType::Review && card.queue != CardQueue::Suspended) {
                        return Ok(());
                    };

                    let deck = collection
                        .get_deck(card.original_or_current_deck_id())?
                        .or_not_found(card.original_or_current_deck_id())?;
                    let deckconfig_id = deck.config_id().unwrap();
                    // reschedule it
                    let days_elapsed = timing.next_day_at.elapsed_days_since(*last_review) as i32;
                    let original_interval = card.interval;
                    let previous_interval = last_info.previous_interval.unwrap_or(0);
                    let interval = fsrs.next_interval(
                        Some(
                            card.memory_state
                                .expect("We set it before this function is called")
                                .stability,
                        ),
                        card.desired_retention
                            .expect("We set it before this function is called"),
                        0,
                    );
                    let min_interval =
                        minimum_review_fuzz_interval(interval, previous_interval, req.max_interval)
                            .max(1);
                    card.interval = rescheduler
                        .as_mut()
                        .and_then(|r| {
                            r.find_interval(
                                interval,
                                min_interval,
                                req.max_interval,
                                days_elapsed as u32,
                                deckconfig_id,
                                get_fuzz_seed(card, true),
                            )
                        })
                        .unwrap_or_else(|| {
                            with_review_fuzz(
                                card.get_fuzz_factor(true),
                                interval,
                                min_interval,
                                req.max_interval,
                            )
                        });
                    let due = if card.original_due != 0 {
                        &mut card.original_due
                    } else {
                        &mut card.due
                    };
                    let new_due =
                        (timing.days_elapsed as i32) - days_elapsed + card.interval as i32;
                    if let Some(rescheduler) = &mut rescheduler {
                        rescheduler.update_due_cnt_per_day(*due, new_due, deckconfig_id);
                    }
                    *due = new_due;
                    // Add a rescheduled revlog entry
                    collection.log_rescheduled_review(card, original_interval)?;

                    Ok(())
                };

            self.update_memory_state_for_cards_with_items(
                items,
                &fsrs,
                set_decay_and_desired_retention,
                reschedule,
                on_updated_card,
            )?;
        }
        Ok(())
    }

    fn create_progress_closure(&self, item_count: usize) -> Result<impl FnMut() -> Result<()>> {
        let mut progress = self.new_progress_handler::<ComputeMemoryProgress>();
        progress.update(false, |s| {
            s.total_cards = item_count as u32;
            s.current_cards = 1;
        })?;
        let on_updated_card = move || progress.update(true, |p| p.current_cards += 1);
        Ok(on_updated_card)
    }

    fn update_memory_state_for_itemless_cards(
        &mut self,
        cards: Vec<CardId>,
        mut set_decay_and_desired_retention: impl FnMut(&mut Card),
        mut on_updated_card: impl FnMut() -> Result<()>,
    ) -> Result<()> {
        for card_id in cards {
            let mut card = self.storage.get_card(card_id)?.or_not_found(card_id)?;
            let original = card.clone();
            set_decay_and_desired_retention(&mut card);
            card.memory_state = None;
            self.update_card_inner(&mut card, original)?;
            on_updated_card()?;
        }
        Ok(())
    }

    fn update_memory_state_for_cards_with_items(
        &mut self,
        items: Vec<(CardId, FsrsItemForMemoryState)>,
        fsrs: &FSRS,
        mut set_decay_and_desired_retention: impl FnMut(&mut Card),
        mut maybe_reschedule_card: impl FnMut(&mut Card, &mut Self, &FSRS) -> Result<()>,
        mut on_updated_card: impl FnMut() -> Result<()>,
    ) -> Result<()> {
        const FSRS_BATCH_SIZE: usize = 1000;

        let mut to_update = Vec::new();
        let mut fsrs_items = Vec::new();
        let mut starting_states = Vec::new();

        for (card_id, item) in items.into_iter() {
            to_update.push(card_id);
            fsrs_items.push(item.item);
            starting_states.push(item.starting_state);
        }

        // fsrs.memory_state_batch is O(nm) where n is the number of cards and m is the
        // max review count between all items. Therefore we want to pass batches
        // to fsrs.memory_state_batch where the review count is relatively even.
        let mut p = permutation::sort_unstable_by_key(&fsrs_items, |item| item.reviews.len());
        p.apply_slice_in_place(&mut to_update);
        p.apply_slice_in_place(&mut fsrs_items);
        p.apply_slice_in_place(&mut starting_states);

        for ((to_update, fsrs_items), starting_states) in to_update
            .chunk_into_vecs(FSRS_BATCH_SIZE)
            .zip_eq(fsrs_items.chunk_into_vecs(FSRS_BATCH_SIZE))
            .zip_eq(starting_states.chunk_into_vecs(FSRS_BATCH_SIZE))
        {
            let memory_states = fsrs.memory_state_batch(fsrs_items, starting_states)?;

            for (card_id, memory_state) in to_update.into_iter().zip_eq(memory_states) {
                let mut card = self.storage.get_card(card_id)?.or_not_found(card_id)?;
                let original = card.clone();
                set_decay_and_desired_retention(&mut card);
                card.memory_state = Some(memory_state.into());
                maybe_reschedule_card(&mut card, self, fsrs)?;
                self.update_card_inner(&mut card, original)?;
                on_updated_card()?;
            }
        }
        Ok(())
    }

    pub fn compute_memory_state(&mut self, card_id: CardId) -> Result<ComputeMemoryStateResponse> {
        let mut card = self.storage.get_card(card_id)?.or_not_found(card_id)?;
        let deck_id = card.original_deck_id.or(card.deck_id);
        let deck = self.get_deck(deck_id)?.or_not_found(card.deck_id)?;
        let conf_id = deck.config_id().or_invalid("home deck is filtered")?;
        let config = self
            .storage
            .get_deck_config(conf_id)?
            .or_not_found(conf_id)?;
        let desired_retention = config.inner.desired_retention;

        let historical_retention = config.inner.historical_retention;
        let params = config.fsrs_params();
        let decay = get_decay_from_params(params);
        let fsrs = FSRS::new(params)?;
        let revlog = self.revlog_for_srs(SearchNode::CardIds(card.id.to_string()))?;
        let item = fsrs_item_for_memory_state(
            &fsrs,
            revlog,
            self.timing_today()?.next_day_at,
            historical_retention,
            ignore_revlogs_before_ms_from_config(&config)?,
        )?;
        if item.is_some() {
            card.set_memory_state(&fsrs, item, historical_retention)?;
            Ok(ComputeMemoryStateResponse {
                state: card.memory_state.map(Into::into),
                desired_retention,
                decay,
            })
        } else {
            Ok(ComputeMemoryStateResponse {
                state: None,
                desired_retention,
                decay,
            })
        }
    }

    // Used for extra-ordinary circumstances where a memory state is needed but is
    // not availiable, e.g. the card has been moved to a different deck.
    // Try to use update_memory_state where you can.
    pub fn compute_and_update_memory_state(&mut self, card: &mut Card) -> Result<()> {
        let fsrs_data = self.compute_memory_state(card.id)?;
        card.memory_state = fsrs_data.state.map(Into::into);
        card.desired_retention = Some(fsrs_data.desired_retention);
        card.decay = Some(fsrs_data.decay);
        self.storage.update_card(card)?;
        Ok(())
    }
}

/// In a stock client's file this key turned FSRS on or off. Here it's only true
/// once a collection has been given memory states, and a file can still bring
/// back false, so the name has to stay.
const MEMORY_STATES_ADDED_KEY: &str = "fsrs";

impl Collection {
    /// Collections that come from a stock client may have been using its old
    /// scheduler, and need memory states for stats, search and retrievability
    /// sorting. To be called when a real user collection is opened, not on
    /// the temporary ones used for import. A failure is logged rather than
    /// preventing the collection from opening: the transaction is rolled
    /// back, cards still get their state when answered, and the next open
    /// tries again.
    pub(crate) fn migrate_to_fsrs_if_needed(&mut self) {
        if let Err(err) = self.migrate_to_fsrs() {
            error!(?err, "unable to compute FSRS memory states");
        }
    }

    /// [MEMORY_STATES_ADDED_KEY] marks whether this is needed. It is set to
    /// true afterwards, so this runs again only if an import brings back
    /// false, along with any cards scheduled by a stock client.
    fn migrate_to_fsrs(&mut self) -> Result<()> {
        if self.get_config_optional::<bool, _>(MEMORY_STATES_ADDED_KEY) == Some(true) {
            return Ok(());
        }
        self.transact_no_undo(|col| {
            col.add_missing_memory_states()?;
            col.set_config(MEMORY_STATES_ADDED_KEY, &true)?;
            Ok(())
        })
    }

    /// For a collection already marked, yet may have cards with no memory
    /// state, such as after importing scheduling from a stock client.
    /// Clears the undo queue, so check [Self::count_missing_memory_states]
    /// first.
    pub fn add_missing_fsrs_memory_states(&mut self) -> Result<()> {
        self.transact_no_undo(|col| col.add_missing_memory_states())
    }

    /// How many reviewed cards have no memory state.
    pub fn count_missing_memory_states(&self) -> Result<u32> {
        Ok(self.storage.db().query_row(
            "select count() from cards where type != 0 and ivl > 0 and data not like '%\"s\":%'",
            [],
            |row| row.get(0),
        )?)
    }

    /// Only cards without a memory state are written to, and only their FSRS
    /// fields: due dates, intervals, ease factors and revlogs are left alone.
    fn add_missing_memory_states(&mut self) -> Result<()> {
        let mut decks = vec![];
        for deck in self.storage.get_all_decks()? {
            if let Some(config_id) = deck.config_id() {
                let config = self
                    .get_deck_config(config_id, true)?
                    .or_not_found(config_id)?;
                decks.push((config, deck.id));
            }
        }

        let without_state = |deck: DeckId| {
            SearchBuilder::all([
                SearchNode::DeckIdsWithoutChildren(deck.to_string()).into(),
                SearchNode::HasMemoryState.negated(),
            ])
            .try_into_search()
        };

        let entries = decks
            .iter()
            .map(|(config, deck)| {
                Ok(UpdateMemoryStateEntry {
                    req: UpdateMemoryStateRequest {
                        params: config.fsrs_params().clone(),
                        desired_retention: config.inner.desired_retention,
                        historical_retention: config.inner.historical_retention,
                        max_interval: config.inner.maximum_review_interval,
                        reschedule: false,
                    },
                    search: without_state(*deck)?,
                    ignore_before: ignore_revlogs_before_ms_from_config(config)?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        self.update_memory_state(entries)?;

        // Cards with no usable revlog are skipped above, so infer their state
        // from the card, as is done when answering them.
        for (config, deck) in &decks {
            let fsrs = FSRS::new(config.fsrs_params())?;
            let search = SearchBuilder::all([
                without_state(*deck)?,
                SearchNode::State(StateKind::New).negated(),
            ]);
            for mut card in self.all_cards_for_search(search)? {
                let original = card.clone();
                if let Err(err) =
                    card.set_memory_state(&fsrs, None, config.inner.historical_retention)
                {
                    error!(?err, card = card.id.0, "unable to infer memory state");
                    continue;
                }
                if card.memory_state.is_none() {
                    continue;
                }
                card.decay = Some(get_decay_from_params(config.fsrs_params()));
                card.desired_retention = Some(config.inner.desired_retention);
                self.update_card_inner(&mut card, original)?;
            }
        }
        Ok(())
    }
}

impl Card {
    pub(crate) fn set_memory_state(
        &mut self,
        fsrs: &FSRS,
        item: Option<FsrsItemForMemoryState>,
        historical_retention: f32,
    ) -> Result<()> {
        let memory_state = if let Some(i) = item {
            Some(fsrs.memory_state(i.item, i.starting_state)?)
        } else if self.ctype == CardType::New || self.interval == 0 {
            None
        } else {
            // no valid revlog entries; infer state from current card state
            Some(fsrs.memory_state_from_sm2(
                self.ease_factor(),
                self.interval as f32,
                historical_retention,
            )?)
        };
        self.memory_state = memory_state.map(Into::into);
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub(crate) struct FsrsItemForMemoryState {
    pub item: FSRSItem,
    /// When revlogs have been truncated, this stores the initial state at first
    /// review
    pub starting_state: Option<MemoryState>,
}

/// Like [fsrs_item_for_memory_state], but for updating multiple cards at once.
pub(crate) fn fsrs_items_for_memory_states(
    fsrs: &FSRS,
    revlogs: Vec<RevlogEntry>,
    next_day_at: TimestampSecs,
    historical_retention: f32,
    ignore_revlogs_before: TimestampMillis,
) -> Result<Vec<(CardId, Option<FsrsItemForMemoryState>)>> {
    revlogs
        .into_iter()
        .chunk_by(|r| r.cid)
        .into_iter()
        .map(|(card_id, group)| {
            Ok((
                card_id,
                fsrs_item_for_memory_state(
                    fsrs,
                    group.collect(),
                    next_day_at,
                    historical_retention,
                    ignore_revlogs_before,
                )?,
            ))
        })
        .collect()
}

pub(crate) struct LastRevlogInfo {
    /// Used to determine the actual elapsed time between the last time the user
    /// reviewed the card and now, so that we can determine an accurate period
    /// when the card has subsequently been rescheduled to a different day.
    pub(crate) last_reviewed_at: Option<TimestampSecs>,
    /// The interval before the latest review. Used to prevent fuzz from going
    /// backwards when rescheduling the card
    pub(crate) previous_interval: Option<u32>,
}

/// Return a map of cards to info about last review.
pub(crate) fn get_last_revlog_info(revlogs: &[RevlogEntry]) -> HashMap<CardId, LastRevlogInfo> {
    let mut out = HashMap::new();
    revlogs
        .iter()
        .chunk_by(|r| r.cid)
        .into_iter()
        .for_each(|(card_id, group)| {
            let mut last_reviewed_at = None;
            let mut previous_interval = None;
            for e in group.into_iter() {
                if e.has_rating_and_affects_scheduling() {
                    last_reviewed_at = Some(e.id.as_secs());
                    previous_interval = if e.last_interval >= 0 && e.button_chosen > 1 {
                        Some(e.last_interval as u32)
                    } else {
                        None
                    };
                } else if e.is_reset() {
                    last_reviewed_at = None;
                    previous_interval = None;
                }
            }
            out.insert(
                card_id,
                LastRevlogInfo {
                    last_reviewed_at,
                    previous_interval,
                },
            );
        });
    out
}

/// When calculating memory state, only the last FSRSItem is required. If the
/// revlog is non-empty and no learning steps have been detected (indicative of
/// a truncated revlog), we return the starting state inferred from the first
/// revlog entry, so that the first review is not treated as if started from
/// scratch.
pub(crate) fn fsrs_item_for_memory_state(
    fsrs: &FSRS,
    entries: Vec<RevlogEntry>,
    next_day_at: TimestampSecs,
    historical_retention: f32,
    ignore_revlogs_before: TimestampMillis,
) -> Result<Option<FsrsItemForMemoryState>> {
    struct FirstReview {
        interval: f32,
        ease_factor: f32,
    }
    if let Some(mut output) = reviews_for_fsrs(entries, next_day_at, false, ignore_revlogs_before) {
        let mut item = output.fsrs_items.pop().unwrap().1;
        if output.revlogs_complete {
            Ok(Some(FsrsItemForMemoryState {
                item,
                starting_state: None,
            }))
        } else if let Some(first_user_grade) = output.filtered_revlogs.first() {
            // the revlog has been truncated, but not fully
            let first_review = FirstReview {
                interval: first_user_grade.interval.max(1) as f32,
                ease_factor: if first_user_grade.ease_factor == 0 {
                    2500
                } else {
                    first_user_grade.ease_factor
                } as f32
                    / 1000.0,
            };
            let mut starting_state = fsrs.memory_state_from_sm2(
                first_review.ease_factor,
                first_review.interval,
                historical_retention,
            )?;
            // if the ease factor is less than 1.1, the revlog entry is generated by FSRS
            if first_review.ease_factor <= 1.1 {
                starting_state.difficulty = (first_review.ease_factor - 0.1) * 9.0 + 1.0;
            }
            // remove the first review because it has been converted to the starting state
            item.reviews.remove(0);
            Ok(Some(FsrsItemForMemoryState {
                item,
                starting_state: Some(starting_state),
            }))
        } else {
            // only manual and rescheduled revlogs; treat like empty
            Ok(None)
        }
    } else {
        // no revlogs (new card or caused by ignore_revlogs_before or deleted revlogs)
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use fsrs::MemoryState;

    use super::*;
    use crate::card::FsrsMemoryState;
    use crate::revlog::RevlogReviewKind;
    use crate::scheduler::fsrs::params::tests::convert;
    use crate::scheduler::fsrs::params::tests::revlog;

    /// Floating point precision can vary between platforms, and each FSRS
    /// update tends to result in small changes to these numbers, so we
    /// round them.
    fn assert_int_eq(actual: Option<FsrsMemoryState>, expected: Option<FsrsMemoryState>) {
        let actual = actual.unwrap();
        let expected = expected.unwrap();
        assert_eq!(actual.stability.round(), expected.stability.round());
        assert_eq!(actual.difficulty.round(), expected.difficulty.round());
    }

    #[test]
    fn bypassed_learning_is_handled() -> Result<()> {
        // cards without any learning steps due to truncated history still have memory
        // state calculated
        let fsrs = FSRS::new(&[]).unwrap();
        let item = fsrs_item_for_memory_state(
            &fsrs,
            vec![
                RevlogEntry {
                    ease_factor: 2500,
                    interval: 100,
                    ..revlog(RevlogReviewKind::Review, 99)
                },
                revlog(RevlogReviewKind::Review, 0),
            ],
            TimestampSecs::now(),
            0.9,
            0.into(),
        )?
        .unwrap();
        assert_int_eq(
            item.starting_state.map(Into::into),
            Some(FsrsMemoryState {
                stability: 100.0,
                difficulty: 5.003576,
            }),
        );
        let mut card = Card {
            reps: 1,
            ..Default::default()
        };
        card.set_memory_state(&fsrs, Some(item), 0.9)?;
        assert_int_eq(
            card.memory_state,
            Some(FsrsMemoryState {
                stability: 248.9251,
                difficulty: 4.9938006,
            }),
        );
        // cards with a single review-type entry also get memory states from revlog
        // rather than card states
        let item = fsrs_item_for_memory_state(
            &fsrs,
            vec![RevlogEntry {
                ease_factor: 2500,
                interval: 100,
                ..revlog(RevlogReviewKind::Review, 100)
            }],
            TimestampSecs::now(),
            0.9,
            0.into(),
        )?
        .unwrap();
        assert!(item.item.reviews.is_empty());
        card.set_memory_state(&fsrs, Some(item), 0.9)?;
        assert_int_eq(
            card.memory_state,
            Some(FsrsMemoryState {
                stability: 100.0,
                difficulty: 5.003576,
            }),
        );
        Ok(())
    }

    #[test]
    fn zero_history_is_handled() -> Result<()> {
        // when the history is empty, no items are produced
        assert_eq!(convert(&[], false), None);
        // but memory state should still be inferred, by using the card's current state
        let mut card = Card {
            ctype: CardType::Review,
            interval: 100,
            ease_factor: 1300,
            reps: 1,
            ..Default::default()
        };
        card.set_memory_state(&FSRS::new(&[]).unwrap(), None, 0.9)?;
        assert_int_eq(
            card.memory_state,
            Some(
                MemoryState {
                    stability: 99.999954,
                    difficulty: 9.979899,
                }
                .into(),
            ),
        );
        Ok(())
    }

    mod always_on {

        use serde_json::Value;

        use super::*;
        use crate::tests::NoteAdder;

        /// As importing a whole collection does: replaces all config, with the
        /// provided value for FSRS.
        fn receive_fsrs_config_from_import(col: &mut Collection, value: bool) {
            let mut config = col.storage.get_all_config().unwrap();
            config.insert("fsrs".to_string(), Value::Bool(value));
            col.storage
                .set_all_config(config, TimestampSecs::now())
                .unwrap();
        }

        fn stored_flag(col: &Collection) -> Option<bool> {
            col.get_config_optional(MEMORY_STATES_ADDED_KEY)
        }

        /// A review card with a real revlog and no memory state, as left behind
        /// by a stock client's old scheduler.
        fn add_review_card_without_memory_state(col: &mut Collection) -> CardId {
            let note = NoteAdder::basic(col).add(col);
            let card_id = col.storage.card_ids_of_notes(&[note.id]).unwrap()[0];
            col.answer_easy();
            let mut card = col.storage.get_card(card_id).unwrap().unwrap();
            card.clear_fsrs_data();
            col.storage.update_card(&card).unwrap();
            card_id
        }

        #[test]
        fn new_collection_is_fsrs_on() {
            let mut col = Collection::new();
            assert_eq!(stored_flag(&col), None);
            NoteAdder::basic(&mut col).add(&mut col);
            col.answer_easy();
            assert!(col.get_first_card().memory_state.is_some());
        }

        #[test]
        fn imported_false_config_does_not_turn_fsrs_off() {
            let mut col = Collection::new();
            NoteAdder::basic(&mut col).add(&mut col);
            receive_fsrs_config_from_import(&mut col, false);

            assert_eq!(stored_flag(&col), Some(false));

            // answering schedules with FSRS
            col.answer_easy();
            assert!(col.get_first_card().memory_state.is_some());
        }

        #[test]
        fn migration_adds_memory_states_without_rescheduling() -> Result<()> {
            let mut col = Collection::new();
            let card_id = add_review_card_without_memory_state(&mut col);
            receive_fsrs_config_from_import(&mut col, false);
            let before = col.storage.get_card(card_id)?.unwrap();
            let revlog_before = col.storage.get_all_revlog_entries(TimestampSecs(0))?;
            assert!(before.memory_state.is_none());

            col.migrate_to_fsrs_if_needed();

            let after = col.storage.get_card(card_id)?.unwrap();
            assert!(after.memory_state.is_some());
            assert!(after.desired_retention.is_some());
            assert!(after.decay.is_some());
            assert_eq!(after.due, before.due);
            assert_eq!(after.interval, before.interval);
            assert_eq!(after.ease_factor, before.ease_factor);
            assert_eq!(after.queue, before.queue);
            assert_eq!(after.ctype, before.ctype);
            assert_eq!(after.reps, before.reps);
            assert_eq!(after.lapses, before.lapses);
            assert_eq!(
                col.storage.get_all_revlog_entries(TimestampSecs(0))?,
                revlog_before
            );
            assert_eq!(stored_flag(&col), Some(true));
            Ok(())
        }

        #[test]
        fn migration_infers_state_for_review_card_without_revlog() -> Result<()> {
            let mut col = Collection::new();
            let card_id = add_review_card_without_memory_state(&mut col);
            col.storage.db.execute_batch("delete from revlog")?;
            let before = col.storage.get_card(card_id)?.unwrap();
            let expected = FSRS::new(&[])?.memory_state_from_sm2(
                before.ease_factor(),
                before.interval as f32,
                0.9,
            )?;

            col.migrate_to_fsrs_if_needed();

            let after = col.storage.get_card(card_id)?.unwrap();
            let state = after.memory_state.unwrap();
            assert!((state.stability - expected.stability).abs() < 0.01);
            assert!((state.difficulty - expected.difficulty).abs() < 0.01);
            assert_eq!(after.due, before.due);
            assert_eq!(after.interval, before.interval);
            Ok(())
        }

        #[test]
        fn migration_uses_the_desired_retention_of_the_deck() -> Result<()> {
            let mut col = Collection::new();
            let card_id = add_review_card_without_memory_state(&mut col);
            let mut config = col.get_deck_config(DeckConfigId(1), false)?.unwrap();
            config.inner.desired_retention = 0.85;
            col.add_or_update_deck_config(&mut config)?;

            col.migrate_to_fsrs_if_needed();

            let card = col.storage.get_card(card_id)?.unwrap();
            assert_eq!(card.desired_retention, Some(0.85));
            Ok(())
        }

        #[test]
        fn migration_leaves_new_cards_and_existing_states_alone() -> Result<()> {
            let mut col = Collection::new();
            let existing = NoteAdder::basic(&mut col).add(&mut col);
            let existing = col.storage.card_ids_of_notes(&[existing.id])?[0];
            col.answer_easy();
            let new = NoteAdder::basic(&mut col).add(&mut col);
            let new = col.storage.card_ids_of_notes(&[new.id])?[0];
            let existing_before = col.storage.get_card(existing)?.unwrap();
            let new_before = col.storage.get_card(new)?.unwrap();
            assert!(existing_before.memory_state.is_some());

            col.migrate_to_fsrs_if_needed();

            assert_eq!(col.storage.get_card(existing)?.unwrap(), existing_before);
            assert_eq!(col.storage.get_card(new)?.unwrap(), new_before);
            Ok(())
        }

        #[test]
        fn migration_only_runs_once_until_import_stores_false_again() -> Result<()> {
            let mut col = Collection::new();
            let card_id = add_review_card_without_memory_state(&mut col);
            col.migrate_to_fsrs_if_needed();
            assert!(col
                .storage
                .get_card(card_id)?
                .unwrap()
                .memory_state
                .is_some());

            // with the stored flag true, a card without a state is not looked at
            let mut card = col.storage.get_card(card_id)?.unwrap();
            card.clear_fsrs_data();
            col.storage.update_card(&card)?;
            col.migrate_to_fsrs_if_needed();
            assert!(col
                .storage
                .get_card(card_id)?
                .unwrap()
                .memory_state
                .is_none());

            // an imported false means a stock client may have been used
            receive_fsrs_config_from_import(&mut col, false);
            col.migrate_to_fsrs_if_needed();
            assert!(col
                .storage
                .get_card(card_id)?
                .unwrap()
                .memory_state
                .is_some());
            assert_eq!(stored_flag(&col), Some(true));
            Ok(())
        }
    }
}
