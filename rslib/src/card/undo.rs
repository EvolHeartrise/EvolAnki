// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

use crate::prelude::*;

#[derive(Debug)]
pub(crate) enum UndoableCardChange {
    Added(Box<Card>),
    Updated(Box<Card>),
    Removed(Box<Card>),
}

impl Collection {
    pub(crate) fn undo_card_change(&mut self, change: UndoableCardChange) -> Result<()> {
        match change {
            UndoableCardChange::Added(card) => self.remove_card_undoable(*card),
            UndoableCardChange::Updated(mut card) => {
                let current = self
                    .storage
                    .get_card(card.id)?
                    .or_invalid("card disappeared")?;
                self.update_card_undoable(&mut card, current)
            }
            UndoableCardChange::Removed(card) => self.restore_deleted_card(*card),
        }
    }

    pub(super) fn add_card_undoable(&mut self, card: &mut Card) -> Result<(), AnkiError> {
        self.storage.add_card(card)?;
        self.save_undo(UndoableCardChange::Added(Box::new(card.clone())));
        Ok(())
    }

    pub(crate) fn add_card_if_unique_undoable(&mut self, card: &Card) -> Result<bool> {
        let added = self.storage.add_card_if_unique(card)?;
        if added {
            self.save_undo(UndoableCardChange::Added(Box::new(card.clone())));
        }
        Ok(added)
    }

    pub(super) fn update_card_undoable(&mut self, card: &mut Card, original: Card) -> Result<()> {
        require!(card.id.0 != 0, "card id not set");
        self.save_undo(UndoableCardChange::Updated(Box::new(original)));
        self.storage.update_card(card)
    }

    pub(crate) fn remove_card_undoable(&mut self, card: Card) -> Result<()> {
        self.storage.remove_card(card.id)?;
        self.save_undo(UndoableCardChange::Removed(Box::new(card)));
        Ok(())
    }

    fn restore_deleted_card(&mut self, card: Card) -> Result<()> {
        self.storage.add_or_update_card(&card)?;
        self.save_undo(UndoableCardChange::Added(Box::new(card)));
        Ok(())
    }
}
