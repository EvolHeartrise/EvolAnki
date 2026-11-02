// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

use super::fuzz::minimum_review_fuzz_interval;
use super::interval_kind::IntervalKind;
use super::CardState;
use super::LearnState;
use super::RelearnState;
use super::SchedulingStates;
use super::StateContext;
use crate::card::FsrsMemoryState;
use crate::revlog::RevlogReviewKind;

pub const INITIAL_EASE_FACTOR: f32 = 2.5;
pub const MINIMUM_EASE_FACTOR: f32 = 1.3;
pub const EASE_FACTOR_AGAIN_DELTA: f32 = -0.2;
pub const EASE_FACTOR_HARD_DELTA: f32 = -0.15;
pub const EASE_FACTOR_EASY_DELTA: f32 = 0.15;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReviewState {
    pub scheduled_days: u32,
    pub elapsed_days: u32,
    pub ease_factor: f32,
    pub lapses: u32,
    pub memory_state: Option<FsrsMemoryState>,
}

impl Default for ReviewState {
    fn default() -> Self {
        ReviewState {
            scheduled_days: 0,
            elapsed_days: 0,
            ease_factor: INITIAL_EASE_FACTOR,
            lapses: 0,
            memory_state: None,
        }
    }
}

impl ReviewState {
    pub(crate) fn days_late(&self) -> i32 {
        self.elapsed_days as i32 - self.scheduled_days as i32
    }

    pub(crate) fn interval_kind(self) -> IntervalKind {
        // fixme: maybe use elapsed days in the future? would only
        // make sense for revlog's lastIvl, not for future interval
        IntervalKind::InDays(self.scheduled_days)
    }

    pub(crate) fn revlog_kind(self) -> RevlogReviewKind {
        if self.days_late() < 0 {
            RevlogReviewKind::Filtered
        } else {
            RevlogReviewKind::Review
        }
    }

    pub(crate) fn next_states(self, ctx: &StateContext) -> SchedulingStates {
        let (hard_interval, good_interval, easy_interval) = self.passing_review_intervals(ctx);

        SchedulingStates {
            current: self.into(),
            again: self.answer_again(ctx),
            hard: self.answer_hard(hard_interval, ctx).into(),
            good: self.answer_good(good_interval, ctx).into(),
            easy: self.answer_easy(easy_interval, ctx).into(),
        }
    }

    /// In FSRS, fuzz is applied when the card leaves the relearning stage.
    pub(crate) fn failing_review_interval(ctx: &StateContext) -> (f32, Option<FsrsMemoryState>) {
        let states = &ctx.fsrs_next_states;
        (states.again.interval, Some(states.again.memory.into()))
    }

    fn answer_again(self, ctx: &StateContext) -> CardState {
        let lapses = self.lapses + 1;
        let (scheduled_days, memory_state) = Self::failing_review_interval(ctx);
        let again_review = ReviewState {
            scheduled_days: scheduled_days.round().max(1.0) as u32,
            elapsed_days: 0,
            ease_factor: (self.ease_factor + EASE_FACTOR_AGAIN_DELTA).max(MINIMUM_EASE_FACTOR),
            lapses,
            memory_state,
        };
        let again_relearn = RelearnState {
            learning: LearnState {
                remaining_steps: ctx.relearn_steps.remaining_for_failed(),
                scheduled_secs: (scheduled_days * 86_400.0) as u32,
                elapsed_secs: 0,
                memory_state,
            },
            review: again_review,
        };

        if let Some(again_delay) = ctx.relearn_steps.again_delay_secs_learn() {
            RelearnState {
                learning: LearnState {
                    remaining_steps: ctx.relearn_steps.remaining_for_failed(),
                    scheduled_secs: again_delay,
                    elapsed_secs: 0,
                    memory_state,
                },
                review: again_review,
            }
            .into()
        } else if ctx.fsrs_allow_short_term
            && (ctx.fsrs_short_term_with_steps_enabled || ctx.relearn_steps.is_empty())
            && scheduled_days < 0.5
        {
            again_relearn.into()
        } else {
            again_review.into()
        }
    }

    fn answer_hard(self, scheduled_days: u32, ctx: &StateContext) -> ReviewState {
        ReviewState {
            scheduled_days,
            elapsed_days: 0,
            ease_factor: (self.ease_factor + EASE_FACTOR_HARD_DELTA).max(MINIMUM_EASE_FACTOR),
            memory_state: Some(ctx.fsrs_next_states.hard.memory.into()),
            ..self
        }
    }

    fn answer_good(self, scheduled_days: u32, ctx: &StateContext) -> ReviewState {
        ReviewState {
            scheduled_days,
            elapsed_days: 0,
            memory_state: Some(ctx.fsrs_next_states.good.memory.into()),
            ..self
        }
    }

    fn answer_easy(self, scheduled_days: u32, ctx: &StateContext) -> ReviewState {
        ReviewState {
            scheduled_days,
            elapsed_days: 0,
            ease_factor: self.ease_factor + EASE_FACTOR_EASY_DELTA,
            memory_state: Some(ctx.fsrs_next_states.easy.memory.into()),
            ..self
        }
    }

    /// Return the intervals for hard, good and easy, each of which depends on
    /// the previous.
    fn passing_review_intervals(self, ctx: &StateContext) -> (u32, u32, u32) {
        let states = &ctx.fsrs_next_states;
        let hard = constrain_passing_interval(
            ctx,
            states.hard.interval,
            minimum_review_fuzz_interval(
                states.hard.interval,
                self.scheduled_days,
                ctx.maximum_review_interval,
            )
            .max(1),
        );
        let good = constrain_passing_interval(
            ctx,
            states.good.interval,
            minimum_review_fuzz_interval(
                states.good.interval,
                self.scheduled_days,
                ctx.maximum_review_interval,
            )
            .max(hard + 1),
        );
        let easy = constrain_passing_interval(
            ctx,
            states.easy.interval,
            minimum_review_fuzz_interval(
                states.easy.interval,
                self.scheduled_days,
                ctx.maximum_review_interval,
            )
            .max(good + 1),
        );
        (hard, good, easy)
    }
}

/// Transform the provided hard/good/easy interval.
/// - Apply fuzz.
/// - Ensure it is at least `minimum`, and at least 1.
/// - Ensure it is at or below the configured maximum interval.
fn constrain_passing_interval(ctx: &StateContext, interval: f32, minimum: u32) -> u32 {
    let (minimum, maximum) = ctx.min_and_max_review_intervals(minimum);
    ctx.with_review_fuzz(interval, minimum, maximum)
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::scheduler::states::test_next_states;

    fn review_state(scheduled_days: u32) -> ReviewState {
        ReviewState {
            scheduled_days,
            elapsed_days: scheduled_days,
            ease_factor: 1.3,
            lapses: 0,
            memory_state: None,
        }
    }

    #[test]
    fn low_intervals_stay_ordered_and_maximum_is_respected() {
        let mut ctx = StateContext::defaults_for_testing();
        // FSRS can return next to nothing for all three, and that shouldn't underflow
        ctx.fsrs_next_states = test_next_states(0.1, 0.1, 0.1, 0.1);
        let state = review_state(1);
        ctx.fuzz_factor = Some(0.0);
        assert_eq!(state.passing_review_intervals(&ctx), (1, 2, 3));
        ctx.fuzz_factor = Some(0.99);
        assert_eq!(state.passing_review_intervals(&ctx), (1, 2, 4));

        // maximum must be respected no matter what
        ctx.fsrs_next_states = test_next_states(1.0, 50.0, 100.0, 200.0);
        ctx.maximum_review_interval = 5;
        assert_eq!(state.passing_review_intervals(&ctx), (5, 5, 5));
    }

    #[test]
    fn low_hard_interval_does_not_pull_good_down() {
        let mut ctx = StateContext::defaults_for_testing();
        ctx.fsrs_next_states = test_next_states(1.0, 0.2, 2.0, 3.0);
        let state = review_state(2);
        ctx.fuzz_factor = Some(0.0);
        assert_eq!(state.passing_review_intervals(&ctx), (1, 2, 3));
    }

    #[test]
    fn again_takes_the_interval_and_memory_from_fsrs_without_fuzz() {
        let ctx = StateContext {
            fsrs_next_states: test_next_states(12.4, 20.0, 30.0, 40.0),
            fuzz_factor: Some(0.99),
            ..StateContext::defaults_for_testing()
        };
        let CardState::Normal(crate::scheduler::states::NormalState::Relearning(again)) =
            review_state(50).next_states(&ctx).again
        else {
            panic!("expected relearning");
        };
        assert_eq!(again.review.scheduled_days, 12);
        assert_eq!(again.review.lapses, 1);
        assert!(again.review.memory_state.is_some());
    }

    #[test]
    fn early_review_keeps_its_revlog_kind() {
        let state = ReviewState {
            elapsed_days: 1,
            ..review_state(5)
        };
        assert_eq!(state.revlog_kind(), RevlogReviewKind::Filtered);
        assert_eq!(review_state(5).revlog_kind(), RevlogReviewKind::Review);
    }
}
