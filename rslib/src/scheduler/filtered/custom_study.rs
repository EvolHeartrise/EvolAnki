// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html


use anki_proto::scheduler::custom_study_request::cram::CramKind;
use anki_proto::scheduler::custom_study_request::Cram;
use anki_proto::scheduler::custom_study_request::Value as CustomStudyValue;

use super::FilteredDeckForUpdate;
use crate::config::DeckConfigKey;
use crate::decks::FilteredDeck;
use crate::decks::FilteredSearchOrder;
use crate::decks::FilteredSearchTerm;
use crate::error::CustomStudyError;
use crate::error::FilteredDeckError;
use crate::prelude::*;
use crate::search::JoinSearches;
use crate::search::Negated;
use crate::search::PropertyKind;
use crate::search::RatingKind;
use crate::search::SearchNode;
use crate::search::StateKind;

impl Collection {
    pub fn custom_study(
        &mut self,
        input: anki_proto::scheduler::CustomStudyRequest,
    ) -> Result<OpOutput<()>> {
        self.transact(Op::CreateCustomStudy, |col| col.custom_study_inner(input))
    }

}

impl Collection {
    fn custom_study_inner(
        &mut self,
        input: anki_proto::scheduler::CustomStudyRequest,
    ) -> Result<()> {
        let mut deck = self
            .storage
            .get_deck(input.deck_id.into())?
            .or_not_found(input.deck_id)?;

        match input.value.or_invalid("missing oneof value")? {
            CustomStudyValue::NewLimitDelta(delta) => {
                let today = self.current_due_day(0)?;
                self.extend_limits(today, self.usn()?, deck.id, delta, 0)?;
                if delta > 0 {
                    deck = self.storage.get_deck(deck.id)?.or_not_found(deck.id)?;
                    let original = deck.clone();
                    deck.normal_mut()?.extend_new = delta as u32;
                    self.update_deck_inner(&mut deck, original, self.usn()?)?;
                }
                Ok(())
            }
            CustomStudyValue::ReviewLimitDelta(delta) => {
                let today = self.current_due_day(0)?;
                self.extend_limits(today, self.usn()?, deck.id, 0, delta)?;
                if delta > 0 {
                    deck = self.storage.get_deck(deck.id)?.or_not_found(deck.id)?;
                    let original = deck.clone();
                    deck.normal_mut()?.extend_review = delta as u32;
                    self.update_deck_inner(&mut deck, original, self.usn()?)?;
                }
                Ok(())
            }
            CustomStudyValue::ForgotDays(days) => {
                self.create_custom_study_deck(forgot_config(deck.human_name(), days))
            }
            CustomStudyValue::ReviewAheadDays(days) => {
                self.create_custom_study_deck(ahead_config(deck.human_name(), days))
            }
            CustomStudyValue::PreviewDays(days) => {
                self.create_custom_study_deck(preview_config(deck.human_name(), days))
            }
            CustomStudyValue::Cram(cram) => {
                self.create_custom_study_deck(cram_config(deck.human_name(), &cram)?)?;
                self.set_config(
                    DeckConfigKey::CustomStudyIncludeTags
                        .for_deck(deck.id)
                        .as_str(),
                    &cram.tags_to_include,
                )?;
                self.set_config(
                    DeckConfigKey::CustomStudyExcludeTags
                        .for_deck(deck.id)
                        .as_str(),
                    &cram.tags_to_exclude,
                )?;
                Ok(())
            }
        }
    }

    /// Reuse existing one or create new one if missing.
    /// Guaranteed to be a filtered deck.
    fn create_custom_study_deck(&mut self, config: FilteredDeck) -> Result<()> {
        let mut id = DeckId(0);
        let human_name = self.tr.custom_study_custom_study_session().to_string();

        if let Some(did) = self.get_deck_id(&human_name)? {
            if !self.get_deck(did)?.or_not_found(did)?.is_filtered() {
                return Err(CustomStudyError::ExistingDeck.into());
            }
            id = did;
        }

        let deck = FilteredDeckForUpdate {
            id,
            human_name,
            config,
            allow_empty: false,
        };

        self.add_or_update_filtered_deck_inner(deck)
            .map(|_| ())
            .map_err(|err| {
                if matches!(
                    err,
                    AnkiError::FilteredDeckError {
                        source: FilteredDeckError::SearchReturnedNoCards
                    }
                ) {
                    CustomStudyError::NoMatchingCards.into()
                } else {
                    err
                }
            })
    }
}

fn custom_study_config(
    reschedule: bool,
    search: String,
    order: FilteredSearchOrder,
    limit: Option<u32>,
) -> FilteredDeck {
    FilteredDeck {
        reschedule,
        search_terms: vec![FilteredSearchTerm {
            search,
            limit: limit.unwrap_or(99_999),
            order: order as i32,
        }],
        delays: vec![],
        preview_delay: 10,
        preview_again_secs: 60,
        preview_hard_secs: 600,
        preview_good_secs: 0,
    }
}

fn forgot_config(deck_name: String, days: u32) -> FilteredDeck {
    let search = SearchNode::Rated {
        days,
        ease: RatingKind::AnswerButton(1),
    }
    .and(SearchNode::from_deck_name(&deck_name))
    .write();
    custom_study_config(false, search, FilteredSearchOrder::Random, None)
}

fn ahead_config(deck_name: String, days: u32) -> FilteredDeck {
    let search = SearchNode::Property {
        operator: "<=".to_string(),
        kind: PropertyKind::Due(days as i32),
    }
    .and(SearchNode::from_deck_name(&deck_name))
    .write();
    custom_study_config(true, search, FilteredSearchOrder::Due, None)
}

fn preview_config(deck_name: String, days: u32) -> FilteredDeck {
    let search = StateKind::New
        .and_flat(SearchNode::AddedInDays(days))
        .and_flat(SearchNode::from_deck_name(&deck_name))
        .write();
    custom_study_config(false, search, FilteredSearchOrder::Added, None)
}

fn cram_config(deck_name: String, cram: &Cram) -> Result<FilteredDeck> {
    let (reschedule, nodes, order) = match cram.kind() {
        CramKind::New => (
            true,
            SearchBuilder::from(StateKind::New),
            FilteredSearchOrder::Added,
        ),
        CramKind::Due => (
            true,
            SearchBuilder::from(StateKind::Due),
            FilteredSearchOrder::Due,
        ),
        CramKind::Review => (
            true,
            SearchBuilder::from(StateKind::New.negated()),
            FilteredSearchOrder::Random,
        ),
        CramKind::All => (false, SearchBuilder::new(), FilteredSearchOrder::Random),
    };

    let search = nodes
        .and(SearchNode::from_deck_name(&deck_name))
        .and_flat(tags_to_nodes(&cram.tags_to_include, &cram.tags_to_exclude))
        .write();

    Ok(custom_study_config(
        reschedule,
        search,
        order,
        Some(cram.card_limit),
    ))
}

fn tags_to_nodes(tags_to_include: &[String], tags_to_exclude: &[String]) -> SearchBuilder {
    let include_nodes = SearchBuilder::any(
        tags_to_include
            .iter()
            .map(|tag| SearchNode::from_tag_name(tag)),
    );
    let exclude_nodes = SearchBuilder::all(
        tags_to_exclude
            .iter()
            .map(|tag| SearchNode::from_tag_name(tag).negated()),
    );

    include_nodes.and(exclude_nodes)
}

#[cfg(test)]
mod test {
    
    use anki_proto::scheduler::custom_study_request::Cram;
    
    

    use super::*;

    #[test]
    fn sql_grouping() -> Result<()> {
        let mut deck = preview_config("d".into(), 1);
        assert_eq!(&deck.search_terms[0].search, "is:new added:1 deck:d");

        let cram = Cram {
            tags_to_include: vec!["1".into(), "2".into()],
            tags_to_exclude: vec!["3".into(), "4".into()],
            ..Default::default()
        };
        deck = cram_config("d".into(), &cram)?;
        assert_eq!(
            &deck.search_terms[0].search,
            "is:due deck:d (tag:1 OR tag:2) (-tag:3 -tag:4)"
        );

        Ok(())
    }
}
