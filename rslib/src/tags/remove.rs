// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

use super::matcher::TagMatcher;
use crate::prelude::*;

impl Collection {
    /// Remove whitespace-separated tags from provided notes.
    pub fn remove_tags_from_notes(
        &mut self,
        nids: &[NoteId],
        tags: &str,
    ) -> Result<OpOutput<usize>> {
        self.transact(Op::RemoveTag, |col| {
            col.remove_tags_from_notes_inner(nids, tags)
        })
    }
}

impl Collection {
    fn remove_tags_from_notes_inner(&mut self, nids: &[NoteId], tags: &str) -> Result<usize> {
        let usn = self.usn()?;

        let mut re = TagMatcher::new(tags)?;
        let mut match_count = 0;
        let notes = self.storage.get_note_tags_by_id_list(nids)?;

        for mut note in notes {
            if !re.is_match(&note.tags) {
                continue;
            }

            match_count += 1;
            let original = note.clone();
            note.tags = re.remove(&note.tags);
            note.set_modified(usn);
            self.update_note_tags_undoable(&note, original)?;
        }

        Ok(match_count)
    }
}
