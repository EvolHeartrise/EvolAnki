// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

use fsrs::FSRS5_DEFAULT_DECAY;

use crate::prelude::*;
use crate::revlog::RevlogEntry;

impl Collection {
    pub fn card_stats(&mut self, cid: CardId) -> Result<anki_proto::stats::CardStatsResponse> {
        let card = self.storage.get_card(cid)?.or_not_found(cid)?;
        let note = self
            .storage
            .get_note(card.note_id)?
            .or_not_found(card.note_id)?;
        let nt = self
            .get_notetype(note.notetype_id)?
            .or_not_found(note.notetype_id)?;
        let deck = self
            .storage
            .get_deck(card.deck_id)?
            .or_not_found(card.deck_id)?;
        let revlog = self.storage.get_revlog_entries_for_card(card.id)?;
        let timing = self.timing_today()?;

        let last_review_time = if let Some(last_review_time) = card.last_review_time {
            last_review_time
        } else {
            let mut new_card = card.clone();
            let last_review_time = self
                .storage
                .time_of_last_review(card.id)?
                .unwrap_or_default();

            new_card.last_review_time = Some(last_review_time);

            self.storage.update_card(&new_card)?;
            last_review_time
        };

        let seconds_elapsed = timing.now.elapsed_secs_since(last_review_time) as u32;

        let fsrs_retrievability = card
            .memory_state
            .zip(Some(seconds_elapsed))
            .zip(Some(card.decay.unwrap_or(FSRS5_DEFAULT_DECAY)))
            .map(|((state, seconds), decay)| {
                fsrs::current_retrievability(state.into(), seconds as f32 / 86_400.0, decay)
            });

        let original_deck = if card.original_deck_id == DeckId(0) {
            deck.clone()
        } else {
            self.storage
                .get_deck(card.original_deck_id)?
                .or_not_found(card.original_deck_id)?
        };
        let config_id = original_deck.config_id().unwrap();
        let config = self
            .get_deck_config(config_id, true)?
            .or_not_found(config_id.to_string())?;

        Ok(anki_proto::stats::CardStatsResponse {
            deck: deck.human_name(),
            notetype: nt.name.clone(),
            revlog: revlog.iter().rev().map(stats_revlog_entry).collect(),
            fsrs_retrievability,
            fsrs_params: config.fsrs_params().to_vec(),
        })
    }
}

fn stats_revlog_entry(
    entry: &RevlogEntry,
) -> anki_proto::stats::card_stats_response::StatsRevlogEntry {
    anki_proto::stats::card_stats_response::StatsRevlogEntry {
        time: entry.id.as_secs().0,
        review_kind: entry.review_kind.into(),
        button_chosen: entry.button_chosen as u32,
        interval: entry.interval_secs(),
        ease: entry.ease_factor,
        taken_secs: entry.taken_millis as f32 / 1000.,
        last_interval: entry.last_interval_secs(),
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::search::SortMode;

    fn test_collection() -> Result<(Collection, CardId)> {
        let mut col = Collection::new();
        let nt = col.get_notetype_by_name("Basic")?.unwrap();
        let mut note = nt.new_note();
        col.add_note(&mut note, DeckId(1))?;
        let cid = col.search_cards("", SortMode::NoOrder)?[0];
        Ok((col, cid))
    }

    #[test]
    fn stats() -> Result<()> {
        let (mut col, cid) = test_collection()?;
        let _report = col.card_stats(cid)?;

        Ok(())
    }
}
