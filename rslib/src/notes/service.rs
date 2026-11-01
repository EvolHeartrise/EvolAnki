// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

use crate::collection::Collection;
use crate::decks::DeckId;
use crate::error;
use crate::error::OrInvalid;
use crate::error::OrNotFound;
use crate::notes::Note;
use crate::notes::NoteId;

pub(crate) fn to_i64s(ids: Vec<NoteId>) -> Vec<i64> {
    ids.into_iter().map(Into::into).collect()
}

impl crate::services::NotesService for Collection {
    fn new_note(
        &mut self,
        input: anki_proto::notetypes::NotetypeId,
    ) -> error::Result<anki_proto::notes::Note> {
        let ntid = input.into();

        let nt = self.get_notetype(ntid)?.or_not_found(ntid)?;
        Ok(nt.new_note().into())
    }

    fn add_note(
        &mut self,
        input: anki_proto::notes::AddNoteRequest,
    ) -> error::Result<anki_proto::notes::AddNoteResponse> {
        let mut note: Note = input.note.or_invalid("no note provided")?.into();
        let changes = self.add_note(&mut note, DeckId(input.deck_id))?;
        Ok(anki_proto::notes::AddNoteResponse {
            note_id: note.id.0,
            changes: Some(changes.into()),
        })
    }

    fn update_notes(
        &mut self,
        input: anki_proto::notes::UpdateNotesRequest,
    ) -> error::Result<anki_proto::collection::OpChanges> {
        let notes = input
            .notes
            .into_iter()
            .map(Into::into)
            .collect::<Vec<Note>>();
        self.update_notes_maybe_undoable(notes, !input.skip_undo_entry)
            .map(Into::into)
    }

    fn get_note(
        &mut self,
        input: anki_proto::notes::NoteId,
    ) -> error::Result<anki_proto::notes::Note> {
        let nid = input.into();
        self.storage
            .get_note(nid)?
            .or_not_found(nid)
            .map(Into::into)
    }

    fn remove_notes(
        &mut self,
        input: anki_proto::notes::RemoveNotesRequest,
    ) -> error::Result<anki_proto::collection::OpChangesWithCount> {
        if !input.note_ids.is_empty() {
            self.remove_notes(
                &input
                    .note_ids
                    .into_iter()
                    .map(Into::into)
                    .collect::<Vec<_>>(),
            )
        } else {
            let nids = self.storage.note_ids_of_cards(
                &input
                    .card_ids
                    .into_iter()
                    .map(Into::into)
                    .collect::<Vec<_>>(),
            )?;
            self.remove_notes(&nids.into_iter().collect::<Vec<_>>())
        }
        .map(Into::into)
    }

    fn note_fields_check(
        &mut self,
        input: anki_proto::notes::Note,
    ) -> error::Result<anki_proto::notes::NoteFieldsCheckResponse> {
        let note: Note = input.into();

        self.note_fields_check(&note)
            .map(|r| anki_proto::notes::NoteFieldsCheckResponse { state: r as i32 })
    }

    fn cards_of_note(
        &mut self,
        input: anki_proto::notes::NoteId,
    ) -> error::Result<anki_proto::cards::CardIds> {
        self.storage
            .all_card_ids_of_note_in_template_order(NoteId(input.nid))
            .map(|v| anki_proto::cards::CardIds {
                cids: v.into_iter().map(Into::into).collect(),
            })
    }
}

pub(crate) fn to_note_ids(ids: Vec<i64>) -> Vec<NoteId> {
    ids.into_iter().map(NoteId).collect()
}

impl From<anki_proto::notes::NoteId> for NoteId {
    fn from(nid: anki_proto::notes::NoteId) -> Self {
        NoteId(nid.nid)
    }
}

impl From<NoteId> for anki_proto::notes::NoteId {
    fn from(nid: NoteId) -> Self {
        anki_proto::notes::NoteId { nid: nid.0 }
    }
}

#[cfg(test)]
mod test {

    use std::assert_matches;

    use anki_proto::notes::*;

    use crate::collection::Collection;
    use crate::prelude::*;
    use crate::services::NotesService;

    #[test]
    fn note_added() -> Result<()> {
        let mut col = Collection::new();
        let nt = col.basic_notetype();

        // Note ID and changes are returned
        let note = NotesService::new_note(&mut col, nt.id.into())?;
        let response = NotesService::add_note(
            &mut col,
            AddNoteRequest {
                note: Some(note),
                deck_id: 1,
            },
        )?;
        assert_ne!(response.note_id, 0);
        assert_ne!(response.changes, None);

        // No note passed
        let result = NotesService::add_note(
            &mut col,
            AddNoteRequest {
                note: None,
                deck_id: 1,
            },
        );
        assert_matches!(result, Err(AnkiError::InvalidInput { .. }));

        Ok(())
    }

    #[test]
    fn get_note() -> Result<()> {
        let mut col = Collection::new();
        let nt = col.basic_notetype();
        let note1 = col.new_note(nt.id.into())?;
        let response = NotesService::add_note(
            &mut col,
            AddNoteRequest {
                note: Some(note1),
                deck_id: 1,
            },
        )?;
        let nid = response.note_id;
        let note2 = NotesService::get_note(&mut col, anki_proto::notes::NoteId { nid })?;
        assert_eq!(nid, note2.id);

        assert_matches!(
            NotesService::get_note(&mut col, anki_proto::notes::NoteId { nid: 0 }),
            Err(AnkiError::NotFound { .. })
        );

        Ok(())
    }

    #[test]
    fn note_fields_check() -> Result<()> {
        let mut col: Collection = Collection::new();
        let nt = col.basic_notetype();
        let note = col.new_note(nt.id.into())?;
        let _ = NotesService::add_note(
            &mut col,
            AddNoteRequest {
                note: Some(note.clone()),
                deck_id: 1,
            },
        )?;

        let response = NotesService::note_fields_check(&mut col, note)?;
        assert_eq!(
            response.state(),
            anki_proto::notes::note_fields_check_response::State::Empty
        );

        Ok(())
    }

    #[test]
    fn cards_of_note() -> Result<()> {
        let mut col: Collection = Collection::new();
        let nt = col.basic_rev_notetype();
        let mut note = col.new_note(nt.id.into())?;
        note.fields[0] = "f".into();
        note.fields[1] = "b".into();
        let response = NotesService::add_note(
            &mut col,
            AddNoteRequest {
                note: Some(note.clone()),
                deck_id: 1,
            },
        )?;
        let nid = response.note_id;

        let response = NotesService::cards_of_note(&mut col, anki_proto::notes::NoteId { nid })?;
        assert_eq!(response.cids.len(), 2);

        Ok(())
    }
}
