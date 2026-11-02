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

/// Old deck config and cards table store 250% as 2500.
pub(crate) const INITIAL_EASE_FACTOR_THOUSANDS: u16 = (INITIAL_EASE_FACTOR * 1000.0) as u16;

use crate::define_newtype;
use crate::prelude::*;
use crate::scheduler::states::review::INITIAL_EASE_FACTOR;

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
    initial_ease: 2.5,
    easy_multiplier: 1.3,
    hard_multiplier: 1.2,
    lapse_multiplier: 0.0,
    interval_multiplier: 1.0,
    maximum_review_interval: 36_500,
    minimum_lapse_interval: 1,
    graduating_interval_good: 1,
    graduating_interval_easy: 4,
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
    ensure_f32_valid(&mut config.initial_ease, default.initial_ease, 1.31, 5.0);
    ensure_f32_valid(
        &mut config.easy_multiplier,
        default.easy_multiplier,
        1.0,
        5.0,
    );
    ensure_f32_valid(
        &mut config.hard_multiplier,
        default.hard_multiplier,
        0.5,
        1.3,
    );
    ensure_f32_valid(
        &mut config.lapse_multiplier,
        default.lapse_multiplier,
        0.0,
        1.0,
    );
    ensure_f32_valid(
        &mut config.interval_multiplier,
        default.interval_multiplier,
        0.5,
        2.0,
    );
    ensure_u32_valid(
        &mut config.maximum_review_interval,
        default.maximum_review_interval,
        1,
        36_500,
    );
    ensure_u32_valid(
        &mut config.minimum_lapse_interval,
        default.minimum_lapse_interval,
        1,
        36_500,
    );
    ensure_u32_valid(
        &mut config.graduating_interval_good,
        default.graduating_interval_good,
        1,
        36_500,
    );
    ensure_u32_valid(
        &mut config.graduating_interval_easy,
        default.graduating_interval_easy,
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
}
