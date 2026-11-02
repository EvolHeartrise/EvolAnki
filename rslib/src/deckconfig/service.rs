// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html
use std::collections::HashMap;

use rayon::iter::IntoParallelIterator;
use rayon::iter::ParallelIterator;

use crate::collection::Collection;
use crate::deckconfig::DeckConfig;
use crate::deckconfig::DeckConfigId;
use crate::deckconfig::UpdateDeckConfigsRequest;
use crate::error::Result;
use crate::scheduler::fsrs::params::ignore_revlogs_before_date_to_ms;
use crate::scheduler::fsrs::simulator::is_included_card;

impl crate::services::DeckConfigService for Collection {
    fn get_deck_config(
        &mut self,
        input: anki_proto::deck_config::DeckConfigId,
    ) -> Result<anki_proto::deck_config::DeckConfig> {
        Ok(Collection::get_deck_config(self, input.into(), true)?
            .unwrap()
            .into())
    }

    fn get_deck_configs_for_update(
        &mut self,
        input: anki_proto::decks::DeckId,
    ) -> Result<anki_proto::deck_config::DeckConfigsForUpdate> {
        self.get_deck_configs_for_update(input.did.into())
    }

    fn update_deck_configs(
        &mut self,
        input: anki_proto::deck_config::UpdateDeckConfigsRequest,
    ) -> Result<anki_proto::collection::OpChanges> {
        self.update_deck_configs(input.into()).map(Into::into)
    }

    fn get_ignored_before_count(
        &mut self,
        input: anki_proto::deck_config::GetIgnoredBeforeCountRequest,
    ) -> Result<anki_proto::deck_config::GetIgnoredBeforeCountResponse> {
        let timestamp = ignore_revlogs_before_date_to_ms(&input.ignore_revlogs_before_date)?;
        let guard = self.search_cards_into_table(
            &format!("{} -is:new", input.search),
            crate::search::SortMode::NoOrder,
        )?;

        Ok(anki_proto::deck_config::GetIgnoredBeforeCountResponse {
            included: guard
                .col
                .storage
                .get_card_count_with_ignore_before(timestamp)?,
            total: guard.cards.try_into().unwrap_or(0),
        })
    }

    fn get_retention_workload(
        &mut self,
        input: anki_proto::deck_config::GetRetentionWorkloadRequest,
    ) -> Result<anki_proto::deck_config::GetRetentionWorkloadResponse> {
        let days_elapsed = self.timing_today().unwrap().days_elapsed as i32;
        let guard =
            self.search_cards_into_table(&input.search, crate::search::SortMode::NoOrder)?;

        let revlogs = guard
            .col
            .storage
            .get_revlog_entries_for_searched_cards_in_card_order()?;

        let mut config = guard.col.get_optimal_retention_parameters(revlogs)?;
        let fsrs_card_params = std::sync::Arc::new(fsrs::check_and_fill_parameters(&input.w)?);
        let cards = guard
            .col
            .storage
            .all_searched_cards()?
            .into_iter()
            .filter(is_included_card)
            .filter_map(|c| {
                let desired_retention = c.desired_retention.unwrap_or(0.9);
                crate::card::Card::convert(
                    c.clone(),
                    days_elapsed,
                    c.memory_state?,
                    desired_retention,
                    fsrs_card_params.clone(),
                )
            })
            .collect::<Vec<fsrs::Card>>();

        config.deck_size = guard.cards;

        let costs = (70u32..=99u32)
            .into_par_iter()
            .map(|dr| {
                Ok((
                    dr,
                    fsrs::expected_workload_with_existing_cards(
                        &input.w,
                        dr as f32 / 100.,
                        &config,
                        &cards,
                    )?,
                ))
            })
            .collect::<Result<HashMap<_, _>>>()?;

        Ok(anki_proto::deck_config::GetRetentionWorkloadResponse { costs })
    }
}

impl From<DeckConfig> for anki_proto::deck_config::DeckConfig {
    fn from(c: DeckConfig) -> Self {
        anki_proto::deck_config::DeckConfig {
            id: c.id.0,
            name: c.name,
            mtime_secs: c.mtime_secs.0,
            config: Some(c.inner),
        }
    }
}

impl From<anki_proto::deck_config::UpdateDeckConfigsRequest> for UpdateDeckConfigsRequest {
    fn from(c: anki_proto::deck_config::UpdateDeckConfigsRequest) -> Self {
        let mode = c.mode();
        UpdateDeckConfigsRequest {
            target_deck_id: c.target_deck_id.into(),
            config: c.config.map(Into::into).unwrap_or_default(),
            mode,
            card_state_customizer: c.card_state_customizer,
            limits: c.limits.unwrap_or_default(),
            new_cards_ignore_review_limit: c.new_cards_ignore_review_limit,
            apply_all_parent_limits: c.apply_all_parent_limits,
            fsrs: c.fsrs,
            fsrs_reschedule: c.fsrs_reschedule,
            fsrs_health_check: c.fsrs_health_check,
        }
    }
}

impl From<anki_proto::deck_config::DeckConfig> for DeckConfig {
    fn from(c: anki_proto::deck_config::DeckConfig) -> Self {
        DeckConfig {
            id: c.id.into(),
            name: c.name,
            mtime_secs: c.mtime_secs.into(),
            inner: c.config.unwrap_or_default(),
        }
    }
}

impl From<anki_proto::deck_config::DeckConfigId> for DeckConfigId {
    fn from(dcid: anki_proto::deck_config::DeckConfigId) -> Self {
        DeckConfigId(dcid.dcid)
    }
}

#[cfg(test)]
mod tests {
    use std::assert_matches;

    use fsrs::DEFAULT_PARAMETERS;

    use super::*;
    use crate::card::CardQueue;
    use crate::card::CardType;
    use crate::card::FsrsMemoryState;
    use crate::prelude::*;
    use crate::revlog::RevlogEntry;
    use crate::revlog::RevlogReviewKind;
    use crate::services::DeckConfigService;
    use crate::tests::CardAdder;
    use crate::tests::DeckAdder;

    #[test]
    fn get_deck_config_returns_config() -> Result<()> {
        let mut col = Collection::new();
        let deck = DeckAdder::new("custom")
            .with_config(|config| config.inner.new_per_day = 3)
            .add(&mut col);
        let returned = DeckConfigService::get_deck_config(
            &mut col,
            anki_proto::deck_config::DeckConfigId { dcid: deck.id.0 },
        )?;

        assert_eq!(returned.id, deck.id.0);
        assert_eq!(returned.config.unwrap().new_per_day, 3);

        Ok(())
    }

    #[test]
    fn get_deck_configs_for_update_returns_requested_deck() -> Result<()> {
        let mut col = Collection::new();
        let deck = DeckAdder::new("child")
            .with_config(|config| config.inner.new_per_day = 3)
            .add(&mut col);

        let output = DeckConfigService::get_deck_configs_for_update(
            &mut col,
            anki_proto::decks::DeckId { did: deck.id.0 },
        )?;

        let current = output.current_deck.unwrap();
        assert_eq!(current.name, "child");
        assert_eq!(current.default_search, "(deck:child -is:suspended)");
        let config = output.config.unwrap();
        assert_eq!(config.id, deck.id.0);
        assert_eq!(config.config.unwrap().new_per_day, 3);

        Ok(())
    }

    #[test]
    fn a_filtered_deck_has_no_options_to_show() {
        let mut col = Collection::new();
        let deck = DeckAdder::new("filtered").filtered(true).add(&mut col);
        assert!(col.get_deck_configs_for_update(deck.id).is_err());
    }

    fn update_request_for_deck(
        col: &mut Collection,
        deck_id: DeckId,
    ) -> Result<anki_proto::deck_config::UpdateDeckConfigsRequest> {
        let output = col.get_deck_configs_for_update(deck_id)?;
        Ok(anki_proto::deck_config::UpdateDeckConfigsRequest {
            target_deck_id: deck_id.0,
            config: output.config,
            ..Default::default()
        })
    }

    #[test]
    fn update_deck_configs_saves_config_from_request() -> Result<()> {
        let mut col = Collection::new();
        let mut request = update_request_for_deck(&mut col, DeckId(1))?;
        request
            .config
            .as_mut()
            .unwrap()
            .config
            .as_mut()
            .unwrap()
            .new_per_day = 7;

        let changes = DeckConfigService::update_deck_configs(&mut col, request)?;

        assert!(changes.deck_config);
        assert_eq!(
            col.get_deck_config(DeckConfigId(1), false)?
                .unwrap()
                .inner
                .new_per_day,
            7
        );

        Ok(())
    }

    const IGNORE_BEFORE_DATE: &str = "2024-01-01";

    fn add_reviewed_card(col: &mut Collection, revlogs: &[(RevlogReviewKind, i64)]) -> Card {
        let mut card = CardAdder::new().add(col).remove(0);
        card.ctype = CardType::Review;
        card.queue = CardQueue::Review;
        col.storage.update_card(&card).unwrap();
        for &(review_kind, id) in revlogs {
            let entry = RevlogEntry {
                id: RevlogId(id),
                cid: card.id,
                review_kind,
                ..Default::default()
            };
            col.storage.add_revlog_entry(&entry, true).unwrap();
        }
        card
    }

    fn get_ignored_before_count(
        col: &mut Collection,
        date: &str,
    ) -> Result<anki_proto::deck_config::GetIgnoredBeforeCountResponse> {
        DeckConfigService::get_ignored_before_count(
            col,
            anki_proto::deck_config::GetIgnoredBeforeCountRequest {
                ignore_revlogs_before_date: date.into(),
                search: "deck:Default".into(),
            },
        )
    }

    #[test]
    fn get_ignored_before_count_counts_cards_learned_after_date() -> Result<()> {
        let mut col = Collection::new();
        let cutoff = ignore_revlogs_before_date_to_ms(&IGNORE_BEFORE_DATE.to_string())?.0;
        let day_ms = 86_400_000;
        CardAdder::new().add(&mut col);
        add_reviewed_card(&mut col, &[(RevlogReviewKind::Learning, cutoff + day_ms)]);
        add_reviewed_card(&mut col, &[(RevlogReviewKind::Learning, cutoff - day_ms)]);
        add_reviewed_card(&mut col, &[(RevlogReviewKind::Review, cutoff + day_ms)]);

        let response = get_ignored_before_count(&mut col, IGNORE_BEFORE_DATE)?;

        assert_eq!(response.included, 1);
        assert_eq!(response.total, 3, "new cards are not counted");

        Ok(())
    }

    #[test]
    fn get_ignored_before_count_includes_all_learned_cards_when_date_is_empty() -> Result<()> {
        let mut col = Collection::new();
        add_reviewed_card(&mut col, &[(RevlogReviewKind::Learning, 1)]);
        add_reviewed_card(&mut col, &[(RevlogReviewKind::Learning, 2)]);

        let response = get_ignored_before_count(&mut col, "")?;

        assert_eq!(response.included, 2);
        assert_eq!(response.total, 2);

        Ok(())
    }

    #[test]
    fn get_ignored_before_count_rejects_invalid_date() {
        let mut col = Collection::new();

        let result = get_ignored_before_count(&mut col, "not a date");

        assert_matches!(result, Err(AnkiError::InvalidInput { .. }));
    }

    fn get_retention_workload(
        col: &mut Collection,
        w: Vec<f32>,
        search: &str,
    ) -> Result<anki_proto::deck_config::GetRetentionWorkloadResponse> {
        DeckConfigService::get_retention_workload(
            col,
            anki_proto::deck_config::GetRetentionWorkloadRequest {
                w,
                search: search.into(),
            },
        )
    }

    #[test]
    fn get_retention_workload_returns_cost_for_each_retention_from_70_to_99() -> Result<()> {
        let mut col = Collection::new();

        let response = get_retention_workload(&mut col, DEFAULT_PARAMETERS.to_vec(), "")?;

        let mut retentions: Vec<u32> = response.costs.into_keys().collect();
        retentions.sort_unstable();
        assert_eq!(retentions, (70..=99).collect::<Vec<_>>());

        Ok(())
    }

    #[test]
    fn get_retention_workload_includes_existing_cards_matching_search() -> Result<()> {
        let mut col = Collection::new();
        let mut card = add_reviewed_card(&mut col, &[(RevlogReviewKind::Learning, 1)]);
        card.interval = 10;

        card.memory_state = None;
        col.storage.update_card(&card)?;
        let without_memory_state =
            get_retention_workload(&mut col, DEFAULT_PARAMETERS.to_vec(), "")?;

        card.memory_state = Some(FsrsMemoryState {
            stability: 10.0,
            difficulty: 5.0,
        });
        col.storage.update_card(&card)?;
        let with_memory_state = get_retention_workload(&mut col, DEFAULT_PARAMETERS.to_vec(), "")?;

        assert_ne!(without_memory_state.costs, with_memory_state.costs);

        Ok(())
    }

    #[test]
    fn get_retention_workload_rejects_invalid_params() {
        let mut col = Collection::new();

        let result = get_retention_workload(&mut col, vec![1.0; 5], "");

        assert_matches!(result, Err(AnkiError::FsrsParamsInvalid));
    }
}
