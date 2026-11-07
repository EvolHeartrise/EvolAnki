// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

//! Updating configs in bulk, from the deck options screen.

use anki_proto::deck_config::deck_configs_for_update::current_deck::Limits;
use anki_proto::deck_config::deck_configs_for_update::CurrentDeck;
use anki_proto::deck_config::UpdateDeckConfigsMode;
use anki_proto::decks::deck::normal::DayLimit;
use fsrs::DEFAULT_PARAMETERS;
use fsrs::FSRS;
use tracing::warn;

use crate::config::I32ConfigKey;
use crate::decks::NormalDeck;
use crate::prelude::*;
use crate::scheduler::fsrs::memory_state::UpdateMemoryStateEntry;
use crate::scheduler::fsrs::memory_state::UpdateMemoryStateRequest;
use crate::scheduler::fsrs::params::ignore_revlogs_before_ms_from_config;
use crate::scheduler::fsrs::params::ComputeParamsRequest;
use crate::search::JoinSearches;
use crate::search::Negated;
use crate::search::Node;
use crate::search::SearchNode;
use crate::search::StateKind;

#[derive(Debug, Clone)]
pub struct UpdateDeckConfigsRequest {
    pub target_deck_id: DeckId,
    /// The settings of the target deck. Its id is ignored.
    pub config: DeckConfig,
    pub mode: UpdateDeckConfigsMode,
    pub limits: Limits,
    pub new_cards_ignore_review_limit: bool,
    pub apply_all_parent_limits: bool,
    pub fsrs_reschedule: bool,
    pub fsrs_health_check: bool,
}

impl Collection {
    /// Information required for the deck options screen.
    pub fn get_deck_configs_for_update(
        &mut self,
        deck: DeckId,
    ) -> Result<anki_proto::deck_config::DeckConfigsForUpdate> {
        let mut defaults = DeckConfig::default();
        defaults.inner.fsrs_params_6 = DEFAULT_PARAMETERS.into();
        let last_optimize = self.get_config_i32(I32ConfigKey::LastFsrsOptimize) as u32;
        let days_since_last_fsrs_optimize = if last_optimize > 0 {
            self.timing_today()?
                .days_elapsed
                .saturating_sub(last_optimize)
        } else {
            0
        };
        Ok(anki_proto::deck_config::DeckConfigsForUpdate {
            current_deck: Some(self.get_current_deck_for_update(deck)?),
            config: Some(self.get_deck_settings_for_update(deck)?.into()),
            defaults: Some(defaults.into()),
            new_cards_ignore_review_limit: self.get_config_bool(BoolKey::NewCardsIgnoreReviewLimit),
            apply_all_parent_limits: self.get_config_bool(BoolKey::ApplyAllParentLimits),
            fsrs_health_check: self.get_config_bool(BoolKey::FsrsHealthCheck),
            days_since_last_fsrs_optimize,
        })
    }

    /// Information required for the deck options screen.
    pub fn update_deck_configs(&mut self, input: UpdateDeckConfigsRequest) -> Result<OpOutput<()>> {
        self.transact(Op::UpdateDeckConfig, |col| {
            col.update_deck_configs_inner(input)
        })
    }
}

/// What a deck's FSRS tools look at, unless it has its own search: the deck and
/// its subdecks, as `deck:` means, without suspended cards.
pub(crate) fn default_param_search(deck: &Deck) -> Result<String> {
    Ok(SearchNode::from_deck_name(&deck.human_name())
        .and(SearchNode::State(StateKind::Suspended).negated())
        .try_into_search()?
        .to_string())
}

impl Collection {
    fn get_deck_settings_for_update(&self, deck: DeckId) -> Result<DeckConfig> {
        let mut config = self
            .get_deck_config(DeckConfigId(deck.0), true)?
            .or_not_found(deck)?;
        // pre-fill empty fsrs params with older params
        if config.inner.fsrs_params_6.is_empty() {
            config.inner.fsrs_params_6 = if config.inner.fsrs_params_5.is_empty() {
                config.inner.fsrs_params_4.clone()
            } else {
                config.inner.fsrs_params_5.clone()
            };
        }
        Ok(config)
    }

    fn get_current_deck_for_update(&mut self, deck: DeckId) -> Result<CurrentDeck> {
        let deck = self.get_deck(deck)?.or_not_found(deck)?;
        let normal = deck.normal()?;
        let today = self.timing_today()?.days_elapsed;

        Ok(CurrentDeck {
            name: deck.human_name(),
            limits: Some(normal_deck_to_limits(normal, today)),
            default_search: default_param_search(&deck)?,
        })
    }

    fn update_deck_configs_inner(&mut self, mut req: UpdateDeckConfigsRequest) -> Result<()> {
        let target = self
            .storage
            .get_deck(req.target_deck_id)?
            .or_not_found(req.target_deck_id)?;
        let target_config_id = target
            .config_id()
            .or_invalid("a filtered deck has no settings")?;
        // What the page says it edited makes no difference to where this goes.
        req.config.id = target_config_id;
        req.config.name = target.id.to_string();

        let configs_before_update = self.storage.get_deck_config_map()?;
        let mut configs_after_update = configs_before_update.clone();
        let today = self
            .timing_today()?
            .next_day_at
            .adding_secs(-24 * 60 * 60)
            .date_string();

        let mut configs = if req.mode == UpdateDeckConfigsMode::ComputeAllParams {
            self.compute_all_params(&req)?
        } else {
            vec![req.config.clone()]
        };

        for conf in &mut configs {
            if conf.inner.ignore_revlogs_before_date > today {
                today.clone_into(&mut conf.inner.ignore_revlogs_before_date);
            }

            // If the user has provided empty FSRS6 params, zero out any
            // old params as well, so we don't fall back on them, which would
            // be surprising as they're not shown in the GUI.
            if conf.inner.fsrs_params_6.is_empty() {
                conf.inner.fsrs_params_5.clear();
                conf.inner.fsrs_params_4.clear();
            }
            // check the provided parameters are valid before we save them
            FSRS::new(conf.fsrs_params())?;
            self.add_or_update_deck_config(conf)?;
            configs_after_update.insert(conf.id, conf.clone());
        }
        let today = self.timing_today()?.days_elapsed;
        let mut updated_target = target.clone();
        update_deck_limits(updated_target.normal_mut()?, &req.limits, today);
        self.update_deck_inner(&mut updated_target, target)?;

        // anything that follows from settings changing, in any deck that changed
        let mut decks_needing_memory_recompute = vec![];
        for deck in self.storage.get_all_decks()? {
            let Some(config_id) = deck.config_id() else {
                continue;
            };
            let previous = configs_before_update.get(&config_id);
            let current = configs_after_update.get(&config_id);

            // if new order differs, deck needs re-sorting
            let current_order = current
                .map(|c| c.inner.new_card_insert_order())
                .unwrap_or_default();
            let previous_order = previous
                .map(|c| c.inner.new_card_insert_order())
                .unwrap_or_default();
            if previous_order != current_order {
                self.sort_deck(deck.id, current_order)?;
            }

            // if params differ, memory state needs to be recomputed
            let params_of = |c: Option<&DeckConfig>| c.map(|c| c.fsrs_params().clone());
            let retention_of = |c: Option<&DeckConfig>| c.map(|c| c.inner.desired_retention);
            let easy_days_of =
                |c: Option<&DeckConfig>| c.map(|c| c.inner.easy_days_percentages.clone());
            if params_of(previous) != params_of(current)
                || retention_of(previous) != retention_of(current)
                || (req.fsrs_reschedule && easy_days_of(previous) != easy_days_of(current))
            {
                decks_needing_memory_recompute.push((deck.id, config_id));
            }
            self.adjust_remaining_steps_in_deck(deck.id, previous, current)?;
        }

        if !decks_needing_memory_recompute.is_empty() {
            let input: Vec<UpdateMemoryStateEntry> = decks_needing_memory_recompute
                .into_iter()
                .filter_map(|(deck_id, config_id)| {
                    configs_after_update
                        .get(&config_id)
                        .map(|config| (deck_id, config))
                })
                .map(|(deck_id, c)| {
                    Ok(UpdateMemoryStateEntry {
                        req: UpdateMemoryStateRequest {
                            params: c.fsrs_params().clone(),
                            desired_retention: c.inner.desired_retention,
                            max_interval: c.inner.maximum_review_interval,
                            reschedule: req.fsrs_reschedule,
                            historical_retention: c.inner.historical_retention,
                        },
                        search: Node::Search(SearchNode::DeckIdsWithoutChildren(
                            deck_id.to_string(),
                        )),
                        ignore_before: ignore_revlogs_before_ms_from_config(c)?,
                    })
                })
                .collect::<Result<_>>()?;
            self.update_memory_state(input)?;
        }

        self.set_config_bool_inner(
            BoolKey::NewCardsIgnoreReviewLimit,
            req.new_cards_ignore_review_limit,
        )?;
        self.set_config_bool_inner(BoolKey::ApplyAllParentLimits, req.apply_all_parent_limits)?;
        self.set_config_bool_inner(BoolKey::FsrsHealthCheck, req.fsrs_health_check)?;

        Ok(())
    }

    /// Adjust the remaining steps of cards in the given deck according to the
    /// config change.
    pub(crate) fn adjust_remaining_steps_in_deck(
        &mut self,
        deck: DeckId,
        previous_config: Option<&DeckConfig>,
        current_config: Option<&DeckConfig>,
    ) -> Result<()> {
        if let (Some(old), Some(new)) = (previous_config, current_config) {
            for (search, old_steps, new_steps) in [
                (
                    SearchBuilder::learning_cards(),
                    &old.inner.learn_steps,
                    &new.inner.learn_steps,
                ),
                (
                    SearchBuilder::relearning_cards(),
                    &old.inner.relearn_steps,
                    &new.inner.relearn_steps,
                ),
            ] {
                if old_steps == new_steps {
                    continue;
                }
                let search = search.clone().and(SearchNode::from_deck_id(deck, false));
                for mut card in self.all_cards_for_search(search)? {
                    self.adjust_remaining_steps(&mut card, old_steps, new_steps)?;
                }
            }
        }
        Ok(())
    }

    /// The settings of all normal decks, with the parameters optimized where
    /// that's possible, each from its own deck. A deck that can't be optimized,
    /// as it has too few reviews or fails, is returned with what it had.
    fn compute_all_params(&mut self, req: &UpdateDeckConfigsRequest) -> Result<Vec<DeckConfig>> {
        let mut decks: Vec<_> = self
            .storage
            .get_all_decks()?
            .into_iter()
            .filter(Deck::is_normal)
            .collect();
        decks.sort_unstable_by_key(|deck| deck.id == req.target_deck_id);

        let total_decks = decks.len() as u32;
        let mut configs = vec![];
        for (idx, deck) in decks.iter().enumerate() {
            let mut config = if deck.id == req.target_deck_id {
                req.config.clone()
            } else {
                self.get_deck_settings_for_update(deck.id)?
            };
            let search = if config.inner.param_search.trim().is_empty() {
                default_param_search(deck)?
            } else {
                config.inner.param_search.clone()
            };
            let current_params = config.fsrs_params().clone();
            match self.compute_params(ComputeParamsRequest {
                search: &search,
                ignore_revlogs_before_ms: ignore_revlogs_before_ms_from_config(&config)?,
                current_deck: idx as u32 + 1,
                total_decks,
                current_params: &current_params,
                num_of_relearning_steps: config.inner.relearn_steps.len(),
                health_check: false,
            }) {
                Ok(params) if params.params != current_params => {
                    config.inner.fsrs_params_6 = params.params;
                }
                Ok(_) => (),
                Err(AnkiError::Interrupted) => return Err(AnkiError::Interrupted),
                Err(err) => warn!(deck = deck.id.0, ?err, "unable to optimize"),
            }
            configs.push(config);
        }
        let today = self.timing_today()?.days_elapsed as i32;
        self.set_config_i32_inner(I32ConfigKey::LastFsrsOptimize, today)?;
        Ok(configs)
    }
}

fn normal_deck_to_limits(deck: &NormalDeck, today: u32) -> Limits {
    Limits {
        review_today: deck.review_limit_today.map(|limit| limit.limit),
        new_today: deck.new_limit_today.map(|limit| limit.limit),
        review_today_active: deck
            .review_limit_today
            .map(|limit| limit.today == today)
            .unwrap_or_default(),
        new_today_active: deck
            .new_limit_today
            .map(|limit| limit.today == today)
            .unwrap_or_default(),
    }
}

fn update_deck_limits(deck: &mut NormalDeck, limits: &Limits, today: u32) {
    update_day_limit(&mut deck.review_limit_today, limits.review_today, today);
    update_day_limit(&mut deck.new_limit_today, limits.new_today, today);
}

fn update_day_limit(day_limit: &mut Option<DayLimit>, new_limit: Option<u32>, today: u32) {
    if let Some(limit) = new_limit {
        day_limit.replace(DayLimit { limit, today });
    } else {
        // if the collection was created today, the
        // "preserve last value" hack below won't work
        // clear "future" limits as well (from imports)
        day_limit.take_if(|limit| limit.today == 0 || limit.today > today);
        if let Some(limit) = day_limit {
            // instead of setting to None, only make sure today is in the past,
            // thus preserving last used value
            limit.today = limit.today.min(today.saturating_sub(1));
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::card::CardQueue;
    use crate::deckconfig::NewCardInsertOrder;
    use crate::search::SortMode;
    use crate::tests::open_test_collection_with_learning_card;
    use crate::tests::open_test_collection_with_relearning_card;
    use crate::tests::CardAdder;
    use crate::tests::DeckAdder;
    use crate::timestamp::TimestampSecs;

    fn request_for(col: &mut Collection, deck: DeckId) -> UpdateDeckConfigsRequest {
        let output = col.get_deck_configs_for_update(deck).unwrap();
        UpdateDeckConfigsRequest {
            target_deck_id: deck,
            config: output.config.unwrap().into(),
            mode: UpdateDeckConfigsMode::Normal,
            limits: Limits::default(),
            new_cards_ignore_review_limit: false,
            apply_all_parent_limits: false,
            fsrs_reschedule: false,
            fsrs_health_check: true,
        }
    }

    #[test]
    fn updating() -> Result<()> {
        let mut col = Collection::new();
        let nt = col.get_notetype_by_name("Basic")?.unwrap();
        let mut note1 = nt.new_note();
        col.add_note(&mut note1, DeckId(1))?;
        let card1_id = col.storage.card_ids_of_notes(&[note1.id])?[0];
        for _ in 0..9 {
            let mut note = nt.new_note();
            col.add_note(&mut note, DeckId(1))?;
        }

        // add the keys so it doesn't trigger a change below
        col.set_config_bool_inner(BoolKey::NewCardsIgnoreReviewLimit, false)?;
        col.set_config_bool_inner(BoolKey::ApplyAllParentLimits, false)?;
        col.set_config_bool_inner(BoolKey::FsrsHealthCheck, true)?;

        let reset_card1_pos = |col: &mut Collection| {
            let mut card = col.storage.get_card(card1_id).unwrap().unwrap();
            // set it out of bounds, so we can be sure it has changed
            card.due = 0;
            col.storage.update_card(&card).unwrap();
        };
        let card1_pos = |col: &mut Collection| col.storage.get_card(card1_id).unwrap().unwrap().due;

        // if nothing changed, no changes should be made
        let mut input = request_for(&mut col, DeckId(1));
        // the page's prefilled values are saved if they differ, so settle them first
        col.update_deck_configs(input.clone())?;
        input = request_for(&mut col, DeckId(1));
        assert!(!col.update_deck_configs(input.clone())?.changes.had_change());

        // modifying a value should update the config, but not the deck
        input.config.inner.new_per_day += 1;
        let changes = col.update_deck_configs(input.clone())?.changes.changes;
        assert!(!changes.deck);
        assert!(changes.deck_config);
        assert!(!changes.card);

        // changing the order will cause the cards to be re-sorted
        assert_eq!(card1_pos(&mut col), 1);
        reset_card1_pos(&mut col);
        assert_eq!(card1_pos(&mut col), 0);
        input.config.inner.new_card_insert_order = NewCardInsertOrder::Random as i32;
        assert!(col.update_deck_configs(input.clone())?.changes.changes.card);
        assert_ne!(card1_pos(&mut col), 0);

        Ok(())
    }

    #[test]
    fn saving_keeps_memory_states_and_recomputes_with_fsrs() -> Result<()> {
        let mut col = Collection::new();
        let nt = col.get_notetype_by_name("Basic")?.unwrap();
        let mut note = nt.new_note();
        col.add_note(&mut note, DeckId(1))?;
        col.answer_easy();
        let before = col.get_first_card();
        assert!(before.memory_state.is_some());

        let mut input = request_for(&mut col, DeckId(1));

        // an unchanged save leaves it all alone
        col.update_deck_configs(input.clone())?;
        let card = col.get_first_card();
        assert_eq!(card.memory_state, before.memory_state);
        assert_eq!(card.desired_retention, before.desired_retention);
        assert_eq!(card.decay, before.decay);

        // a change that triggers a recompute must keep FSRS data too
        input.config.inner.desired_retention = 0.8;
        col.update_deck_configs(input)?;
        let card = col.get_first_card();
        assert!(card.memory_state.is_some());
        assert_eq!(card.desired_retention, Some(0.8));
        assert_eq!(card.due, before.due);
        assert_eq!(card.interval, before.interval);
        Ok(())
    }

    #[test]
    fn a_save_changes_only_the_target_deck_whatever_id_it_carries() -> Result<()> {
        let mut col = Collection::new();
        let first = DeckAdder::new("first").add(&mut col);
        let second = DeckAdder::new("second").add(&mut col);
        let first_before = col.get_deck_config(first.config_id().unwrap(), false)?;
        let default_before = col.get_deck_config(DeckConfigId(1), false)?;

        let mut input = request_for(&mut col, second.id);
        input.config.inner.new_per_day = 3;
        // a stale page, naming another row
        input.config.id = first.config_id().unwrap();
        col.update_deck_configs(input)?;

        assert_eq!(
            col.get_deck_config(first.config_id().unwrap(), false)?,
            first_before
        );
        assert_eq!(col.get_deck_config(DeckConfigId(1), false)?, default_before);
        let second_after = col
            .get_deck_config(second.config_id().unwrap(), false)?
            .unwrap();
        assert_eq!(second_after.inner.new_per_day, 3);
        Ok(())
    }

    #[test]
    fn undoing_a_save_brings_back_the_settings() -> Result<()> {
        let mut col = Collection::new();
        let deck = DeckAdder::new("deck").add(&mut col);
        let before = col.get_deck_config(deck.config_id().unwrap(), false)?;
        let mut input = request_for(&mut col, deck.id);
        input.config.inner.new_per_day = 3;
        col.update_deck_configs(input)?;
        col.undo()?;
        assert_eq!(
            col.get_deck_config(deck.config_id().unwrap(), false)?,
            before
        );
        Ok(())
    }

    #[test]
    fn a_filtered_deck_cant_be_saved_to() {
        let mut col = Collection::new();
        let deck = DeckAdder::new("filtered").filtered(true).add(&mut col);
        let mut input = request_for(&mut col, DeckId(1));
        input.target_deck_id = deck.id;
        assert!(col.update_deck_configs(input).is_err());
    }

    #[test]
    fn today_only_limits_are_kept_and_the_regular_limit_is_the_settings() -> Result<()> {
        let mut col = Collection::new();
        let today = col.timing_today()?.days_elapsed;
        let deck = DeckAdder::new("deck").add(&mut col);
        let mut input = request_for(&mut col, deck.id);
        input.config.inner.new_per_day = 3;
        input.limits.new_today = Some(9);
        col.update_deck_configs(input.clone())?;

        let saved = col.storage.get_deck(deck.id)?.unwrap();
        assert_eq!(
            saved.normal()?.new_limit_today,
            Some(DayLimit { limit: 9, today })
        );
        assert_eq!(saved.normal()?.new_limit, None);
        let shown = col.get_deck_configs_for_update(deck.id)?;
        let limits = shown.current_deck.unwrap().limits.unwrap();
        assert!(limits.new_today_active);
        assert_eq!(limits.new_today, Some(9));
        assert_eq!(shown.config.unwrap().config.unwrap().new_per_day, 3);
        Ok(())
    }

    #[test]
    fn default_search_is_the_deck_with_its_subdecks() -> Result<()> {
        let mut col = Collection::new();
        for name in ["Kaishi 1.5k", "A::B", "odd_na*me"] {
            let deck = DeckAdder::new(name).add(&mut col);
            let search = default_param_search(&deck)?;
            assert!(search.contains("deck:"), "{search}");
            assert!(search.ends_with("-is:suspended)"), "{search}");
            // and it parses back to what it was
            col.search_cards(search.as_str(), SortMode::NoOrder)?;
        }

        let parent = DeckAdder::new("P").add(&mut col);
        let child = DeckAdder::new("P::C").add(&mut col);
        let other = DeckAdder::new("PP").add(&mut col);
        let in_parent = CardAdder::new().deck(parent.id).add(&mut col).remove(0);
        let in_child = CardAdder::new().deck(child.id).add(&mut col).remove(0);
        let in_other = CardAdder::new().deck(other.id).add(&mut col).remove(0);
        let mut suspended = CardAdder::new().deck(parent.id).add(&mut col).remove(0);
        suspended.queue = CardQueue::Suspended;
        col.storage.update_card(&suspended)?;

        let found = col.search_cards(default_param_search(&parent)?.as_str(), SortMode::NoOrder)?;
        assert!(found.contains(&in_parent.id));
        assert!(found.contains(&in_child.id));
        assert!(!found.contains(&in_other.id));
        assert!(!found.contains(&suspended.id));
        Ok(())
    }

    #[test]
    fn should_increase_remaining_learning_steps_if_unpassed_learning_step_added() {
        let mut col = open_test_collection_with_learning_card();
        col.set_default_learn_steps(vec![1., 10., 100.]);
        assert_eq!(col.get_first_card().remaining_steps, 3);
    }

    #[test]
    fn should_keep_remaining_learning_steps_if_unpassed_relearning_step_added() {
        let mut col = open_test_collection_with_learning_card();
        col.set_default_relearn_steps(vec![1., 10., 100.]);
        assert_eq!(col.get_first_card().remaining_steps, 2);
    }

    #[test]
    fn should_keep_remaining_learning_steps_if_passed_learning_step_added() {
        let mut col = open_test_collection_with_learning_card();
        col.answer_good();
        col.set_default_learn_steps(vec![1., 1., 10.]);
        assert_eq!(col.get_first_card().remaining_steps, 1);
    }

    #[test]
    fn should_keep_at_least_one_remaining_learning_step() {
        let mut col = open_test_collection_with_learning_card();
        col.answer_good();
        col.set_default_learn_steps(vec![1.]);
        assert_eq!(col.get_first_card().remaining_steps, 1);
    }

    #[test]
    fn should_increase_remaining_relearning_steps_if_unpassed_relearning_step_added() {
        let mut col = open_test_collection_with_relearning_card();
        col.set_default_relearn_steps(vec![1., 10., 100.]);
        assert_eq!(col.get_first_card().remaining_steps, 3);
    }

    #[test]
    fn should_keep_remaining_relearning_steps_if_unpassed_learning_step_added() {
        let mut col = open_test_collection_with_relearning_card();
        col.set_default_learn_steps(vec![1., 10., 100.]);
        assert_eq!(col.get_first_card().remaining_steps, 1);
    }

    #[test]
    fn should_keep_remaining_relearning_steps_if_passed_relearning_step_added() {
        let mut col = open_test_collection_with_relearning_card();
        col.set_default_relearn_steps(vec![10., 100.]);
        col.answer_good();
        col.set_default_relearn_steps(vec![1., 10., 100.]);
        assert_eq!(col.get_first_card().remaining_steps, 1);
    }

    #[test]
    fn should_keep_at_least_one_remaining_relearning_step() {
        let mut col = open_test_collection_with_relearning_card();
        col.set_default_relearn_steps(vec![10., 100.]);
        col.answer_good();
        col.set_default_relearn_steps(vec![1.]);
        assert_eq!(col.get_first_card().remaining_steps, 1);
    }

    #[test]
    fn should_clamp_ignore_revlogs_before_date_to_today() {
        let mut col = Collection::new();
        let today = col
            .timing_today()
            .unwrap()
            .next_day_at
            .adding_secs(-24 * 60 * 60)
            .date_string();
        let base_input = request_for(&mut col, DeckId(1));
        let mut saved_date = |date: String| {
            let mut input = base_input.clone();
            input.config.inner.ignore_revlogs_before_date = date;
            assert!(col.update_deck_configs(input).is_ok());
            let updated = col.get_deck_configs_for_update(DeckId(1)).unwrap();
            let config: DeckConfig = updated.config.unwrap().into();
            config.inner.ignore_revlogs_before_date
        };

        // future date should be clamped to today
        let future = TimestampSecs::now().adding_secs(86_400).date_string();
        assert_eq!(saved_date(future), today);
        // past dates should be left unchanged
        let past = TimestampSecs::now().adding_secs(-86_400).date_string();
        assert_eq!(saved_date(past.clone()), past);
        // today's date should also be left unchanged
        assert_eq!(saved_date(today.clone()), today);
        // empty dates should be saved as is and handled by
        // ignore_revlogs_before_date_to_ms
        assert_eq!(saved_date("".to_string()), "");
    }
}
