// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

mod collection;
mod config;
mod import_export;
mod ops;

use std::ops::Deref;
use std::sync::Arc;
use std::sync::Mutex;

use crate::prelude::*;
use crate::progress::Progress;
use crate::progress::ProgressState;
use crate::progress::ThrottlingProgressHandler;

#[derive(Clone)]
#[repr(transparent)]
pub struct Backend(Arc<BackendInner>);

impl Deref for Backend {
    type Target = BackendInner;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub struct BackendInner {
    col: Mutex<Option<Collection>>,
    pub(crate) tr: I18n,
    progress_state: Arc<Mutex<ProgressState>>,
}

impl Backend {
    pub fn new(tr: I18n) -> Backend {
        Backend(Arc::new(BackendInner {
            col: Mutex::new(None),
            tr,
            progress_state: Arc::new(Mutex::new(ProgressState {
                want_abort: false,
                last_progress: None,
            })),
        }))
    }

    pub fn i18n(&self) -> &I18n {
        &self.tr
    }

    /// If collection is open, run the provided closure while holding
    /// the mutex.
    /// If collection is not open, return an error.
    pub(crate) fn with_col<F, T>(&self, func: F) -> Result<T>
    where
        F: FnOnce(&mut Collection) -> Result<T>,
    {
        func(
            self.col
                .lock()
                .unwrap()
                .as_mut()
                .ok_or(AnkiError::CollectionNotOpen)?,
        )
    }

    /// Run a closure with exclusive use of the open collection, for what isn't
    /// an RPC: evolanki-server's backups. Waits for any running operation.
    pub fn with_collection<F, T>(&self, func: F) -> Result<T>
    where
        F: FnOnce(&mut Collection) -> Result<T>,
    {
        self.with_col(func)
    }

    /// Useful for operations that function with a closed collection, such as
    /// a colpkg import. For collection operations, you can use
    /// [Collection::new_progress_handler] instead.
    pub(crate) fn new_progress_handler<P: Into<Progress> + Default + Clone>(
        &self,
    ) -> ThrottlingProgressHandler<P> {
        ThrottlingProgressHandler::new(self.progress_state.clone())
    }
}
