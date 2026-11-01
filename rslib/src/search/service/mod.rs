// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html



use anki_proto::generic;
use anki_proto::search::sort_order::Value as SortOrderProto;

use crate::prelude::*;
use crate::search::SortMode;

impl crate::services::SearchService for Collection {
    fn search_cards(
        &mut self,
        input: anki_proto::search::SearchRequest,
    ) -> Result<anki_proto::search::SearchResponse> {
        let order = input.order.unwrap_or_default().value.into();
        let cids = self.search_cards(&input.search, order)?;
        Ok(anki_proto::search::SearchResponse {
            ids: cids.into_iter().map(|v| v.0).collect(),
        })
    }

    fn search_notes(
        &mut self,
        input: anki_proto::search::SearchRequest,
    ) -> Result<anki_proto::search::SearchResponse> {
        let order = input.order.unwrap_or_default().value.into();
        let nids = self.search_notes(&input.search, order)?;
        Ok(anki_proto::search::SearchResponse {
            ids: nids.into_iter().map(|v| v.0).collect(),
        })
    }

}

impl From<Option<SortOrderProto>> for SortMode {
    fn from(order: Option<SortOrderProto>) -> Self {
        use anki_proto::search::sort_order::Value as V;
        match order.unwrap_or(V::None(generic::Empty {})) {
            V::None(_) => SortMode::NoOrder,
            V::Custom(s) => SortMode::Custom(s),
        }
    }
}

#[cfg(test)]
mod tests {
    
    

        use crate::collection::Collection;
    
    use crate::search::service::SortOrderProto;
    use crate::search::SortMode;
    use crate::services::SearchService;
    use crate::tests::DeckAdder;
    use crate::tests::NoteAdder;

    // --- Service methods requiring a DB ---

    #[test]
    fn search_cards_returns_card_ids() {
        let mut col = Collection::new();
        NoteAdder::basic(&mut col).add(&mut col);
        let input = anki_proto::search::SearchRequest {
            search: "".to_string(),
            order: None,
        };
        let result = SearchService::search_cards(&mut col, input).unwrap();
        assert!(!result.ids.is_empty(), "expected at least one card id");
    }

    #[test]
    fn search_notes_returns_note_ids() {
        let mut col = Collection::new();
        NoteAdder::basic(&mut col).add(&mut col);
        let input = anki_proto::search::SearchRequest {
            search: "".to_string(),
            order: None,
        };
        let result = SearchService::search_notes(&mut col, input).unwrap();
        assert!(!result.ids.is_empty(), "expected at least one note id");
    }

    // --- Exact-match search integration tests ---

    #[test]
    fn empty_search_returns_all_cards_in_collection() {
        let mut col = Collection::new();
        NoteAdder::basic(&mut col).add(&mut col);
        NoteAdder::basic(&mut col).fields(&["b", ""]).add(&mut col);
        NoteAdder::cloze(&mut col)
            .fields(&["{{c1::x}}", ""])
            .add(&mut col);

        let total_cards = col.storage.get_all_cards().len();
        let result = SearchService::search_cards(
            &mut col,
            anki_proto::search::SearchRequest {
                search: "".to_string(),
                order: None,
            },
        )
        .unwrap();

        assert_eq!(
            result.ids.len(),
            total_cards,
            "empty search should return all {total_cards} cards"
        );
    }

    #[test]
    fn search_cards_by_notetype_returns_only_cards_of_that_type() {
        let mut col = Collection::new();
        // Basic note → 1 card
        let basic_note = NoteAdder::basic(&mut col).add(&mut col);
        // Cloze note → 1 card, different notetype — should NOT appear
        NoteAdder::cloze(&mut col)
            .fields(&["{{c1::hello}}", ""])
            .add(&mut col);

        let result = SearchService::search_cards(
            &mut col,
            anki_proto::search::SearchRequest {
                search: "note:Basic".to_string(),
                order: None,
            },
        )
        .unwrap();

        assert_eq!(
            result.ids.len(),
            1,
            "expected exactly one card for note:Basic"
        );
        let expected_cids = col.storage.card_ids_of_notes(&[basic_note.id]).unwrap();
        assert_eq!(
            result.ids[0], expected_cids[0].0,
            "card id should match the Basic note"
        );
    }

    #[test]
    fn search_cards_by_deck_returns_only_cards_in_that_deck() {
        let mut col = Collection::new();
        // create a named deck
        let deck = DeckAdder::new("TargetDeck").add(&mut col);
        // card in TargetDeck
        let in_deck = NoteAdder::basic(&mut col).deck(deck.id).add(&mut col);
        // card in default deck (id 1) — should NOT appear
        NoteAdder::basic(&mut col)
            .fields(&["other", ""])
            .add(&mut col);

        let result = SearchService::search_cards(
            &mut col,
            anki_proto::search::SearchRequest {
                search: "deck:TargetDeck".to_string(),
                order: None,
            },
        )
        .unwrap();

        assert_eq!(
            result.ids.len(),
            1,
            "expected exactly one card in TargetDeck"
        );
        let expected_cids = col.storage.card_ids_of_notes(&[in_deck.id]).unwrap();
        assert_eq!(
            result.ids[0], expected_cids[0].0,
            "card id should match the note in TargetDeck"
        );
    }

    #[test]
    fn search_cards_by_tag_returns_only_matching_cards() {
        let mut col = Collection::new();
        // note with tag "target" → 1 card
        let mut tagged = NoteAdder::basic(&mut col).note();
        tagged.tags = vec!["target".to_string()];
        col.add_note(&mut tagged, crate::prelude::DeckId(1))
            .unwrap();
        // untagged note → 1 card, should NOT appear
        NoteAdder::basic(&mut col)
            .fields(&["other", ""])
            .add(&mut col);

        let result = SearchService::search_cards(
            &mut col,
            anki_proto::search::SearchRequest {
                search: "tag:target".to_string(),
                order: None,
            },
        )
        .unwrap();

        assert_eq!(
            result.ids.len(),
            1,
            "expected exactly one card for tag:target"
        );
        let expected_cids = col.storage.card_ids_of_notes(&[tagged.id]).unwrap();
        assert_eq!(
            result.ids[0], expected_cids[0].0,
            "card id should match the tagged note"
        );
    }

    // --- From<Option<SortOrderProto>> for SortMode ---

    #[test]
    fn sort_order_none_option_produces_no_order() {
        // outer Option::None → unwrap_or picks V::None → NoOrder
        let mode = SortMode::from(None);
        assert!(matches!(mode, SortMode::NoOrder));
    }

    #[test]
    fn sort_order_v_none_produces_no_order() {
        // explicit V::None variant → NoOrder
        let mode = SortMode::from(Some(SortOrderProto::None(anki_proto::generic::Empty {})));
        assert!(matches!(mode, SortMode::NoOrder));
    }

    #[test]
    fn sort_order_custom_preserves_string() {
        let mode = SortMode::from(Some(SortOrderProto::Custom("due desc".to_string())));
        assert!(
            matches!(mode, SortMode::Custom(ref s) if s == "due desc"),
            "expected Custom(\"due desc\")"
        );
    }

}
