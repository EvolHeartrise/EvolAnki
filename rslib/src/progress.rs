// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

use std::marker::PhantomData;
use std::sync::Arc;
use std::sync::Mutex;

use anki_proto::collection::progress::Value;

use crate::dbcheck::DatabaseCheckProgress;
use crate::error::AnkiError;
use crate::error::Result;
use crate::i18n::I18n;
use crate::import_export::ExportProgress;
use crate::import_export::ImportProgress;
use crate::media::check::MediaCheckProgress;
use crate::prelude::Collection;
use crate::scheduler::fsrs::memory_state::ComputeMemoryProgress;
use crate::scheduler::fsrs::params::ComputeParamsProgress;
use crate::scheduler::fsrs::retention::ComputeRetentionProgress;

/// Stores progress state that can be updated cheaply, and will update a
/// Mutex-protected copy that other threads can check, if more than 0.1
/// secs has elapsed since the previous update.
/// If another thread has set the `want_abort` flag on the shared state,
/// then the next non-throttled update will fail with [AnkiError::Interrupted].
/// Automatically updates the shared state on creation, with the default
/// value for the type.
#[derive(Debug, Default)]
pub struct ThrottlingProgressHandler<P: Into<Progress> + Default> {
    pub(crate) state: P,
    shared_state: Arc<Mutex<ProgressState>>,
    last_shared_update: coarsetime::Instant,
}

impl<P: Into<Progress> + Default + Clone> ThrottlingProgressHandler<P> {
    pub(crate) fn new(shared_state: Arc<Mutex<ProgressState>>) -> Self {
        let initial = P::default();
        {
            let mut guard = shared_state.lock().unwrap();
            guard.last_progress = Some(initial.clone().into());
            guard.want_abort = false;
        }
        Self {
            shared_state,
            state: initial,
            ..Default::default()
        }
    }

    /// Overwrite the currently-stored state. This does not throttle, and should
    /// be used when you want to ensure the UI state gets updated, and
    /// ensure that the abort flag is checked between expensive steps.
    pub(crate) fn set(&mut self, progress: P) -> Result<()> {
        self.update(false, |state| *state = progress)
    }

    /// Mutate the currently-stored state, and maybe update shared state.
    pub(crate) fn update(&mut self, throttle: bool, mutator: impl FnOnce(&mut P)) -> Result<()> {
        mutator(&mut self.state);

        let now = coarsetime::Instant::now();
        if throttle && now.duration_since(self.last_shared_update).as_f64() < 0.1 {
            return Ok(());
        }
        self.last_shared_update = now;

        let mut guard = self.shared_state.lock().unwrap();
        guard.last_progress.replace(self.state.clone().into());

        if std::mem::take(&mut guard.want_abort) {
            Err(AnkiError::Interrupted)
        } else {
            Ok(())
        }
    }

    /// An alternative to incrementor() below, that can be used across function
    /// calls easily, as it continues from the previous state.
    pub(crate) fn increment(&mut self, accessor: impl Fn(&mut P) -> &mut usize) -> Result<()> {
        let field = accessor(&mut self.state);
        *field += 1;
        if *field % 17 == 0 {
            self.update(true, |_| ())?;
        }
        Ok(())
    }

    /// Returns an [Incrementor] with an `increment()` function for use in
    /// loops.
    pub(crate) fn incrementor<'inc, 'progress: 'inc, 'map: 'inc>(
        &'progress mut self,
        mut count_map: impl 'map + FnMut(usize) -> P,
    ) -> Incrementor<'inc, impl FnMut(usize) -> Result<()> + 'inc> {
        Incrementor::new(move |u| self.update(true, |p| *p = count_map(u)))
    }

    /// Stopgap for returning a progress fn compliant with the media code.
    pub(crate) fn media_db_fn(
        &mut self,
        count_map: impl 'static + Fn(usize) -> P,
    ) -> Result<impl FnMut(usize) -> bool + '_>
    where
        P: Into<Progress>,
    {
        Ok(move |count| self.update(true, |p| *p = count_map(count)).is_ok())
    }
}

#[derive(Default, Debug)]
pub struct ProgressState {
    pub want_abort: bool,
    pub last_progress: Option<Progress>,
}

impl ProgressState {
    pub fn reset(&mut self) {
        self.want_abort = false;
        self.last_progress = None;
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Progress {
    /// A media or database check, import or export. Nothing displays their
    /// progress, but they can still be aborted.
    Unreported,
    ComputeParams(ComputeParamsProgress),
    ComputeRetention(ComputeRetentionProgress),
    ComputeMemory(ComputeMemoryProgress),
}

pub(crate) fn progress_to_proto(
    progress: Option<Progress>,
    tr: &I18n,
) -> anki_proto::collection::Progress {
    let progress = match progress {
        None | Some(Progress::Unreported) => Value::None(anki_proto::generic::Empty {}),
        Some(Progress::ComputeParams(progress)) => {
            Value::ComputeParams(anki_proto::collection::ComputeParamsProgress {
                current: progress.current_iteration,
                total: progress.total_iterations,
                reviews: progress.reviews,
                current_deck: progress.current_deck,
                total_decks: progress.total_decks,
            })
        }
        Some(Progress::ComputeRetention(progress)) => {
            Value::ComputeRetention(anki_proto::collection::ComputeRetentionProgress {
                current: progress.current,
                total: progress.total,
            })
        }
        Some(Progress::ComputeMemory(progress)) => {
            Value::ComputeMemory(anki_proto::collection::ComputeMemoryProgress {
                current_cards: progress.current_cards,
                total_cards: progress.total_cards,
                label: tr
                    .deck_config_updating_cards(progress.current_cards, progress.total_cards)
                    .into(),
            })
        }
    };
    anki_proto::collection::Progress {
        value: Some(progress),
    }
}

impl From<MediaCheckProgress> for Progress {
    fn from(_: MediaCheckProgress) -> Self {
        Progress::Unreported
    }
}

impl From<DatabaseCheckProgress> for Progress {
    fn from(_: DatabaseCheckProgress) -> Self {
        Progress::Unreported
    }
}

impl From<ImportProgress> for Progress {
    fn from(_: ImportProgress) -> Self {
        Progress::Unreported
    }
}

impl From<ExportProgress> for Progress {
    fn from(_: ExportProgress) -> Self {
        Progress::Unreported
    }
}

impl From<ComputeParamsProgress> for Progress {
    fn from(p: ComputeParamsProgress) -> Self {
        Progress::ComputeParams(p)
    }
}

impl From<ComputeRetentionProgress> for Progress {
    fn from(p: ComputeRetentionProgress) -> Self {
        Progress::ComputeRetention(p)
    }
}

impl From<ComputeMemoryProgress> for Progress {
    fn from(p: ComputeMemoryProgress) -> Self {
        Progress::ComputeMemory(p)
    }
}

impl Collection {
    pub fn new_progress_handler<P: Into<Progress> + Default + Clone>(
        &self,
    ) -> ThrottlingProgressHandler<P> {
        ThrottlingProgressHandler::new(self.state.progress.clone())
    }

    pub(crate) fn clear_progress(&mut self) {
        self.state.progress.lock().unwrap().reset();
    }
}

pub(crate) struct Incrementor<'f, F: 'f + FnMut(usize) -> Result<()>> {
    update_fn: F,
    count: usize,
    update_interval: usize,
    _phantom: PhantomData<&'f ()>,
}

impl<'f, F: 'f + FnMut(usize) -> Result<()>> Incrementor<'f, F> {
    fn new(update_fn: F) -> Self {
        Self {
            update_fn,
            count: 0,
            update_interval: 17,
            _phantom: PhantomData,
        }
    }

    /// Increments the progress counter, periodically triggering an update.
    /// Returns [AnkiError::Interrupted] if the operation should be cancelled.
    pub(crate) fn increment(&mut self) -> Result<()> {
        self.count += 1;
        if self.count % self.update_interval != 0 {
            return Ok(());
        }
        (self.update_fn)(self.count)
    }
}
