// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html
use anki_proto::generic;

use crate::collection::Collection;
use crate::error;
use crate::notes::service::to_note_ids;

impl crate::services::TagsService for Collection {
    fn all_tags(&mut self) -> error::Result<generic::StringList> {
        Ok(generic::StringList {
            vals: self
                .storage
                .all_tags()?
                .into_iter()
                .map(|t| t.name)
                .collect(),
        })
    }

    fn rename_tags(
        &mut self,
        input: anki_proto::tags::RenameTagsRequest,
    ) -> error::Result<anki_proto::collection::OpChangesWithCount> {
        self.rename_tag(&input.current_prefix, &input.new_prefix)
            .map(Into::into)
    }

    fn add_note_tags(
        &mut self,
        input: anki_proto::tags::NoteIdsAndTagsRequest,
    ) -> error::Result<anki_proto::collection::OpChangesWithCount> {
        self.add_tags_to_notes(&to_note_ids(input.note_ids), &input.tags)
            .map(Into::into)
    }

    fn remove_note_tags(
        &mut self,
        input: anki_proto::tags::NoteIdsAndTagsRequest,
    ) -> error::Result<anki_proto::collection::OpChangesWithCount> {
        self.remove_tags_from_notes(&to_note_ids(input.note_ids), &input.tags)
            .map(Into::into)
    }

}

#[cfg(test)]
mod tests {
    
    use crate::prelude::*;
    use crate::services::TagsService;

    fn add_note_with_tags(col: &mut Collection, tags: &[&str]) -> Result<NoteId> {
        let notetype = col.get_notetype_by_name("Basic")?.unwrap();
        let mut note = notetype.new_note();
        note.tags = tags.iter().map(ToString::to_string).collect();
        col.add_note(&mut note, DeckId(1))?;
        Ok(note.id)
    }

    fn stored_tags(col: &Collection, note_id: NoteId) -> Result<Vec<String>> {
        let mut tags = col.storage.get_note(note_id)?.unwrap().tags;
        tags.sort();
        Ok(tags)
    }

    fn registered_tags(col: &Collection) -> Result<Vec<String>> {
        let mut tags = col
            .storage
            .all_tags()?
            .into_iter()
            .map(|tag| tag.name)
            .collect::<Vec<_>>();
        tags.sort();
        Ok(tags)
    }

    #[test]
    fn add_note_tags_canonifies_and_persists_names() -> Result<()> {
        let mut col = Collection::new();
        add_note_with_tags(&mut col, &["Parent"])?;
        let note_id = add_note_with_tags(&mut col, &[])?;

        let response = TagsService::add_note_tags(
            &mut col,
            anki_proto::tags::NoteIdsAndTagsRequest {
                note_ids: vec![note_id.0],
                tags: "parent::Child PARENT::child Extra".into(),
            },
        )?;

        assert_eq!(response.count, 1);
        assert!(response.changes.is_some());
        assert_eq!(stored_tags(&col, note_id)?, ["Extra", "Parent::Child"]);
        assert_eq!(registered_tags(&col)?, ["Extra", "Parent", "Parent::Child"]);
        Ok(())
    }

    #[test]
    fn all_tags_returns_registered_names() -> Result<()> {
        let mut col = Collection::new();
        add_note_with_tags(&mut col, &["second", "first"])?;

        let mut response = TagsService::all_tags(&mut col)?.vals;
        response.sort();

        assert_eq!(response, ["first", "second"]);
        Ok(())
    }

    #[test]
    fn remove_note_tags_only_changes_requested_notes() -> Result<()> {
        let mut col = Collection::new();
        let selected = add_note_with_tags(&mut col, &["keep", "shared"])?;
        let unselected = add_note_with_tags(&mut col, &["keep", "shared"])?;

        let response = TagsService::remove_note_tags(
            &mut col,
            anki_proto::tags::NoteIdsAndTagsRequest {
                note_ids: vec![selected.0],
                tags: "shared".into(),
            },
        )?;

        assert_eq!(response.count, 1);
        assert!(response.changes.is_some());
        assert_eq!(stored_tags(&col, selected)?, ["keep"]);
        assert_eq!(stored_tags(&col, unselected)?, ["keep", "shared"]);
        Ok(())
    }

    #[test]
    fn rename_tags_updates_prefix_and_children() -> Result<()> {
        let mut col = Collection::new();
        let note_id = add_note_with_tags(&mut col, &["keep", "old", "old::Child"])?;

        let response = TagsService::rename_tags(
            &mut col,
            anki_proto::tags::RenameTagsRequest {
                current_prefix: "old".into(),
                new_prefix: "New".into(),
            },
        )?;

        assert_eq!(response.count, 1);
        assert!(response.changes.is_some());
        assert_eq!(stored_tags(&col, note_id)?, ["New", "New::Child", "keep"]);
        assert_eq!(registered_tags(&col)?, ["New", "New::Child", "keep"]);
        Ok(())
    }

    #[test]
    fn rename_tags_rejects_blank_new_prefix_without_changing_state() -> Result<()> {
        let mut col = Collection::new();
        let note_id = add_note_with_tags(&mut col, &["old"])?;

        let error = TagsService::rename_tags(
            &mut col,
            anki_proto::tags::RenameTagsRequest {
                current_prefix: "old".into(),
                new_prefix: "   ".into(),
            },
        )
        .unwrap_err();

        assert!(matches!(error, AnkiError::InvalidInput { .. }));
        assert_eq!(stored_tags(&col, note_id)?, ["old"]);
        assert_eq!(registered_tags(&col)?, ["old"]);
        Ok(())
    }

}
