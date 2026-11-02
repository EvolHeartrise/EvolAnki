// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

use crate::prelude::*;

#[derive(Debug)]

pub(crate) enum UndoableDeckChange {
    Added(Box<Deck>),
    Updated(Box<Deck>),
    Removed(Box<Deck>),
}

impl Collection {
    pub(crate) fn undo_deck_change(&mut self, change: UndoableDeckChange) -> Result<()> {
        match change {
            UndoableDeckChange::Added(deck) => self.remove_deck_undoable(*deck),
            UndoableDeckChange::Updated(mut deck) => {
                let current = self
                    .storage
                    .get_deck(deck.id)?
                    .or_invalid("deck disappeared")?;
                self.update_single_deck_undoable(&mut deck, current)
            }
            UndoableDeckChange::Removed(deck) => self.restore_deleted_deck(*deck),
        }
    }

    pub(crate) fn remove_deck_and_settings_undoable(&mut self, deck: Deck) -> Result<()> {
        self.state.deck_cache.clear();
        self.storage.remove_deck(deck.id)?;
        if let Some(config) = self.storage.get_deck_config(DeckConfigId(deck.id.0))? {
            self.remove_deck_config_undoable(config)?;
        }
        self.save_undo(UndoableDeckChange::Removed(Box::new(deck)));
        Ok(())
    }
}

impl Collection {
    pub(super) fn add_deck_undoable(&mut self, deck: &mut Deck) -> Result<(), AnkiError> {
        self.storage.add_deck(deck)?;
        self.save_undo(UndoableDeckChange::Added(Box::new(deck.clone())));
        self.add_settings_if_missing(deck)
    }

    pub(super) fn add_or_update_deck_with_existing_id_undoable(
        &mut self,
        deck: &mut Deck,
    ) -> Result<(), AnkiError> {
        self.state.deck_cache.clear();
        self.storage.add_or_update_deck_with_existing_id(deck)?;
        self.save_undo(UndoableDeckChange::Added(Box::new(deck.clone())));
        self.add_settings_if_missing(deck)
    }

    /// Update an individual, existing deck. Caller is responsible for ensuring
    /// deck is normalized, matches parents, is not a duplicate name, and
    /// bumping mtime. Clears deck cache.
    pub(super) fn update_single_deck_undoable(
        &mut self,
        deck: &mut Deck,
        original: Deck,
    ) -> Result<()> {
        self.state.deck_cache.clear();
        self.save_undo(UndoableDeckChange::Updated(Box::new(original)));
        self.storage.update_deck(deck)
    }

    fn restore_deleted_deck(&mut self, deck: Deck) -> Result<()> {
        self.storage.add_or_update_deck_with_existing_id(&deck)?;
        self.save_undo(UndoableDeckChange::Added(Box::new(deck)));
        Ok(())
    }

    fn remove_deck_undoable(&mut self, deck: Deck) -> Result<()> {
        self.state.deck_cache.clear();
        self.storage.remove_deck(deck.id)?;
        if let Some(config) = self.storage.get_deck_config(DeckConfigId(deck.id.0))? {
            self.remove_deck_config_undoable(config)?;
        }
        self.save_undo(UndoableDeckChange::Removed(Box::new(deck)));
        Ok(())
    }
}
