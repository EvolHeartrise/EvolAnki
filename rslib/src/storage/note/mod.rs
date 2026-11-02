// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

use std::collections::HashMap;
use std::collections::HashSet;

use rusqlite::params;
use rusqlite::Row;

use crate::import_export::package::NoteMeta;
use crate::notes::NoteTags;
use crate::prelude::*;
use crate::tags::join_tags;
use crate::tags::split_tags;

pub(crate) fn split_fields(fields: &str) -> Vec<String> {
    fields.split('\x1f').map(Into::into).collect()
}

pub(crate) fn join_fields(fields: &[String]) -> String {
    fields.join("\x1f")
}

impl super::SqliteStorage {
    pub fn get_note(&self, nid: NoteId) -> Result<Option<Note>> {
        self.db
            .prepare_cached(concat!(include_str!("get.sql"), " where id = ?"))?
            .query_and_then(params![nid], row_to_note)?
            .next()
            .transpose()
    }

    pub fn get_all_note_ids(&self) -> Result<HashSet<NoteId>> {
        self.db
            .prepare("SELECT id FROM notes")?
            .query_and_then([], |row| Ok(row.get(0)?))?
            .collect()
    }

    /// If fields have been modified, caller must call note.prepare_for_update()
    /// prior to calling this.
    pub(crate) fn update_note(&self, note: &Note) -> Result<()> {
        assert_ne!(note.id.0, 0);
        let mut stmt = self.db.prepare_cached(include_str!("update.sql"))?;
        stmt.execute(params![
            note.guid,
            note.notetype_id,
            note.mtime,
            join_tags(&note.tags),
            join_fields(note.fields()),
            note.sort_field.as_ref().unwrap(),
            note.checksum.unwrap(),
            note.id
        ])?;
        Ok(())
    }

    pub(crate) fn add_note(&self, note: &mut Note) -> Result<()> {
        assert_eq!(note.id.0, 0);
        let mut stmt = self.db.prepare_cached(include_str!("add.sql"))?;
        stmt.execute(params![
            TimestampMillis::now(),
            note.guid,
            note.notetype_id,
            note.mtime,
            join_tags(&note.tags),
            join_fields(note.fields()),
            note.sort_field.as_ref().unwrap(),
            note.checksum.unwrap(),
        ])?;
        note.id.0 = self.db.last_insert_rowid();
        Ok(())
    }

    pub(crate) fn add_note_if_unique(&self, note: &Note) -> Result<bool> {
        self.db
            .prepare_cached(include_str!("add_if_unique.sql"))?
            .execute(params![
                note.id,
                note.guid,
                note.notetype_id,
                note.mtime,
                join_tags(&note.tags),
                join_fields(note.fields()),
                note.sort_field.as_ref().unwrap(),
                note.checksum.unwrap(),
            ])
            .map(|added| added == 1)
            .map_err(Into::into)
    }

    /// Add or update the provided note, preserving ID. Used by the syncing
    /// code.
    pub(crate) fn add_or_update_note(&self, note: &Note) -> Result<()> {
        let mut stmt = self.db.prepare_cached(include_str!("add_or_update.sql"))?;
        stmt.execute(params![
            note.id,
            note.guid,
            note.notetype_id,
            note.mtime,
            join_tags(&note.tags),
            join_fields(note.fields()),
            note.sort_field.as_ref().unwrap(),
            note.checksum.unwrap(),
        ])?;
        Ok(())
    }

    pub(crate) fn remove_note(&self, nid: NoteId) -> Result<()> {
        self.db
            .prepare_cached("delete from notes where id = ?")?
            .execute([nid])?;
        Ok(())
    }

    pub(crate) fn note_is_orphaned(&self, nid: NoteId) -> Result<bool> {
        self.db
            .prepare_cached(include_str!("is_orphaned.sql"))?
            .query_row([nid], |r| r.get(0))
            .map_err(Into::into)
    }

    pub(crate) fn fix_invalid_utf8_in_note(&self, nid: NoteId) -> Result<()> {
        self.db
            .query_row(
                "select cast(flds as blob), cast(tags as blob) from notes where id=?",
                [nid],
                |row| {
                    let fixed_flds: Vec<u8> = row.get(0)?;
                    let fixed_str = String::from_utf8_lossy(&fixed_flds);
                    let fixed_tags: Vec<u8> = row.get(1)?;
                    let fixed_tags = String::from_utf8_lossy(&fixed_tags);
                    self.db.execute(
                        "update notes set flds = ?, sfld = '', tags = ? where id = ?",
                        params![fixed_str, fixed_tags, nid],
                    )
                },
            )
            .map_err(Into::into)
            .map(|_| ())
    }

    /// Returns [(nid, field 0)] of notes with the same checksum.
    /// The caller should strip the fields and compare to see if they actually
    /// match.
    pub(crate) fn note_fields_by_checksum(
        &self,
        ntid: NotetypeId,
        csum: u32,
    ) -> Result<Vec<(NoteId, String)>> {
        self.db
            .prepare("select id, field_at_index(flds, 0) from notes where csum=? and mid=?")?
            .query_and_then(params![csum, ntid], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect()
    }

    /// Returns [(nid, field 0)] of notes with the same checksum.
    /// The caller should strip the fields and compare to see if they actually
    /// match.
    pub(crate) fn all_notes_by_type_and_checksum(
        &self,
    ) -> Result<HashMap<(NotetypeId, u32), Vec<NoteId>>> {
        let mut map = HashMap::new();
        let mut stmt = self.db.prepare("SELECT mid, csum, id FROM notes")?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            map.entry((row.get(0)?, row.get(1)?))
                .or_insert_with(Vec::new)
                .push(row.get(2)?);
        }
        Ok(map)
    }

    pub(crate) fn all_notes_by_type_checksum_and_deck(
        &self,
    ) -> Result<HashMap<(NotetypeId, u32, DeckId), Vec<NoteId>>> {
        let mut map = HashMap::new();
        let mut stmt = self
            .db
            .prepare(include_str!("notes_types_checksums_decks.sql"))?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            map.entry((row.get(1)?, row.get(2)?, row.get(3)?))
                .or_insert_with(Vec::new)
                .push(row.get(0)?);
        }
        Ok(map)
    }

    /// Return total number of notes. Slow.
    pub(crate) fn total_notes(&self) -> Result<u32> {
        self.db
            .prepare("select count() from notes")?
            .query_row([], |r| r.get(0))
            .map_err(Into::into)
    }

    pub(crate) fn get_note_tags_by_id(&mut self, note_id: NoteId) -> Result<Option<NoteTags>> {
        self.db
            .prepare_cached(&format!("{} where id = ?", include_str!("get_tags.sql")))?
            .query_and_then([note_id], row_to_note_tags)?
            .next()
            .transpose()
    }

    pub(crate) fn get_note_tags_by_id_list(&self, note_ids: &[NoteId]) -> Result<Vec<NoteTags>> {
        self.with_ids_in_searched_notes_table(note_ids, || {
            self.db
                .prepare_cached(&format!(
                    "{} where id in (select nid from search_nids)",
                    include_str!("get_tags.sql")
                ))?
                .query_and_then([], row_to_note_tags)?
                .collect()
        })
    }

    pub(crate) fn all_searched_notes(&self) -> Result<Vec<Note>> {
        self.db
            .prepare_cached(concat!(
                include_str!("get.sql"),
                " WHERE id IN (SELECT nid FROM search_nids)"
            ))?
            .query_and_then([], row_to_note)?
            .collect()
    }

    pub(crate) fn get_note_tags_by_predicate<F>(&mut self, want: F) -> Result<Vec<NoteTags>>
    where
        F: Fn(&str) -> bool,
    {
        let mut query_stmt = self.db.prepare_cached(include_str!("get_tags.sql"))?;
        let mut rows = query_stmt.query([])?;
        let mut output = vec![];
        while let Some(row) = rows.next()? {
            let tags = row.get_ref_unwrap(2).as_str()?;
            if want(tags) {
                output.push(row_to_note_tags(row)?)
            }
        }
        Ok(output)
    }

    pub(crate) fn update_note_tags(&mut self, note: &NoteTags) -> Result<()> {
        self.db
            .prepare_cached(include_str!("update_tags.sql"))?
            .execute(params![note.mtime, note.tags, note.id])?;
        Ok(())
    }

    pub(crate) fn setup_searched_notes_table(&self) -> Result<()> {
        self.db
            .execute_batch(include_str!("search_nids_setup.sql"))?;
        Ok(())
    }

    pub(crate) fn clear_searched_notes_table(&self) -> Result<()> {
        self.db.execute("drop table if exists search_nids", [])?;
        Ok(())
    }

    /// Executes the closure with the note ids placed in the search_nids table.
    /// WARNING: the column name is nid, not id.
    pub(crate) fn with_ids_in_searched_notes_table<T>(
        &self,
        note_ids: &[NoteId],
        func: impl FnOnce() -> Result<T>,
    ) -> Result<T> {
        self.setup_searched_notes_table()?;
        let mut stmt = self
            .db
            .prepare_cached("insert into search_nids values (?)")?;
        for nid in note_ids {
            stmt.execute([nid])?;
        }
        let result = func();
        self.clear_searched_notes_table()?;
        result
    }

    pub(crate) fn note_guid_map(&mut self) -> Result<HashMap<String, NoteMeta>> {
        self.db
            .prepare("SELECT guid, id, mod, mid FROM notes")?
            .query_and_then([], row_to_note_meta)?
            .collect()
    }

    pub(crate) fn all_notes_by_guid(&mut self) -> Result<HashMap<String, NoteId>> {
        self.db
            .prepare("SELECT guid, id FROM notes")?
            .query_and_then([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect()
    }

    #[cfg(test)]
    pub(crate) fn get_all_notes(&mut self) -> Vec<Note> {
        self.db
            .prepare("SELECT * FROM notes")
            .unwrap()
            .query_and_then([], row_to_note)
            .unwrap()
            .collect::<Result<_>>()
            .unwrap()
    }

    #[cfg(test)]
    pub(crate) fn notes_table_len(&mut self) -> usize {
        self.db_scalar("SELECT COUNT(*) FROM notes").unwrap()
    }
}

fn row_to_note(row: &Row) -> Result<Note> {
    Ok(Note::new_from_storage(
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        split_tags(row.get_ref_unwrap(4).as_str()?)
            .map(Into::into)
            .collect(),
        split_fields(row.get_ref_unwrap(5).as_str()?),
        Some(row.get(6)?),
        Some(row.get(7).unwrap_or_default()),
    ))
}

fn row_to_note_tags(row: &Row) -> Result<NoteTags> {
    Ok(NoteTags {
        id: row.get(0)?,
        mtime: row.get(1)?,
        tags: row.get(2)?,
    })
}

fn row_to_note_meta(row: &Row) -> Result<(String, NoteMeta)> {
    Ok((
        row.get(0)?,
        NoteMeta::new(row.get(1)?, row.get(2)?, row.get(3)?),
    ))
}
