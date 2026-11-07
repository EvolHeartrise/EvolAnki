// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

use crate::scheduler::states::RelearnState;

impl From<RelearnState> for anki_proto::scheduler::scheduling_state::Relearning {
    fn from(state: RelearnState) -> Self {
        anki_proto::scheduler::scheduling_state::Relearning {
            review: Some(state.review.into()),
            learning: Some(state.learning.into()),
        }
    }
}
