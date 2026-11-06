// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html
use anki_proto::generic;

use crate::collection::Collection;
use crate::error;
use crate::prelude::BoolKey;
use crate::prelude::Op;

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

    fn redo(&mut self) -> error::Result<anki_proto::collection::OpChangesAfterUndo> {
        self.redo().map(|out| out.into_protobuf(&self.tr))
    }

    fn set_load_balancer_enabled(
        &mut self,
        input: generic::Bool,
    ) -> error::Result<anki_proto::collection::OpChanges> {
        self.transact(Op::ToggleLoadBalancer, |col| {
            col.set_config(BoolKey::LoadBalancerEnabled, &input.val)?;
            Ok(())
        })
        .map(Into::into)
    }
}
