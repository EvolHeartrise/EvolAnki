// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

mod filtered;
mod learning;
mod new;
mod normal;
mod preview;
mod relearning;
mod rescheduling;
mod review;

use crate::scheduler::states::CardState;
use crate::scheduler::states::SchedulingStates;

impl From<SchedulingStates> for anki_proto::scheduler::SchedulingStates {
    fn from(choices: SchedulingStates) -> Self {
        anki_proto::scheduler::SchedulingStates {
            current: Some(choices.current.into()),
            again: Some(choices.again.into()),
            hard: Some(choices.hard.into()),
            good: Some(choices.good.into()),
            easy: Some(choices.easy.into()),
        }
    }
}

impl From<CardState> for anki_proto::scheduler::SchedulingState {
    fn from(state: CardState) -> Self {
        anki_proto::scheduler::SchedulingState {
            kind: Some(match state {
                CardState::Normal(state) => {
                    anki_proto::scheduler::scheduling_state::Kind::Normal(state.into())
                }
                CardState::Filtered(state) => {
                    anki_proto::scheduler::scheduling_state::Kind::Filtered(state.into())
                }
            }),
        }
    }
}
