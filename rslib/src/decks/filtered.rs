// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

use super::DeckCommon;
use super::FilteredDeck;
use super::FilteredSearchOrder;
use super::FilteredSearchTerm;
use crate::prelude::*;

impl Deck {
    pub fn new_filtered() -> Deck {
        let mut filt = FilteredDeck::default();
        filt.search_terms.push(FilteredSearchTerm {
            search: "".into(),
            limit: 100,
            order: FilteredSearchOrder::Random as i32,
        });
        filt.search_terms.push(FilteredSearchTerm {
            search: "".into(),
            limit: 20,
            order: FilteredSearchOrder::Due as i32,
        });
        filt.preview_again_secs = 60;
        filt.preview_hard_secs = 600;
        filt.reschedule = true;
        Deck {
            id: DeckId(0),
            name: NativeDeckName::from_native_str(""),
            mtime_secs: TimestampSecs(0),
            common: DeckCommon {
                study_collapsed: true,
                browser_collapsed: true,
                ..Default::default()
            },
            kind: DeckKind::Filtered(filt),
        }
    }

    pub(crate) fn is_normal(&self) -> bool {
        !self.is_filtered()
    }

    pub(crate) fn is_filtered(&self) -> bool {
        matches!(self.kind, DeckKind::Filtered(_))
    }
}
