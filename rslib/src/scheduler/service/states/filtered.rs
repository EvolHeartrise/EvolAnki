// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

use crate::scheduler::states::FilteredState;

impl From<FilteredState> for anki_proto::scheduler::scheduling_state::Filtered {
    fn from(state: FilteredState) -> Self {
        anki_proto::scheduler::scheduling_state::Filtered {
            kind: Some(match state {
                FilteredState::Preview(state) => {
                    anki_proto::scheduler::scheduling_state::filtered::Kind::Preview(state.into())
                }
                FilteredState::Rescheduling(state) => {
                    anki_proto::scheduler::scheduling_state::filtered::Kind::Rescheduling(
                        state.into(),
                    )
                }
            }),
        }
    }
}
