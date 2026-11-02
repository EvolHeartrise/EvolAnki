// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

use super::interval_kind::IntervalKind;
use super::CardState;
use super::LearnState;
use super::ReviewState;
use super::SchedulingStates;
use super::StateContext;
use crate::revlog::RevlogReviewKind;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RelearnState {
    pub learning: LearnState,
    pub review: ReviewState,
}

impl RelearnState {
    pub(crate) fn interval_kind(self) -> IntervalKind {
        self.learning.interval_kind()
    }

    pub(crate) fn revlog_kind(self) -> RevlogReviewKind {
        RevlogReviewKind::Relearning
    }

    pub(crate) fn next_states(self, ctx: &StateContext) -> SchedulingStates {
        SchedulingStates {
            current: self.into(),
            again: self.answer_again(ctx),
            hard: self.answer_hard(ctx),
            good: self.answer_good(ctx),
            easy: self.answer_easy(ctx).into(),
        }
    }

    fn answer_again(self, ctx: &StateContext) -> CardState {
        let (scheduled_days, memory_state) = ReviewState::failing_review_interval(ctx);
        if let Some(again_delay) = ctx.relearn_steps.again_delay_secs_learn() {
            RelearnState {
                learning: LearnState {
                    remaining_steps: ctx.relearn_steps.remaining_for_failed(),
                    scheduled_secs: again_delay,
                    elapsed_secs: 0,
                    memory_state,
                },
                review: ReviewState {
                    scheduled_days: scheduled_days.round().max(1.0) as u32,
                    elapsed_days: 0,
                    memory_state,
                    ..self.review
                },
            }
            .into()
        } else {
            let (minimum, maximum) = ctx.min_and_max_review_intervals(1);
            let interval = ctx.fsrs_next_states.again.interval;
            let again_review = ReviewState {
                scheduled_days: ctx.with_review_fuzz(interval.round().max(1.0), minimum, maximum),
                memory_state,
                ..self.review
            };
            let again_relearn = RelearnState {
                learning: LearnState {
                    remaining_steps: ctx.relearn_steps.remaining_for_failed(),
                    scheduled_secs: (interval * 86_400.0) as u32,
                    elapsed_secs: 0,
                    memory_state,
                },
                review: again_review,
            };
            if ctx.fsrs_allow_short_term
                && (ctx.fsrs_short_term_with_steps_enabled || ctx.relearn_steps.is_empty())
                && interval < 0.5
            {
                again_relearn.into()
            } else {
                again_review.into()
            }
        }
    }

    fn answer_hard(self, ctx: &StateContext) -> CardState {
        let memory_state = Some(ctx.fsrs_next_states.hard.memory.into());
        if let Some(hard_delay) = ctx
            .relearn_steps
            .hard_delay_secs(self.learning.remaining_steps)
        {
            RelearnState {
                learning: LearnState {
                    scheduled_secs: hard_delay,
                    memory_state,
                    ..self.learning
                },
                review: ReviewState {
                    elapsed_days: 0,
                    memory_state,
                    ..self.review
                },
            }
            .into()
        } else {
            let (minimum, maximum) = ctx.min_and_max_review_intervals(1);
            let interval = ctx.fsrs_next_states.hard.interval;
            let hard_review = ReviewState {
                scheduled_days: ctx.with_review_fuzz(interval.round().max(1.0), minimum, maximum),
                memory_state,
                ..self.review
            };
            let hard_relearn = RelearnState {
                learning: LearnState {
                    scheduled_secs: (interval * 86_400.0) as u32,
                    memory_state,
                    ..self.learning
                },
                review: hard_review,
            };
            if ctx.fsrs_allow_short_term
                && (ctx.fsrs_short_term_with_steps_enabled || ctx.relearn_steps.is_empty())
                && interval < 0.5
            {
                hard_relearn.into()
            } else {
                hard_review.into()
            }
        }
    }

    fn answer_good(self, ctx: &StateContext) -> CardState {
        let memory_state = Some(ctx.fsrs_next_states.good.memory.into());
        if let Some(good_delay) = ctx
            .relearn_steps
            .good_delay_secs(self.learning.remaining_steps)
        {
            RelearnState {
                learning: LearnState {
                    scheduled_secs: good_delay,
                    remaining_steps: ctx
                        .relearn_steps
                        .remaining_for_good(self.learning.remaining_steps),
                    elapsed_secs: 0,
                    memory_state,
                },
                review: ReviewState {
                    elapsed_days: 0,
                    memory_state,
                    ..self.review
                },
            }
            .into()
        } else {
            let (minimum, maximum) = ctx.min_and_max_review_intervals(1);
            let interval = ctx.fsrs_next_states.good.interval;
            let good_review = ReviewState {
                scheduled_days: ctx.with_review_fuzz(interval.round().max(1.0), minimum, maximum),
                memory_state,
                ..self.review
            };
            let good_relearn = RelearnState {
                learning: LearnState {
                    scheduled_secs: (interval * 86_400.0) as u32,
                    remaining_steps: ctx
                        .relearn_steps
                        .remaining_for_good(self.learning.remaining_steps),
                    memory_state,
                    ..self.learning
                },
                review: good_review,
            };
            if ctx.fsrs_allow_short_term
                && (ctx.fsrs_short_term_with_steps_enabled || ctx.relearn_steps.is_empty())
                && interval < 0.5
            {
                good_relearn.into()
            } else {
                good_review.into()
            }
        }
    }

    fn answer_easy(self, ctx: &StateContext) -> ReviewState {
        let states = &ctx.fsrs_next_states;
        let (mut minimum, maximum) = ctx.min_and_max_review_intervals(1);
        let good = ctx.with_review_fuzz(states.good.interval, minimum, maximum);
        minimum = good + 1;
        let interval = states.easy.interval;
        ReviewState {
            scheduled_days: ctx.with_review_fuzz(interval.round().max(1.0), minimum, maximum),
            elapsed_days: 0,
            memory_state: Some(states.easy.memory.into()),
            ..self.review
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::steps::LearningSteps;
    use super::*;
    use crate::scheduler::states::test_next_states as fsrs_states;
    use crate::scheduler::states::NormalState;

    // defaults_for_testing: relearn_steps=[10.0min=600s], FSRS intervals of
    // again=1, hard=2, good=3 and easy=7 days, fuzz_factor=None (deterministic)
    //   remaining_steps=1 → only relearn step (10min = 600s)
    //   hard delay for single step = 150% of 600 = 900s
    //   good_delay_secs(1) = None → only 1 step, no next step

    fn relearn_state() -> RelearnState {
        RelearnState {
            learning: LearnState {
                remaining_steps: 1,
                scheduled_secs: 600,
                elapsed_secs: 0,
                memory_state: None,
            },
            review: ReviewState {
                scheduled_days: 3,
                elapsed_days: 3,
                lapses: 1,
                memory_state: None,
            },
        }
    }

    #[test]
    fn again_stays_in_relearning() {
        let ctx = StateContext::defaults_for_testing();
        let state = relearn_state();
        let states = state.next_states(&ctx);
        // Again with active relearn steps → stays in RelearnState
        assert!(matches!(
            states.again,
            CardState::Normal(NormalState::Relearning(_))
        ));
    }

    #[test]
    fn again_takes_the_review_interval_from_fsrs() {
        let ctx = StateContext {
            fsrs_next_states: fsrs_states(2.0, 3.0, 5.0, 7.0),
            ..StateContext::defaults_for_testing()
        };
        let state = relearn_state(); // review.scheduled_days = 3
        let states = state.next_states(&ctx);

        let CardState::Normal(NormalState::Relearning(relearn)) = states.again else {
            panic!(
                "again should produce a RelearnState, got: {:?}",
                states.again
            );
        };

        assert_eq!(
            relearn.review.scheduled_days, 2,
            "the interval should be FSRS's 2, not the 3 from before"
        );
    }

    #[test]
    fn again_with_no_steps_exits_to_review() {
        let ctx = StateContext {
            relearn_steps: LearningSteps::new(&[]),
            ..StateContext::defaults_for_testing()
        };
        let state = relearn_state();
        let states = state.next_states(&ctx);
        // No relearn steps → exit relearning
        assert!(matches!(
            states.again,
            CardState::Normal(NormalState::Review(_))
        ));
    }

    #[test]
    fn hard_stays_in_relearning_and_keeps_same_step() {
        let ctx = StateContext::defaults_for_testing(); // relearn_steps=[10.0], remaining=1
        let state = relearn_state();
        let states = state.next_states(&ctx);
        let CardState::Normal(NormalState::Relearning(r)) = states.hard else {
            panic!("hard should produce a RelearnState, got: {:?}", states.hard);
        };
        assert_eq!(
            r.learning.remaining_steps, 1,
            "hard must keep current step (remaining=1), not reset or advance"
        );
    }

    #[test]
    fn hard_with_no_steps_exits_to_review() {
        let ctx = StateContext {
            relearn_steps: LearningSteps::new(&[]),
            ..StateContext::defaults_for_testing()
        };
        let state = relearn_state();
        let states = state.next_states(&ctx);
        // No relearn steps → exit relearning
        assert!(matches!(
            states.hard,
            CardState::Normal(NormalState::Review(_))
        ));
    }

    #[test]
    fn good_from_last_step_graduates_to_review() {
        let ctx = StateContext::defaults_for_testing();
        let state = relearn_state(); // remaining_steps=1, only 1 relearn step → no next step
        let states = state.next_states(&ctx);
        // Good from last (only) relearn step → graduate to Review
        assert!(matches!(
            states.good,
            CardState::Normal(NormalState::Review(_))
        ));
    }

    #[test]
    fn easy_always_graduates_to_review() {
        let ctx = StateContext::defaults_for_testing();
        let state = relearn_state();
        let states = state.next_states(&ctx);
        assert!(matches!(
            states.easy,
            CardState::Normal(NormalState::Review(_))
        ));
    }

    #[test]
    fn again_fsrs_exits_to_review_with_algorithm_interval() {
        let ctx = StateContext {
            relearn_steps: LearningSteps::new(&[]),
            fsrs_next_states: fsrs_states(2.0, 3.0, 5.0, 7.0),
            ..StateContext::defaults_for_testing()
        };
        let state = relearn_state();
        let states = state.next_states(&ctx);
        // again.interval=2, not the previous scheduled_days=3
        let CardState::Normal(NormalState::Review(r)) = states.again else {
            panic!("expected Review, got: {:?}", states.again);
        };
        assert_eq!(
            r.scheduled_days, 2,
            "FSRS again should use algorithm interval (2d)"
        );
    }

    #[test]
    fn again_fsrs_short_term_stays_in_relearning() {
        let ctx = StateContext {
            relearn_steps: LearningSteps::new(&[]),
            fsrs_next_states: fsrs_states(0.2, 0.3, 0.4, 7.0),
            fsrs_allow_short_term: true,
            ..StateContext::defaults_for_testing()
        };
        let state = relearn_state();
        let states = state.next_states(&ctx);
        // interval=0.2 < 0.5, fsrs_allow_short_term=true, relearn_steps empty
        // → stays in Relearning with scheduled_secs = (0.2 * 86_400.0) as u32 = 17280
        let CardState::Normal(NormalState::Relearning(r)) = states.again else {
            panic!("expected Relearning, got: {:?}", states.again);
        };
        assert_eq!(r.learning.scheduled_secs, 17280);
    }

    #[test]
    fn hard_fsrs_exits_to_review_with_algorithm_interval() {
        let ctx = StateContext {
            relearn_steps: LearningSteps::new(&[]),
            fsrs_next_states: fsrs_states(2.0, 3.0, 5.0, 7.0),
            ..StateContext::defaults_for_testing()
        };
        let state = relearn_state();
        let states = state.next_states(&ctx);
        // hard.interval=3
        let CardState::Normal(NormalState::Review(r)) = states.hard else {
            panic!("expected Review, got: {:?}", states.hard);
        };
        assert_eq!(
            r.scheduled_days, 3,
            "FSRS hard should use algorithm interval (3d)"
        );
    }

    #[test]
    fn hard_fsrs_short_term_stays_in_relearning() {
        let ctx = StateContext {
            relearn_steps: LearningSteps::new(&[]),
            fsrs_next_states: fsrs_states(0.2, 0.3, 0.4, 7.0),
            fsrs_allow_short_term: true,
            ..StateContext::defaults_for_testing()
        };
        let state = relearn_state();
        let states = state.next_states(&ctx);
        // interval=0.3 < 0.5, fsrs_allow_short_term=true, relearn_steps empty
        // → stays in Relearning with scheduled_secs = (0.3 * 86_400.0) as u32 = 25920
        let CardState::Normal(NormalState::Relearning(r)) = states.hard else {
            panic!("expected Relearning, got: {:?}", states.hard);
        };
        assert_eq!(r.learning.scheduled_secs, 25920);
    }

    #[test]
    fn good_fsrs_exits_to_review_with_algorithm_interval() {
        let ctx = StateContext {
            relearn_steps: LearningSteps::new(&[]),
            fsrs_next_states: fsrs_states(2.0, 3.0, 5.0, 7.0),
            ..StateContext::defaults_for_testing()
        };
        let state = relearn_state();
        let states = state.next_states(&ctx);
        // good.interval=5, not the previous scheduled_days=3
        let CardState::Normal(NormalState::Review(r)) = states.good else {
            panic!("expected Review, got: {:?}", states.good);
        };
        assert_eq!(
            r.scheduled_days, 5,
            "FSRS good should use algorithm interval (5d)"
        );
    }

    #[test]
    fn good_fsrs_short_term_stays_in_relearning() {
        let ctx = StateContext {
            relearn_steps: LearningSteps::new(&[]),
            fsrs_next_states: fsrs_states(0.2, 0.3, 0.4, 7.0),
            fsrs_allow_short_term: true,
            ..StateContext::defaults_for_testing()
        };
        let state = relearn_state();
        let states = state.next_states(&ctx);
        // interval=0.4 < 0.5, fsrs_allow_short_term=true, relearn_steps empty
        // → stays in Relearning with scheduled_secs = (0.4 * 86_400.0) as u32 = 34560
        let CardState::Normal(NormalState::Relearning(r)) = states.good else {
            panic!("expected Relearning, got: {:?}", states.good);
        };
        assert_eq!(r.learning.scheduled_secs, 34560);
    }

    #[test]
    fn easy_fsrs_uses_algorithm_interval_not_plus_one() {
        let ctx = StateContext {
            // easy.interval=7: clamped above good(3)+1=4 minimum → 7 days
            fsrs_next_states: fsrs_states(1.0, 2.0, 3.0, 7.0),
            ..StateContext::defaults_for_testing()
        };
        let state = relearn_state(); // review.scheduled_days = 3
        let states = state.next_states(&ctx);
        let CardState::Normal(NormalState::Review(r)) = states.easy else {
            panic!("easy should produce a ReviewState, got: {:?}", states.easy);
        };
        assert_eq!(
            r.scheduled_days, 7,
            "FSRS easy should use algorithm interval (7d)"
        );
    }

    // Multi-step relearning tests.
    // Two relearn steps [5.0, 10.0]min: remaining=2 → first step, remaining=1 →
    // last step.

    fn relearn_state_at_last_of_two_steps() -> RelearnState {
        RelearnState {
            learning: LearnState {
                remaining_steps: 1, // at the last step of two
                scheduled_secs: 600,
                elapsed_secs: 0,
                memory_state: None,
            },
            review: ReviewState {
                scheduled_days: 3,
                elapsed_days: 3,
                lapses: 1,
                memory_state: None,
            },
        }
    }

    #[test]
    fn again_with_two_steps_resets_to_first_step() {
        // Card is at the last step (remaining=1). Again should jump back to the
        // first step (remaining=2), not stay at remaining=1.
        let ctx = StateContext {
            relearn_steps: LearningSteps::new(&[5.0, 10.0]),
            ..StateContext::defaults_for_testing()
        };
        let state = relearn_state_at_last_of_two_steps();
        let states = state.next_states(&ctx);
        let CardState::Normal(NormalState::Relearning(r)) = states.again else {
            panic!(
                "again should produce a RelearnState, got: {:?}",
                states.again
            );
        };
        assert_eq!(
            r.learning.remaining_steps, 2,
            "again must reset to first step (remaining=2), not stay at remaining=1"
        );
    }

    #[test]
    fn hard_with_two_steps_keeps_same_remaining_steps() {
        // Hard should keep the card on the current step without advancing or resetting.
        let ctx = StateContext {
            relearn_steps: LearningSteps::new(&[5.0, 10.0]),
            ..StateContext::defaults_for_testing()
        };
        let state = relearn_state_at_last_of_two_steps(); // remaining=1
        let states = state.next_states(&ctx);
        let CardState::Normal(NormalState::Relearning(r)) = states.hard else {
            panic!("hard should produce a RelearnState, got: {:?}", states.hard);
        };
        assert_eq!(
            r.learning.remaining_steps, 1,
            "hard must keep current step (remaining=1), not reset or advance"
        );
    }

    #[test]
    fn good_from_non_last_step_stays_in_relearning() {
        // Card is at remaining=2 (first of two steps). Good advances to next step
        // but should NOT graduate to Review yet.
        let ctx = StateContext {
            relearn_steps: LearningSteps::new(&[5.0, 10.0]),
            ..StateContext::defaults_for_testing()
        };
        let state = RelearnState {
            learning: LearnState {
                remaining_steps: 2,
                ..relearn_state_at_last_of_two_steps().learning
            },
            ..relearn_state_at_last_of_two_steps()
        };
        let states = state.next_states(&ctx);
        assert!(
            matches!(states.good, CardState::Normal(NormalState::Relearning(_))),
            "good from non-last step must stay in Relearning, got: {:?}",
            states.good
        );
    }

    #[test]
    fn good_from_non_last_step_advances_remaining_steps() {
        // Good must decrease remaining_steps by 1, not reset it.
        let ctx = StateContext {
            relearn_steps: LearningSteps::new(&[5.0, 10.0]),
            ..StateContext::defaults_for_testing()
        };
        let state = RelearnState {
            learning: LearnState {
                remaining_steps: 2,
                ..relearn_state_at_last_of_two_steps().learning
            },
            ..relearn_state_at_last_of_two_steps()
        };
        let states = state.next_states(&ctx);
        let CardState::Normal(NormalState::Relearning(r)) = states.good else {
            panic!("good should produce a RelearnState, got: {:?}", states.good);
        };
        assert_eq!(
            r.learning.remaining_steps, 1,
            "good must advance remaining_steps from 2 to 1, not reset or stay"
        );
    }
}
