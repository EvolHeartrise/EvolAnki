// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

use crate::card::CardQueue;
use crate::config::SchedulerVersion;
use crate::prelude::*;

impl Collection {
    /// Marks the cards as modified.
    pub fn unsuspend_cards(&mut self, cids: &[CardId]) -> Result<OpOutput<()>> {
        self.transact(Op::Unsuspend, |col| {
            for original in col.all_cards_for_ids(cids, false)? {
                if original.queue == CardQueue::Suspended {
                    let mut card = original.clone();
                    card.restore_queue_from_type();
                    col.update_card_inner(&mut card, original)?;
                }
            }
            Ok(())
        })
    }

    /// Marks the cards as modified.
    pub fn suspend_cards(&mut self, cids: &[CardId]) -> Result<OpOutput<usize>> {
        self.transact(Op::Suspend, |col| {
            if col.scheduler_version() == SchedulerVersion::V1 {
                return Err(AnkiError::SchedulerUpgradeRequired);
            }
            let mut count = 0;
            for original in col.all_cards_for_ids(cids, false)? {
                if original.queue != CardQueue::Suspended {
                    let mut card = original.clone();
                    card.queue = CardQueue::Suspended;
                    count += 1;
                    col.update_card_inner(&mut card, original)?;
                }
            }
            Ok(count)
        })
    }
}
