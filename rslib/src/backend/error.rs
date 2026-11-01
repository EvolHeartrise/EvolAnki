// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

use anki_proto::backend::backend_error::Kind;

use crate::error::AnkiError;
use crate::prelude::*;

impl AnkiError {
    pub fn into_protobuf(self, tr: &I18n) -> anki_proto::backend::BackendError {
        let message = self.message(tr);
        let context = self.context();
        let backtrace = self.backtrace();
        let kind = match self {
            AnkiError::InvalidInput { .. } => Kind::InvalidInput,
            AnkiError::TemplateError { .. } => Kind::TemplateParse,
            AnkiError::DbError { .. } => Kind::DbError,
            AnkiError::Interrupted => Kind::Interrupted,
            AnkiError::CollectionNotOpen => Kind::InvalidInput,
            AnkiError::CollectionAlreadyOpen => Kind::InvalidInput,
            AnkiError::JsonError { .. } => Kind::JsonError,
            AnkiError::ProtoError { .. } => Kind::ProtoError,
            AnkiError::NotFound { .. } => Kind::NotFoundError,
            AnkiError::Deleted => Kind::Deleted,
            AnkiError::Existing => Kind::Exists,
            AnkiError::FilteredDeckError { .. } => Kind::FilteredDeckError,
            AnkiError::SearchError { .. } => Kind::SearchError,
            AnkiError::CardTypeError { .. } => Kind::CardTypeError,
            AnkiError::ParseNumError => Kind::InvalidInput,
            AnkiError::InvalidRegex { .. } => Kind::InvalidInput,
            AnkiError::UndoEmpty => Kind::UndoEmpty,
            AnkiError::MultipleNotetypesSelected => Kind::InvalidInput,
            AnkiError::DatabaseCheckRequired => Kind::InvalidInput,
            AnkiError::CustomStudyError { .. } => Kind::CustomStudyError,
            AnkiError::ImportError { .. } => Kind::ImportError,
            AnkiError::FileIoError { .. } => Kind::IoError,
            AnkiError::MediaCheckRequired => Kind::InvalidInput,
            AnkiError::InvalidId => Kind::InvalidInput,
            AnkiError::InvalidMethodIndex
            | AnkiError::InvalidServiceIndex
            | AnkiError::FsrsParamsInvalid
            | AnkiError::FsrsUnableToDetermineDesiredRetention
            | AnkiError::FsrsInsufficientData => Kind::InvalidInput,
            AnkiError::SchedulerUpgradeRequired => Kind::SchedulerUpgradeRequired,
            AnkiError::FsrsInsufficientReviews { .. } => Kind::InvalidInput,
            AnkiError::ZipError { .. } => Kind::IoError,
            AnkiError::InvalidChecksum { .. } => Kind::InvalidChecksum,
        };

        anki_proto::backend::BackendError {
            kind: kind as i32,
            message,
            context,
            backtrace,
        }
    }
}
