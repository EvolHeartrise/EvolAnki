// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html
use crate::collection::Collection;
use crate::error;

impl crate::services::CollectionService for Collection {
    fn check_database(&mut self) -> error::Result<anki_proto::collection::CheckDatabaseResponse> {
        {
            self.check_database()
                .map(|problems| anki_proto::collection::CheckDatabaseResponse {
                    problems: problems.to_i18n_strings(&self.tr),
                })
        }
    }

    fn get_undo_status(&mut self) -> error::Result<anki_proto::collection::UndoStatus> {
        Ok(self.undo_status().into_protobuf(&self.tr))
    }

    fn undo(&mut self) -> error::Result<anki_proto::collection::OpChangesAfterUndo> {
        self.undo().map(|out| out.into_protobuf(&self.tr))
    }
}
