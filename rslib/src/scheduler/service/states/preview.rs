// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

use crate::scheduler::states::PreviewState;

impl From<PreviewState> for anki_proto::scheduler::scheduling_state::Preview {
    fn from(state: PreviewState) -> Self {
        anki_proto::scheduler::scheduling_state::Preview {
            scheduled_secs: state.scheduled_secs,
            finished: state.finished,
        }
    }
}
