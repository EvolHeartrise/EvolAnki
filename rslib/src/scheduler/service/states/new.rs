// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

use crate::scheduler::states::NewState;

impl From<NewState> for anki_proto::scheduler::scheduling_state::New {
    fn from(state: NewState) -> Self {
        anki_proto::scheduler::scheduling_state::New {
            position: state.position,
        }
    }
}
