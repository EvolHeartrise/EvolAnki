// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

use rusqlite::params;

use super::SqliteStorage;
use crate::prelude::*;

#[repr(u8)]
enum GraveKind {
    Card,
    Note,
    Deck,
}

impl SqliteStorage {
    #[cfg(test)]
    pub(crate) fn clear_all_graves(&self) -> Result<()> {
        self.db.execute("delete from graves", [])?;
        Ok(())
    }

    pub(crate) fn add_card_grave(&self, cid: CardId, usn: Usn) -> Result<()> {
        self.add_grave(cid.0, GraveKind::Card, usn)
    }

    pub(crate) fn add_note_grave(&self, nid: NoteId, usn: Usn) -> Result<()> {
        self.add_grave(nid.0, GraveKind::Note, usn)
    }

    pub(crate) fn add_deck_grave(&self, did: DeckId, usn: Usn) -> Result<()> {
        self.add_grave(did.0, GraveKind::Deck, usn)
    }

    pub(crate) fn remove_card_grave(&self, cid: CardId) -> Result<()> {
        self.remove_grave(cid.0, GraveKind::Card)
    }

    pub(crate) fn remove_note_grave(&self, nid: NoteId) -> Result<()> {
        self.remove_grave(nid.0, GraveKind::Note)
    }

    pub(crate) fn remove_deck_grave(&self, did: DeckId) -> Result<()> {
        self.remove_grave(did.0, GraveKind::Deck)
    }

    fn add_grave(&self, oid: i64, kind: GraveKind, usn: Usn) -> Result<()> {
        self.db
            .prepare_cached(include_str!("add.sql"))?
            .execute(params![usn, oid, kind as u8])?;
        Ok(())
    }

    /// Only useful when undoing
    fn remove_grave(&self, oid: i64, kind: GraveKind) -> Result<()> {
        self.db
            .prepare_cached(include_str!("remove.sql"))?
            .execute(params![oid, kind as u8])?;
        Ok(())
    }
}
