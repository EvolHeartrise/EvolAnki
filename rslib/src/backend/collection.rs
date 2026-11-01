// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

use std::sync::MutexGuard;

use anki_proto::generic;
use tracing::error;

use super::Backend;
use crate::collection::CollectionBuilder;
use crate::prelude::*;
use crate::progress::progress_to_proto;
use crate::services::BackendCollectionService;
use crate::storage::SchemaVersion;

impl BackendCollectionService for Backend {
    fn open_collection(&self, input: anki_proto::collection::OpenCollectionRequest) -> Result<()> {
        let mut guard = self.lock_closed_collection()?;

        let mut builder = CollectionBuilder::new(input.collection_path);
        builder
            .set_media_paths(input.media_folder_path, input.media_db_path)
            .set_server(self.server)
            .set_tr(self.tr.clone())
            .set_shared_progress_state(self.progress_state.clone());

        let mut col = builder.build()?;
        col.ensure_decks_own_settings()?;
        col.migrate_to_fsrs_if_needed();
        *guard = Some(col);

        Ok(())
    }

    fn close_collection(
        &self,
        input: anki_proto::collection::CloseCollectionRequest,
    ) -> Result<()> {
        let desired_version = if input.downgrade_to_schema11 {
            Some(SchemaVersion::V11)
        } else {
            None
        };

        let mut guard = self.lock_open_collection()?;
        let col_inner = guard.take().unwrap();

        if let Err(e) = col_inner.close(desired_version) {
            error!(" failed: {:?}", e);
        }

        Ok(())
    }

    fn create_backup(
        &self,
        input: anki_proto::collection::CreateBackupRequest,
    ) -> Result<generic::Bool> {
        // lock collection
        let mut col_lock = self.lock_open_collection()?;
        let col = col_lock.as_mut().unwrap();
        // await any previous backup first
        let mut task_lock = self.backup_task.lock().unwrap();
        if let Some(task) = task_lock.take() {
            task.join().unwrap()?;
        }
        // start the new backup
        let created = if let Some(task) = col.maybe_backup(input.backup_folder, input.force)? {
            if input.wait_for_completion {
                drop(col_lock);
                task.join().unwrap()?;
            } else {
                *task_lock = Some(task);
            }
            true
        } else {
            false
        };
        Ok(created.into())
    }

    fn await_backup_completion(&self) -> Result<()> {
        self.await_backup_completion()?;
        Ok(())
    }

    fn latest_progress(&self) -> Result<anki_proto::collection::Progress> {
        let progress = self.progress_state.lock().unwrap().last_progress;
        Ok(progress_to_proto(progress, &self.tr))
    }

    fn set_wants_abort(&self) -> Result<()> {
        self.progress_state.lock().unwrap().want_abort = true;
        Ok(())
    }
}

impl Backend {
    pub(super) fn lock_open_collection(&self) -> Result<MutexGuard<'_, Option<Collection>>> {
        let guard = self.col.lock().unwrap();
        guard
            .is_some()
            .then_some(guard)
            .ok_or(AnkiError::CollectionNotOpen)
    }

    pub(super) fn lock_closed_collection(&self) -> Result<MutexGuard<'_, Option<Collection>>> {
        let guard = self.col.lock().unwrap();
        guard
            .is_none()
            .then_some(guard)
            .ok_or(AnkiError::CollectionAlreadyOpen)
    }

    fn await_backup_completion(&self) -> Result<()> {
        if let Some(task) = self.backup_task.lock().unwrap().take() {
            task.join().unwrap()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use anki_i18n::I18n;
    use tempfile::tempdir;

    use super::*;
    use crate::tests::NoteAdder;

    #[test]
    fn opening_migrates_sm2_collection_but_building_a_temporary_one_does_not() -> Result<()> {
        let dir = tempdir()?;
        let path = dir.path().join("col.anki2");
        let mut col = CollectionBuilder::new(&path).build()?;
        NoteAdder::basic(&mut col).add(&mut col);
        col.answer_easy();
        let mut card = col.get_first_card();
        card.clear_fsrs_data();
        col.storage.update_card(&card)?;
        col.close(None)?;

        // as done with temporary collections during import
        let col = CollectionBuilder::new(&path).build()?;
        assert!(col.get_first_card().memory_state.is_none());
        col.close(None)?;

        let backend = Backend::new(I18n::template_only(), false);
        BackendCollectionService::open_collection(
            &backend,
            anki_proto::collection::OpenCollectionRequest {
                collection_path: path.to_string_lossy().into(),
                ..Default::default()
            },
        )?;
        let guard = backend.lock_open_collection()?;
        let col = guard.as_ref().unwrap();
        assert!(col.get_first_card().memory_state.is_some());
        Ok(())
    }
}
