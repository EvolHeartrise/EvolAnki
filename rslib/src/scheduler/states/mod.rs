// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

pub(crate) mod filtered;
pub(crate) mod fuzz;
pub(crate) mod interval_kind;
pub(crate) mod learning;
pub(crate) mod load_balancer;
pub(crate) mod new;
pub(crate) mod normal;
pub(crate) mod preview_filter;
pub(crate) mod relearning;
pub(crate) mod rescheduling_filter;
pub(crate) mod review;
pub(crate) mod steps;

pub use filtered::FilteredState;
use fsrs::NextStates;
pub(crate) use interval_kind::IntervalKind;
pub use learning::LearnState;
use load_balancer::LoadBalancerContext;
pub use new::NewState;
pub use normal::NormalState;
pub use preview_filter::PreviewState;
pub use relearning::RelearnState;
pub use rescheduling_filter::ReschedulingFilterState;
pub use review::ReviewState;

use self::steps::LearningSteps;
use crate::revlog::RevlogReviewKind;
use crate::scheduler::answering::PreviewDelays;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CardState {
    Normal(NormalState),
    Filtered(FilteredState),
}

impl CardState {
    pub(crate) fn interval_kind(self) -> IntervalKind {
        match self {
            CardState::Normal(normal) => normal.interval_kind(),
            CardState::Filtered(filtered) => filtered.interval_kind(),
        }
    }

    pub(crate) fn revlog_kind(self) -> RevlogReviewKind {
        match self {
            CardState::Normal(normal) => normal.revlog_kind(),
            CardState::Filtered(filtered) => filtered.revlog_kind(),
        }
    }

    pub(crate) fn next_states(self, ctx: &StateContext) -> SchedulingStates {
        match self {
            CardState::Normal(state) => state.next_states(ctx),
            CardState::Filtered(state) => state.next_states(ctx),
        }
    }

    /// Returns the position if it's a [NewState].
    pub(super) fn new_position(&self) -> Option<u32> {
        match self {
            Self::Normal(NormalState::New(NewState { position }))
            | Self::Filtered(FilteredState::Rescheduling(ReschedulingFilterState {
                original_state: NormalState::New(NewState { position }),
            })) => Some(*position),
            _ => None,
        }
    }
}

/// Info required during state transitions.
pub(crate) struct StateContext<'a> {
    /// In range `0.0..1.0`. Used to pick the final interval from the fuzz
    /// range.
    pub fuzz_factor: Option<f32>,
    pub fsrs_next_states: NextStates,
    pub fsrs_short_term_with_steps_enabled: bool,
    pub fsrs_allow_short_term: bool,
    // learning
    pub steps: LearningSteps<'a>,
    pub initial_ease_factor: f32,

    // reviewing
    pub maximum_review_interval: u32,
    pub load_balancer_ctx: Option<LoadBalancerContext<'a>>,

    // relearning
    pub relearn_steps: LearningSteps<'a>,

    // filtered
    pub in_filtered_deck: bool,
    pub preview_delays: PreviewDelays,
}

impl StateContext<'_> {
    /// Return the minimum and maximum review intervals.
    /// - `maximum` is `self.maximum_review_interval`, but at least 1.
    /// - `minimum` is as passed, but at least 1, and at most `maximum`.
    pub(crate) fn min_and_max_review_intervals(&self, minimum: u32) -> (u32, u32) {
        let maximum = self.maximum_review_interval.max(1);
        let minimum = minimum.clamp(1, maximum);
        (minimum, maximum)
    }

    #[cfg(test)]
    pub(crate) fn defaults_for_testing() -> Self {
        Self {
            fuzz_factor: None,
            steps: LearningSteps::new(&[1.0, 10.0]),
            initial_ease_factor: 2.5,
            maximum_review_interval: 36500,
            load_balancer_ctx: None,
            relearn_steps: LearningSteps::new(&[10.0]),
            in_filtered_deck: false,
            preview_delays: PreviewDelays {
                again: 1,
                hard: 10,
                good: 0,
            },
            fsrs_next_states: test_next_states(1.0, 2.0, 3.0, 7.0),
            fsrs_short_term_with_steps_enabled: false,
            fsrs_allow_short_term: false,
        }
    }
}

/// What FSRS would return, with the given intervals in days.
#[cfg(test)]
pub(crate) fn test_next_states(again: f32, hard: f32, good: f32, easy: f32) -> NextStates {
    let item = |interval| fsrs::ItemState {
        memory: fsrs::MemoryState {
            stability: 4.0,
            difficulty: 5.0,
        },
        interval,
    };
    NextStates {
        again: item(again),
        hard: item(hard),
        good: item(good),
        easy: item(easy),
    }
}

#[derive(Debug, Clone)]
pub struct SchedulingStates {
    pub current: CardState,
    pub again: CardState,
    pub hard: CardState,
    pub good: CardState,
    pub easy: CardState,
}

impl From<NewState> for CardState {
    fn from(state: NewState) -> Self {
        CardState::Normal(state.into())
    }
}

impl From<ReviewState> for CardState {
    fn from(state: ReviewState) -> Self {
        CardState::Normal(state.into())
    }
}

impl From<LearnState> for CardState {
    fn from(state: LearnState) -> Self {
        CardState::Normal(state.into())
    }
}

impl From<RelearnState> for CardState {
    fn from(state: RelearnState) -> Self {
        CardState::Normal(state.into())
    }
}

impl From<NormalState> for CardState {
    fn from(state: NormalState) -> Self {
        CardState::Normal(state)
    }
}

impl From<PreviewState> for CardState {
    fn from(state: PreviewState) -> Self {
        CardState::Filtered(FilteredState::Preview(state))
    }
}

impl From<ReschedulingFilterState> for CardState {
    fn from(state: ReschedulingFilterState) -> Self {
        CardState::Filtered(FilteredState::Rescheduling(state))
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn min_and_max_review_intervals() {
        let mut ctx = StateContext::defaults_for_testing();
        ctx.maximum_review_interval = 0;
        assert_eq!(ctx.min_and_max_review_intervals(0), (1, 1));
        assert_eq!(ctx.min_and_max_review_intervals(2), (1, 1));
        ctx.maximum_review_interval = 3;
        assert_eq!(ctx.min_and_max_review_intervals(0), (1, 3));
        assert_eq!(ctx.min_and_max_review_intervals(2), (2, 3));
        assert_eq!(ctx.min_and_max_review_intervals(4), (3, 3));
    }
}
