// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

mod service;
pub(crate) mod timestamps;
mod transact;
pub(crate) mod undo;

use std::collections::HashMap;
use std::fmt::Debug;
use std::fmt::Formatter;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;

#[cfg(test)]
use anki_io::create_dir_all;

use crate::decks::Deck;
use crate::decks::DeckId;
use crate::error::Result;
use crate::i18n::I18n;
use crate::notetype::Notetype;
use crate::notetype::NotetypeId;
use crate::progress::ProgressState;
use crate::scheduler::queue::CardQueues;
use crate::scheduler::SchedulerInfo;
use crate::storage::SqliteStorage;
use crate::undo::UndoManager;

#[derive(Default)]
pub struct CollectionBuilder {
    collection_path: Option<PathBuf>,
    media_folder: Option<PathBuf>,
    media_db: Option<PathBuf>,
    tr: Option<I18n>,
    check_integrity: bool,
    progress_handler: Option<Arc<Mutex<ProgressState>>>,
}

impl CollectionBuilder {
    /// Create a new builder with the provided collection path.
    /// If an in-memory database is desired, used ::default() instead.
    pub fn new(col_path: impl Into<PathBuf>) -> Self {
        let mut builder = Self::default();
        builder.set_collection_path(col_path);
        builder
    }

    pub fn build(&mut self) -> Result<Collection> {
        let col_path = self
            .collection_path
            .clone()
            .unwrap_or_else(|| PathBuf::from(":memory:"));
        let tr = self.tr.clone().unwrap_or_default();
        let media_folder = self.media_folder.clone().unwrap_or_default();
        let media_db = self.media_db.clone().unwrap_or_default();
        let storage = SqliteStorage::open_or_create(&col_path, &tr, self.check_integrity)?;
        let mut col = Collection {
            storage,
            col_path,
            media_folder,
            media_db,
            tr,
            state: CollectionState {
                progress: self.progress_handler.clone().unwrap_or_default(),
                ..Default::default()
            },
        };
        col.upgrade_v1_scheduler()?;

        Ok(col)
    }

    pub fn set_collection_path<P: Into<PathBuf>>(&mut self, collection: P) -> &mut Self {
        self.collection_path = Some(collection.into());
        self
    }

    pub fn set_media_paths<P: Into<PathBuf>>(&mut self, media_folder: P, media_db: P) -> &mut Self {
        self.media_folder = Some(media_folder.into());
        self.media_db = Some(media_db.into());
        self
    }

    #[cfg(test)]
    /// For a `foo.anki2` file, use `foo.media` and `foo.mdb`. Mobile clients
    /// use different paths, so the backend must continue to use
    /// [set_media_paths].
    pub fn with_desktop_media_paths(&mut self) -> &mut Self {
        let col_path = self.collection_path.as_ref().unwrap();
        let media_folder = col_path.with_extension("media");
        create_dir_all(&media_folder).expect("creating media folder");
        let media_db = col_path.with_extension("mdb");
        self.set_media_paths(media_folder, media_db)
    }

    pub fn set_tr(&mut self, tr: I18n) -> &mut Self {
        self.tr = Some(tr);
        self
    }

    /// If provided, progress info will be written to the provided mutex, and
    /// can be tracked on a separate thread.
    pub fn set_shared_progress_state(&mut self, state: Arc<Mutex<ProgressState>>) -> &mut Self {
        self.progress_handler = Some(state);
        self
    }
}

#[derive(Debug, Default)]
pub struct CollectionState {
    pub(crate) undo: UndoManager,
    pub(crate) notetype_cache: HashMap<NotetypeId, Arc<Notetype>>,
    pub(crate) deck_cache: HashMap<DeckId, Arc<Deck>>,
    pub(crate) scheduler_info: Option<SchedulerInfo>,
    pub(crate) card_queues: Option<CardQueues>,
    /// The modification time at the last backup, so we don't create multiple
    /// identical backups.
    pub(crate) progress: Arc<Mutex<ProgressState>>,
}

pub struct Collection {
    pub storage: SqliteStorage,
    pub(crate) col_path: PathBuf,
    pub(crate) media_folder: PathBuf,
    pub(crate) media_db: PathBuf,
    pub(crate) tr: I18n,
    pub(crate) state: CollectionState,
}

impl Debug for Collection {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Collection")
            .field("col_path", &self.col_path)
            .finish()
    }
}

impl Collection {
    /// Closes the database, which happens when the connection is dropped.
    pub fn close(self) -> Result<()> {
        Ok(())
    }
}
