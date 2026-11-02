// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

mod ownership;
mod schema11;
mod service;
pub(crate) mod undo;
mod update;

pub use anki_proto::deck_config::deck_config::config::AnswerAction;
pub use anki_proto::deck_config::deck_config::config::NewCardGatherPriority;
pub use anki_proto::deck_config::deck_config::config::NewCardInsertOrder;
pub use anki_proto::deck_config::deck_config::config::NewCardSortOrder;
pub use anki_proto::deck_config::deck_config::config::QuestionAction;
pub use anki_proto::deck_config::deck_config::config::ReviewCardOrder;
pub use anki_proto::deck_config::deck_config::config::ReviewMix;
pub use anki_proto::deck_config::deck_config::Config as DeckConfigInner;
pub use ownership::settings_repair_needed;
pub use schema11::DeckConfSchema11;
#[cfg(test)]
pub(crate) use update::default_param_search;
pub use update::UpdateDeckConfigsRequest;

/// The cards table stores 250% as 2500.
pub(crate) const INITIAL_EASE_FACTOR_THOUSANDS: u16 = 2500;

use crate::define_newtype;
use crate::prelude::*;

define_newtype!(DeckConfigId, i64);

#[derive(Debug, PartialEq, Clone)]
pub struct DeckConfig {
    pub id: DeckConfigId,
    pub name: String,
    pub mtime_secs: TimestampSecs,
    pub inner: DeckConfigInner,
}

/// NOTE: this does not set the default steps
const DEFAULT_DECK_CONFIG_INNER: DeckConfigInner = DeckConfigInner {
    learn_steps: Vec::new(),
    relearn_steps: Vec::new(),
    new_per_day: 20,
    reviews_per_day: 200,
    new_per_day_minimum: 0,
    maximum_review_interval: 36_500,
    new_card_insert_order: NewCardInsertOrder::Due as i32,
    new_card_gather_priority: NewCardGatherPriority::Deck as i32,
    new_card_sort_order: NewCardSortOrder::Template as i32,
    review_order: ReviewCardOrder::Day as i32,
    new_mix: ReviewMix::MixWithReviews as i32,
    interday_learning_mix: ReviewMix::MixWithReviews as i32,
    disable_autoplay: false,
    cap_answer_time_to_secs: 60,
    show_timer: false,
    stop_timer_on_answer: false,
    seconds_to_show_question: 0.0,
    seconds_to_show_answer: 0.0,
    question_action: QuestionAction::ShowAnswer as i32,
    answer_action: AnswerAction::ShowReminder as i32,
    wait_for_audio: true,
    skip_question_when_replaying_answer: false,
    fsrs_params_4: vec![],
    fsrs_params_5: vec![],
    fsrs_params_6: vec![],
    desired_retention: 0.9,
    other: Vec::new(),
    historical_retention: 0.9,
    param_search: String::new(),
    ignore_revlogs_before_date: String::new(),
    easy_days_percentages: Vec::new(),
};

impl Default for DeckConfig {
    fn default() -> Self {
        DeckConfig {
            id: DeckConfigId(0),
            name: "".to_string(),
            mtime_secs: Default::default(),
            inner: DeckConfigInner {
                learn_steps: vec![1.0, 10.0],
                relearn_steps: vec![10.0],
                easy_days_percentages: vec![1.0; 7],
                ..DEFAULT_DECK_CONFIG_INNER
            },
        }
    }
}

impl DeckConfig {
    /// The built-in values, as the settings of this deck.
    pub(crate) fn new_for_deck(deck: DeckId) -> Self {
        Self::with_inner_for_deck(deck, Self::default().inner)
    }

    pub(crate) fn with_inner_for_deck(deck: DeckId, inner: DeckConfigInner) -> Self {
        DeckConfig {
            id: DeckConfigId(deck.0),
            name: deck.to_string(),
            mtime_secs: Default::default(),
            inner,
        }
    }

    pub(crate) fn set_modified(&mut self) {
        self.mtime_secs = TimestampSecs::now();
    }

    /// Retrieve the FSRS 6.0 params, falling back on 5.0 or 4.x ones.
    pub fn fsrs_params(&self) -> &Vec<f32> {
        if !self.inner.fsrs_params_6.is_empty() {
            &self.inner.fsrs_params_6
        } else if !self.inner.fsrs_params_5.is_empty() {
            &self.inner.fsrs_params_5
        } else {
            &self.inner.fsrs_params_4
        }
    }

    /// Clear the FSRS 6.0 params, along with the 5.0 and 4.x fallbacks.
    pub(crate) fn clear_fsrs_params(&mut self) {
        self.inner.fsrs_params_4.clear();
        self.inner.fsrs_params_5.clear();
        self.inner.fsrs_params_6.clear();
    }
}

impl Collection {
    /// If fallback is true, guaranteed to return a deck config: the built-in
    /// values, if the row is missing.
    pub fn get_deck_config(
        &self,
        dcid: DeckConfigId,
        fallback: bool,
    ) -> Result<Option<DeckConfig>> {
        if let Some(conf) = self.storage.get_deck_config(dcid)? {
            return Ok(Some(conf));
        }
        Ok(fallback.then(|| DeckConfig {
            id: dcid,
            ..Default::default()
        }))
    }
}

impl Collection {
    /// Save the settings of a deck, which `config.id` names.
    pub(crate) fn add_or_update_deck_config(&mut self, config: &mut DeckConfig) -> Result<()> {
        match self.storage.get_deck_config(config.id)? {
            Some(original) => self.update_deck_config_inner(config, original, true),
            None => {
                config.set_modified();
                self.add_deck_config_if_unique_undoable(config)
            }
        }
    }

    /// Update an existing deck config, optionally bumping its modification
    /// time.
    pub(crate) fn update_deck_config_inner(
        &mut self,
        config: &mut DeckConfig,
        original: DeckConfig,
        update_mtime: bool,
    ) -> Result<()> {
        if config == &original {
            return Ok(());
        }
        if update_mtime {
            config.set_modified();
        }
        self.update_deck_config_undoable(config, original)
    }
}

/// There was a period of time when the deck options screen was allowing
/// 0/NaN to be persisted, so we need to check the values are within
/// valid bounds when reading from the DB.
pub(crate) fn ensure_deck_config_values_valid(config: &mut DeckConfigInner) {
    let default = DEFAULT_DECK_CONFIG_INNER;
    ensure_u32_valid(&mut config.new_per_day, default.new_per_day, 0, 9999);
    ensure_u32_valid(
        &mut config.reviews_per_day,
        default.reviews_per_day,
        0,
        9999,
    );
    ensure_u32_valid(
        &mut config.new_per_day_minimum,
        default.new_per_day_minimum,
        0,
        9999,
    );
    ensure_u32_valid(
        &mut config.maximum_review_interval,
        default.maximum_review_interval,
        1,
        36_500,
    );
    ensure_u32_valid(
        &mut config.cap_answer_time_to_secs,
        default.cap_answer_time_to_secs,
        1,
        9999,
    );
    ensure_f32_valid(
        &mut config.desired_retention,
        default.desired_retention,
        0.7,
        0.99,
    );
    ensure_f32_valid(
        &mut config.historical_retention,
        default.historical_retention,
        0.7,
        0.97,
    )
}

fn ensure_f32_valid(val: &mut f32, default: f32, min: f32, max: f32) {
    if val.is_nan() || *val < min || *val > max {
        *val = default;
    }
}

fn ensure_u32_valid(val: &mut u32, default: u32, min: u32, max: u32) {
    if *val < min || *val > max {
        *val = default;
    }
}

#[cfg(test)]
mod tests {
    use std::assert_matches;

    use super::*;

    #[test]
    fn get_deck_config_returns_none_when_flag_is_unset() -> Result<()> {
        let col = Collection::new();
        let config = col.get_deck_config(DeckConfigId(TimestampMillis::now().0), false)?;
        assert_eq!(config, None);

        Ok(())
    }

    #[test]
    fn a_missing_row_falls_back_on_built_in_values_rather_than_another_decks() -> Result<()> {
        let mut col = Collection::new();
        col.update_default_deck_config(|config| config.new_per_day = 77);
        let dcid = DeckConfigId(TimestampMillis::now().0);
        let config = col.get_deck_config(dcid, true)?.unwrap();
        assert_eq!(config.id, dcid);
        assert_eq!(
            config.inner.new_per_day,
            DeckConfig::default().inner.new_per_day
        );
        assert_matches!(col.get_deck_config(dcid, false)?, None);

        Ok(())
    }

    #[test]
    fn saving_uses_the_id_given_and_adds_a_missing_row() -> Result<()> {
        let mut col = Collection::new();
        let mut config = DeckConfig {
            id: DeckConfigId(1234),
            ..Default::default()
        };
        col.add_or_update_deck_config(&mut config)?;
        assert_eq!(config.id, DeckConfigId(1234));
        config.inner.new_per_day = 5;
        col.add_or_update_deck_config(&mut config)?;
        assert_eq!(col.get_deck_config(config.id, false)?, Some(config));

        Ok(())
    }

    #[test]
    fn update_deck_config_inner_updates_mtime() -> Result<()> {
        let mut col = Collection::new();
        let original = col.get_deck_config(DeckConfigId(1), false)?.unwrap();
        let mtime = original.mtime_secs;
        let mut config = original.clone();
        config.inner.new_per_day = 1;
        col.update_deck_config_inner(&mut config, original, true)?;
        let returned_config = col.get_deck_config(config.id, false)?;
        assert_ne!(config.mtime_secs, mtime);
        assert_eq!(returned_config, Some(config));

        Ok(())
    }

    #[test]
    fn update_deck_config_inner_leaves_mtime_if_identical() -> Result<()> {
        let mut col = Collection::new();
        let original = col.get_deck_config(DeckConfigId(1), false)?.unwrap();
        let mtime = original.mtime_secs;
        let mut config = original.clone();
        col.update_deck_config_inner(&mut config, original, true)?;
        let returned_config = col.get_deck_config(config.id, false)?;
        assert_eq!(config.mtime_secs, mtime);
        assert_eq!(returned_config, Some(config));

        Ok(())
    }

    /// Real settings can hold values a stock client saved for its old
    /// scheduler: of a real deck, 1.31, 0.5 and 1.91. They mustn't get in
    /// the way or do anything.
    fn stored_row_with_old_fields(col: &Collection, dcid: DeckConfigId, retention: f32) {
        use prost::Message;

        let mut inner = DeckConfig::default().inner;
        inner.historical_retention = retention;
        let mut blob = inner.encode_to_vec();
        for (field, value) in [(11u8, 1.31f32), (14, 0.5), (15, 1.91), (13, 0.5), (12, 2.0)] {
            // a float field's tag, with wire type 5
            blob.push((field << 3) | 5);
            blob.extend_from_slice(&value.to_le_bytes());
        }
        // varint fields: the minimum lapse interval and graduating intervals
        for (field, value) in [(17u8, 7u8), (18, 9), (19, 12)] {
            blob.extend_from_slice(&[0x80 | (field << 3), field >> 4, value]);
        }
        col.storage
            .db
            .execute(
                "update deck_config set config = ? where id = ?",
                rusqlite::params![blob, dcid],
            )
            .unwrap();
    }

    #[test]
    fn stored_old_scheduler_values_are_dropped_and_change_nothing() -> Result<()> {
        use crate::scheduler::states::CardState;
        use crate::scheduler::states::NormalState;

        let answer = |col: &mut Collection| -> Result<(u32, u32, u32)> {
            let nt = col.get_notetype_by_name("Basic")?.unwrap();
            let mut note = nt.new_note();
            col.add_note(&mut note, DeckId(1))?;
            let cid = col.search_cards(note.id, crate::search::SortMode::NoOrder)?[0];
            let mut card = col.storage.get_card(cid)?.unwrap();
            card.ctype = crate::card::CardType::Review;
            card.queue = crate::card::CardQueue::Review;
            card.interval = 40;
            card.ease_factor = 2500;
            card.due = 0;
            col.storage.update_card(&card)?;
            let states = col.get_scheduling_states(cid)?;
            let days = |state: CardState| match state {
                CardState::Normal(NormalState::Review(review)) => review.scheduled_days,
                other => panic!("expected review: {other:?}"),
            };
            Ok((days(states.hard), days(states.good), days(states.easy)))
        };
        let mut plain = Collection::new();
        let expected = answer(&mut plain)?;

        let mut col = Collection::new();
        stored_row_with_old_fields(&col, DeckConfigId(1), 0.9);
        let config = col.get_deck_config(DeckConfigId(1), false)?.unwrap();
        assert_eq!(config.inner, DeckConfig::default().inner);
        assert_eq!(answer(&mut col)?, expected);

        // saving writes the row without them
        let mut config = config;
        config.inner.new_per_day = 5;
        col.add_or_update_deck_config(&mut config)?;
        let blob: Vec<u8> =
            col.storage
                .db
                .query_row("select config from deck_config where id = 1", [], |row| {
                    row.get(0)
                })?;
        let mut expected_inner = DeckConfig::default().inner;
        expected_inner.new_per_day = 5;
        use prost::Message;
        assert_eq!(blob, expected_inner.encode_to_vec());

        Ok(())
    }

    /// FSRS takes a logarithm of this, so a 0 gives nonsense stability.
    #[test]
    fn missing_historical_retention_is_repaired_on_reading() -> Result<()> {
        let col = Collection::new();
        stored_row_with_old_fields(&col, DeckConfigId(1), 0.0);
        let unchecked = &col.storage.all_deck_config_unchecked()?[0];
        assert_eq!(unchecked.inner.historical_retention, 0.0);
        let config = col.get_deck_config(DeckConfigId(1), false)?.unwrap();
        assert_eq!(config.inner.historical_retention, 0.9);
        Ok(())
    }
}
