// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

// The text shown to users. EvolAnki is English-only, so these are plain
// strings, used as `import * as tr from "@tslib/tr"` and `tr.someString()`.

const numberFormat = new Intl.NumberFormat("en-US", { maximumFractionDigits: 2 });

/** At most two decimal places, with digit grouping, as in "1,234.5". */
function formatNumber(n: number): string {
    return numberFormat.format(n);
}

function formatArg(arg: number | string): string {
    return typeof arg === "number" ? formatNumber(arg) : arg;
}

/** Whether English uses the singular, as in "1 card". */
function isOne(n: number): boolean {
    return Math.abs(Math.round(n * 100)) === 100;
}

export function actionsCancel(): string {
    return "Cancel";
}

export function actionsDecks(): string {
    return "Decks";
}

export function actionsDecrementValue(): string {
    return "Decrement value";
}

export function actionsIncrementValue(): string {
    return "Increment value";
}

export function actionsNew(): string {
    return "New";
}

export function actionsOptions(): string {
    return "Options";
}

export function actionsReplayAudio(): string {
    return "Replay Audio";
}

export function actionsAutoAdvance(): string {
    return "Auto Advance";
}

export function actionsProcessing(): string {
    return "Processing...";
}

export function deckConfigDailyLimits(): string {
    return "Daily Limits";
}

export function deckConfigNewLimitTooltip(): string {
    return "The maximum number of new cards to introduce in a day, if new cards are available.\nBecause new material will increase your short-term review workload, this should typically\nbe at least 10x smaller than your review limit.";
}

export function deckConfigReviewLimitTooltip(): string {
    return "The maximum number of review cards to show in a day,\nif cards are ready for review.";
}

export function deckConfigLimitDeckV3(): string {
    return "When studying a deck that has subdecks inside it, the limits set on each\nsubdeck control the maximum number of cards gathered from that particular deck.\nThe selected deck's limits control the total cards that will be shown.";
}

export function deckConfigLimitInterdayBoundByReviews(): string {
    return "The review limit also affects interday learning cards. When applying the limit,\ninterday learning cards are gathered first, then review cards.";
}

export function deckConfigTabDescription(): string {
    return "- `Every day`: The limit for this deck.\n- `Today only`: Make a temporary change to this deck's limit.";
}

export function deckConfigNewCardsIgnoreReviewLimit(): string {
    return "New cards ignore review limit";
}

export function deckConfigNewCardsIgnoreReviewLimitTooltip(): string {
    return "By default, the review limit also applies to new cards, and no new cards will be\nshown when the review limit has been reached. If this option is enabled, new cards\nwill be shown regardless of the review limit.";
}

export function deckConfigApplyAllParentLimits(): string {
    return "Limits start from top";
}

export function deckConfigApplyAllParentLimitsTooltip(): string {
    return "By default, the daily limits of a higher-level deck do not apply if you're studying from its subdeck.\nIf this option is enabled, the limits will\nstart from the top-level deck instead, which can be useful if you wish to study individual\nsubdecks, while enforcing a total limit on cards for the deck tree.";
}

export function deckConfigAffectsEntireCollection(): string {
    return "Affects the entire collection.";
}

export function deckConfigEveryDay(): string {
    return "Every day";
}

export function deckConfigTodayOnly(): string {
    return "Today only";
}

export function deckConfigLearningSteps(): string {
    return "Learning steps";
}

export function deckConfigLearningStepsTooltip(): string {
    return "One or more delays, separated by spaces. The first delay will be used\nwhen you press the `Again` button on a new card, and is 1 minute by default.\nThe `Good` button will advance to the next step, which is 10 minutes by default.\nOnce all steps have been passed, the card will become a review card, and\nwill appear on a different day. Delays are typically minutes (e.g. `1m`) or days (e.g. `2d`), but hours (e.g. `1h`) and seconds (e.g. `30s`) are also supported.";
}

export function deckConfigNewInsertionOrder(): string {
    return "Insertion order";
}

export function deckConfigNewInsertionOrderTooltip(): string {
    return "Controls the position (due #) new cards are assigned when you add new cards.\nCards with a lower due number will be shown first when studying. Changing\nthis option will automatically update the existing position of new cards.";
}

export function deckConfigNewInsertionOrderSequential(): string {
    return "Sequential (oldest cards first)";
}

export function deckConfigNewInsertionOrderRandom(): string {
    return "Random";
}

export function deckConfigNewInsertionOrderRandomWithV3(): string {
    return "With the v3 scheduler, it is better to leave this set to sequential, and\nadjust the new card gather order instead.";
}

export function deckConfigRelearningSteps(): string {
    return "Relearning steps";
}

export function deckConfigRelearningStepsTooltip(): string {
    return "Zero or more delays, separated by spaces. By default, pressing the `Again`\nbutton on a review card will show it again 10 minutes later. If no delays\nare provided, the card will have its interval changed, without entering\nrelearning. Delays are typically minutes (e.g. `1m`) or days (e.g. `2d`), but hours (e.g. `1h`) and seconds (e.g. `30s`) are also supported.";
}

export function deckConfigOrderingTitle(): string {
    return "Display Order";
}

export function deckConfigNewGatherPriority(): string {
    return "New card gather order";
}

export function deckConfigNewGatherPriorityTooltip3(): string {
    return "`Deck`: Gathers cards from each subdeck in order, starting from the top. Cards from each subdeck are\ngathered in ascending position. If the daily limit of the selected deck is reached, gathering\ncan stop before all subdecks have been checked. This order is fastest in large collections, and\nallows you to prioritize subdecks that are closer to the top.\n\n`Deck, then random notes`: Like `Random notes`, but goes through subdecks in order,\nas `Deck` does, picking notes at random within each subdeck and gathering all of their cards.\n\n`Ascending position`: Gathers cards by ascending position (due #), which is typically\nthe oldest-added first.\n\n`Descending position`: Gathers cards by descending position (due #), which is typically\nthe latest-added first.\n\n`Random notes`: Picks notes at random, then gathers all of its cards.\n\n`Random cards`: Gathers cards in a random order.";
}

export function deckConfigNewCardSortOrder(): string {
    return "New card sort order";
}

export function deckConfigNewCardSortOrderTooltip2(): string {
    return "`Card type, then order gathered`: Shows cards in order of card type number.\nCards of each card type number are shown in the order they were gathered. \nThis will ensure all front→back cards are seen before any back→front cards.\nThis is useful to have all cards of the same note shown in the same session, but not\ntoo close to one another.\n\n`Order gathered`: Shows cards exactly as they were gathered. This will typically result in all cards of a note being seen one after the other.\n\n`Card type, then random`: Shows cards in order of card type number. Cards of each card\ntype number are shown in a random order. This order is useful if you don't want sibling cards\nto appear too close to each other, but still want the cards to appear in a random order.\n\n`Random note, then card type`: Picks notes at random, then shows all of its cards\nin order.\n\n`Random`: Shows cards in a random order.";
}

export function deckConfigNewReviewPriority(): string {
    return "New/review order";
}

export function deckConfigNewReviewPriorityTooltip(): string {
    return "When to show new cards in relation to review cards.";
}

export function deckConfigInterdayStepPriority(): string {
    return "Interday learning/review order";
}

export function deckConfigInterdayStepPriorityTooltip(): string {
    return "When to show (re)learning cards that cross a day boundary.\n\nThe review limit is always applied first to interday learning cards, and\nthen review cards. This option will control the order the gathered cards are shown in,\nbut interday learning cards will always be gathered first.";
}

export function deckConfigReviewSortOrder(): string {
    return "Review sort order";
}

export function deckConfigReviewSortOrderTooltip(): string {
    return "The default order prioritizes cards that have been waiting longest, so that\nif you have a backlog of reviews, the longest-waiting ones will appear\nfirst. If you have a large backlog that will take more than a few days to\nclear, or wish to see cards in subdeck order, you may find the alternate\nsort orders preferable.";
}

export function deckConfigDisplayOrderWillUseCurrentDeck(): string {
    return "Anki will use the display order from the deck you \nselect to study, and not any subdecks it may have.";
}

export function deckConfigNewGatherPriorityDeck(): string {
    return "Deck";
}

export function deckConfigNewGatherPriorityDeckThenRandomNotes(): string {
    return "Deck, then random notes";
}

export function deckConfigNewGatherPriorityPositionLowestFirst(): string {
    return "Ascending position";
}

export function deckConfigNewGatherPriorityPositionHighestFirst(): string {
    return "Descending position";
}

export function deckConfigNewGatherPriorityRandomNotes(): string {
    return "Random notes";
}

export function deckConfigNewGatherPriorityRandomCards(): string {
    return "Random cards";
}

export function deckConfigSortOrderCardTemplateThenRandom(): string {
    return "Card type, then random";
}

export function deckConfigSortOrderRandomNoteThenTemplate(): string {
    return "Random note, then card type";
}

export function deckConfigSortOrderRandom(): string {
    return "Random";
}

export function deckConfigSortOrderTemplateThenGather(): string {
    return "Card type, then order gathered";
}

export function deckConfigSortOrderGather(): string {
    return "Order gathered";
}

export function deckConfigReviewMixMixWithReviews(): string {
    return "Mix with reviews";
}

export function deckConfigReviewMixShowAfterReviews(): string {
    return "Show after reviews";
}

export function deckConfigReviewMixShowBeforeReviews(): string {
    return "Show before reviews";
}

export function deckConfigSortOrderDueDateThenRandom(): string {
    return "Due date, then random";
}

export function deckConfigSortOrderDueDateThenDeck(): string {
    return "Due date, then deck";
}

export function deckConfigSortOrderDeckThenDueDate(): string {
    return "Deck, then due date";
}

export function deckConfigSortOrderAscendingIntervals(): string {
    return "Ascending intervals";
}

export function deckConfigSortOrderDescendingIntervals(): string {
    return "Descending intervals";
}

export function deckConfigSortOrderAscendingDifficulty(): string {
    return "Easy cards first";
}

export function deckConfigSortOrderDescendingDifficulty(): string {
    return "Difficult cards first";
}

export function deckConfigSortOrderRetrievabilityAscending(): string {
    return "Ascending retrievability";
}

export function deckConfigSortOrderRetrievabilityDescending(): string {
    return "Descending retrievability";
}

export function deckConfigTimerTitle(): string {
    return "Timers";
}

export function deckConfigMaximumAnswerSecs(): string {
    return "Maximum answer seconds";
}

export function deckConfigMaximumAnswerSecsTooltip(): string {
    return "The maximum number of seconds to record for a single review. If an answer\nexceeds this time (because you stepped away from the screen for example),\nthe time taken will be recorded as the limit you have set.";
}

export function deckConfigShowAnswerTimerTooltip(): string {
    return "On the Study screen, show a timer that counts the time you're\ntaking to study each card.";
}

export function deckConfigStopTimerOnAnswer(): string {
    return "Stop on-screen timer on answer";
}

export function deckConfigStopTimerOnAnswerTooltip(): string {
    return "Whether to stop the on-screen timer when the answer is revealed.\nThis doesn't affect statistics.";
}

export function deckConfigSecondsToShowQuestion(): string {
    return "Seconds to show question for";
}

export function deckConfigSecondsToShowQuestionTooltip3(): string {
    return "When auto advance is activated, the number of seconds to wait before applying the question action. Set to 0 to disable.";
}

export function deckConfigSecondsToShowAnswer(): string {
    return "Seconds to show answer for";
}

export function deckConfigSecondsToShowAnswerTooltip2(): string {
    return "When auto advance is activated, the number of seconds to wait before applying the answer action. Set to 0 to disable.";
}

export function deckConfigQuestionActionShowAnswer(): string {
    return "Show Answer";
}

export function deckConfigQuestionActionShowReminder(): string {
    return "Show Reminder";
}

export function deckConfigQuestionAction(): string {
    return "Question action";
}

export function deckConfigQuestionActionToolTip(): string {
    return "The action to perform after the question is shown, and time has elapsed.";
}

export function deckConfigAnswerAction(): string {
    return "Answer action";
}

export function deckConfigAnswerActionTooltip2(): string {
    return "The action to perform after the answer is shown, and time has elapsed.";
}

export function deckConfigWaitForAudioTooltip2(): string {
    return "Wait for audio to finish before automatically applying the question action or answer action.";
}

export function deckConfigAudioTitle(): string {
    return "Audio";
}

export function deckConfigDisableAutoplay(): string {
    return "Don't play audio automatically";
}

export function deckConfigDisableAutoplayTooltip(): string {
    return "When enabled, Anki will not play audio automatically.\nIt can be played manually by clicking/tapping on an audio icon, or by using the Replay action.";
}

export function deckConfigSkipQuestionWhenReplaying(): string {
    return "Skip question when replaying answer";
}

export function deckConfigAlwaysIncludeQuestionAudioTooltip(): string {
    return "Whether the question audio should be included when the Replay action is\nused while looking at the answer side of a card.";
}

export function deckConfigAdvancedTitle(): string {
    return "Advanced";
}

export function deckConfigMaximumIntervalTooltip(): string {
    return "The maximum number of days a review card will wait. When reviews have\nreached the limit, `Hard`, `Good` and `Easy` will all give the same delay.\nThe shorter you set this, the greater your workload will be.";
}

export function deckConfigSaveButton(): string {
    return "Save";
}

export function deckConfigSaveAndOptimize(): string {
    return "Optimize All Decks";
}

export function deckConfigRevertButtonTooltip(): string {
    return "Restore this setting to its default value?";
}

export function deckConfigReviewsTooLow({ cards, expected }: { cards: number; expected: number | string }): string {
    return isOne(cards)
        ? `If adding ${formatNumber(cards)} new card each day, your review limit should be at least ${
            formatArg(expected)
        }.`
        : `If adding ${formatNumber(cards)} new cards each day, your review limit should be at least ${
            formatArg(expected)
        }.`;
}

export function deckConfigMaximumAnswerSecsAboveRecommended(): string {
    return "Anki can schedule your reviews more efficiently when you keep each question short.";
}

export function deckConfigTooShortMaximumInterval(): string {
    return "A maximum interval less than 6 months is not recommended.";
}

export function deckConfigIgnoreBeforeInfo({ included, totalCards }: { included: string; totalCards: string }): string {
    return `(Approximately) ${included}/${totalCards} cards will be used to optimize the FSRS parameters.`;
}

export function deckConfigInvalidParameters(): string {
    return "The provided FSRS parameters are invalid. Leave them blank to use the default values.";
}

export function deckConfigPlaceholderParameters(): string {
    return "Default parameters\n(Press \"Optimize This Deck\" periodically to allow FSRS to better adjust to your memory)";
}

export function deckConfigManualParameterEditWarning(): string {
    return "The parameters should only be modified using the optimize button. Manually editing them is heavily advised against.";
}

export function deckConfigMustHave400Reviews({ count }: { count: number }): string {
    return isOne(count)
        ? `Only ${formatNumber(count)} review was found. You must have at least 400 reviews for this operation.`
        : `Only ${formatNumber(count)} reviews were found. You must have at least 400 reviews for this operation.`;
}

export function deckConfigWeights(): string {
    return "FSRS parameters";
}

export function deckConfigOptimizeButton(): string {
    return "Optimize This Deck";
}

export function deckConfigSlowSuffix({ text }: { text: string }): string {
    return `${text} (slow)`;
}

export function deckConfigComputeButton(): string {
    return "Compute";
}

export function deckConfigIgnoreBefore(): string {
    return "Ignore cards reviewed before";
}

export function deckConfigTimeToOptimize(): string {
    return "It's been a while - using the Optimize All Decks button is recommended.";
}

export function deckConfigEvaluateButton(): string {
    return "Evaluate";
}

export function deckConfigDesiredRetention(): string {
    return "Desired retention";
}

export function deckConfigHistoricalRetention(): string {
    return "Historical retention";
}

export function deckConfigSmallerIsBetter(): string {
    return "Smaller numbers indicate a better fit to your review history.";
}

export function deckConfigStepsTooLargeForFsrs(): string {
    return "Steps of 1 day or more are not recommended.";
}

export function deckConfigIterations({ count }: { count: number }): string {
    return `Iteration: ${formatNumber(count)}...`;
}

export function deckConfigRescheduleCardsOnChange(): string {
    return "Reschedule cards on change";
}

export function deckConfigFsrsTooltip(): string {
    return "Affects the entire collection.\n\nThe Free Spaced Repetition Scheduler (FSRS) decides when you see each card.\nBy more accurately determining how likely you are to forget a card, it can help you remember\nmore material in the same amount of time.";
}

export function deckConfigDesiredRetentionTooltip(): string {
    return "By default, Anki schedules cards so that you have a 90% chance of remembering them when\nthey come up for review again. If you increase this value, Anki will show cards more frequently\nto increase the chances of you remembering them. If you decrease the value, Anki will show cards\nless frequently, and you will forget more of them. Be conservative when adjusting this - higher\nvalues will greatly increase your workload, and lower values can be demoralizing when you forget\na lot of material.";
}

export function deckConfigDesiredRetentionTooltip2(): string {
    return "The workload values provided by the info box are a rough approximation. For a greater level of accuracy, use the simulator.";
}

export function deckConfigHistoricalRetentionTooltip(): string {
    return "When some of your review history is missing, FSRS needs to fill in the gaps. By default, it will\nassume that when you did those old reviews, you remembered 90% of the material. If your old retention\nwas appreciably higher or lower than 90%, adjusting this option will allow FSRS to better approximate\nthe missing reviews.\n\nYour review history may be incomplete for two reasons:\n1. Because you're using the 'ignore cards reviewed before' option.\n2. Because you previously deleted review logs to free up space, or imported material from a different\nSRS program.\n\nThe latter is quite rare, so unless you're using the former option, you probably don't need to adjust\nthis option.";
}

export function deckConfigWeightsTooltip2(): string {
    return "FSRS parameters affect how cards are scheduled. Anki will start with default parameters. You can use \nthe option below to optimize the parameters to best match your performance in this deck.";
}

export function deckConfigRescheduleCardsOnChangeTooltip(): string {
    return "Affects the entire collection, and is not saved.\n\nThis option controls whether the due dates of cards will be changed when you optimize\nthe parameters. The default is not to reschedule cards: future reviews will use the new scheduling, but\nthere will be no immediate change to your workload. If rescheduling is enabled, the due dates of cards\nwill be changed.";
}

export function deckConfigRescheduleCardsWarning(): string {
    return "Depending on your desired retention, this can result in a large number of cards becoming\ndue.\n\nUse this option sparingly, as it will add a review entry to each of your cards, and\nincrease the size of your collection.";
}

export function deckConfigIgnoreBeforeTooltip2(): string {
    return "If set, cards reviewed before the provided date will be ignored when optimizing FSRS parameters.\nThis can be useful if you imported someone else's scheduling data, or have changed the way you use the answer buttons.";
}

export function deckConfigComputeOptimalWeightsTooltip2(): string {
    return "When you click the Optimize button, FSRS will analyze your review history, and generate parameters that are \noptimal for your memory and the content you're studying. Each deck has its own parameters.\nYou don't need to optimize your parameters frequently - once every few months is sufficient.\n\nBy default, parameters will be calculated from the review history of this deck and its subdecks. With too few\nreviews, a deck keeps the parameters it has. You can\noptionally adjust the search before calculating the parameters, if you'd like to alter which cards are used for\noptimizing the parameters.";
}

export function deckConfigWorkloadFactorChange({ factor, previousDr }: { factor: string; previousDr: string }): string {
    return `Approximate workload: ${factor}x\n(compared to ${previousDr}% desired retention)`;
}

export function deckConfigWorkloadFactorUnchanged(): string {
    return "The higher your desired retention, the more frequently cards will be shown to you.";
}

export function deckConfigDesiredRetentionTooLow(): string {
    return "Your desired retention is very low, which can lead to very long intervals.";
}

export function deckConfigDesiredRetentionTooHigh(): string {
    return "Your desired retention is very high, which can lead to very short intervals.";
}

export function deckConfigPercentOfReviews({ reviews, pct }: { reviews: number; pct: string }): string {
    return isOne(reviews)
        ? `${pct}% of ${formatNumber(reviews)} review`
        : `${pct}% of ${formatNumber(reviews)} reviews`;
}

export function deckConfigPercentInput({ pct }: { pct: string }): string {
    return `${pct}%`;
}

export function deckConfigCheckingForImprovement(): string {
    return "Checking for improvement...";
}

export function deckConfigFsrsParamsOptimal(): string {
    return "The FSRS parameters are currently optimal. This is expected once parameters are well-fitted to your review history. You can try optimizing again after doing more reviews.";
}

export function deckConfigFsrsConfirmSaveAndOptimize(): string {
    return "This will save any other changes you have made. Are you sure?";
}

export function deckConfigFsrsParamsNoReviews(): string {
    return "No reviews found in this deck or its subdecks.";
}

export function deckConfigWaitForAudio(): string {
    return "Wait for audio";
}

export function deckConfigShowReminder(): string {
    return "Show Reminder";
}

export function deckConfigAnswerAgain(): string {
    return "Answer Again";
}

export function deckConfigAnswerHard(): string {
    return "Answer Hard";
}

export function deckConfigAnswerGood(): string {
    return "Answer Good";
}

export function deckConfigDaysToSimulate(): string {
    return "Days to simulate";
}

export function deckConfigDesiredRetentionBelowOptimal(): string {
    return "Your desired retention is below optimal. Increasing it is recommended.";
}

export function deckConfigFsrsSimulatorExperimental(): string {
    return "FSRS Simulator (Experimental)";
}

export function deckConfigFsrsSimulateDesiredRetentionExperimental(): string {
    return "FSRS Desired Retention Simulator (Experimental)";
}

export function deckConfigFsrsSimulateSaveFirst(): string {
    return "After optimizing, please save your deck options before running the simulator.";
}

export function deckConfigFsrsDesiredRetentionHelpMeDecideExperimental(): string {
    return "Help Me Decide (Experimental)";
}

export function deckConfigAdditionalNewCardsToSimulate(): string {
    return "Additional new cards to simulate";
}

export function deckConfigSimulate(): string {
    return "Simulate";
}

export function deckConfigClearLastSimulate(): string {
    return "Clear Last Simulation";
}

export function deckConfigFsrsSimulatorRadioCount(): string {
    return "Reviews";
}

export function deckConfigAdvancedSettings(): string {
    return "Advanced Settings";
}

export function deckConfigSmoothGraph(): string {
    return "Smooth graph";
}

export function deckConfigSaveOptionsToDeck(): string {
    return "Save Changes to Deck";
}

export function deckConfigSaveOptionsToDeckConfirm(): string {
    return "Overwrite the options of this deck with the options that are currently set in the simulator?";
}

export function deckConfigFsrsSimulatorRadioMemorized(): string {
    return "Memorized";
}

export function deckConfigFsrsSimulatorRadioEfficiency(): string {
    return "Efficiency";
}

export function deckConfigFsrsSimulatorRatioTooltip2({ time }: { time: string }): string {
    return `${time} memorized cards per hour`;
}

export function deckConfigHealthCheck(): string {
    return "Check health when optimizing";
}

export function deckConfigFsrsBadFitWarning(): string {
    return "Health Check:\nYour memory is difficult for FSRS to predict. Recommendations:\n\n- Suspend or reformulate any cards you constantly forget.\n- Use the answer buttons consistently. Keep in mind that \"Hard\" is a passing grade, not a failing grade.\n- Understand before you memorize.\n\nIf you follow these suggestions, performance will usually improve over the next few months.";
}

export function deckConfigFsrsGoodFit(): string {
    return "Health Check:\nFSRS can adapt to your memory well.";
}

export function deckConfigHealthCheckTooltip1(): string {
    return "This will show a warning if FSRS struggles to adapt to your memory.";
}

export function deckConfigHealthCheckTooltip2(): string {
    return "Health check is performed only when using Optimize This Deck.";
}

export function deckConfigComputeOptimalRetention(): string {
    return "Compute minimum recommended retention";
}

export function deckConfigPredictedOptimalRetention({ num }: { num: number | string }): string {
    return `Minimum recommended retention: ${formatArg(num)}`;
}

export function decksDeck(): string {
    return "Deck";
}

export function decksOrderAdded(): string {
    return "Order added";
}

export function decksLatestAddedFirst(): string {
    return "Latest added first";
}

export function decksRelativeOverdueness(): string {
    return "Relative overdueness";
}

export function helpForMoreInfo({ link }: { link: string }): string {
    return `For more information, see ${link} in the manual.`;
}

export function helpOpenManualChapter({ name }: { name: string }): string {
    return `Open ${name} in the manual`;
}

export function helpOk(): string {
    return "OK";
}

export function helpNoExplanation({ link }: { link: string }): string {
    return `Whoops! There doesn't seem to be an explanation for this setting yet.\n\nYou can help us complete this help page on ${link}.`;
}

export function keyboardCtrl(): string {
    return "Ctrl";
}

export function keyboardShift(): string {
    return "Shift";
}

export function schedulingTimeSpanSeconds({ amount }: { amount: number }): string {
    return isOne(amount) ? `${formatNumber(amount)} second` : `${formatNumber(amount)} seconds`;
}

export function schedulingTimeSpanMinutes({ amount }: { amount: number }): string {
    return isOne(amount) ? `${formatNumber(amount)} minute` : `${formatNumber(amount)} minutes`;
}

export function schedulingTimeSpanHours({ amount }: { amount: number }): string {
    return isOne(amount) ? `${formatNumber(amount)} hour` : `${formatNumber(amount)} hours`;
}

export function schedulingTimeSpanDays({ amount }: { amount: number }): string {
    return isOne(amount) ? `${formatNumber(amount)} day` : `${formatNumber(amount)} days`;
}

export function schedulingTimeSpanMonths({ amount }: { amount: number }): string {
    return isOne(amount) ? `${formatNumber(amount)} month` : `${formatNumber(amount)} months`;
}

export function schedulingTimeSpanYears({ amount }: { amount: number }): string {
    return isOne(amount) ? `${formatNumber(amount)} year` : `${formatNumber(amount)} years`;
}

export function schedulingLapses(): string {
    return "Lapses";
}

export function schedulingLearning(): string {
    return "Learning";
}

export function schedulingMaximumInterval(): string {
    return "Maximum interval";
}

export function schedulingMaximumReviewsday(): string {
    return "Maximum reviews/day";
}

export function schedulingNewCards(): string {
    return "New Cards";
}

export function schedulingNewCardsday(): string {
    return "New cards/day";
}

export function schedulingShowAnswerTimer(): string {
    return "Show on-screen timer";
}

export function statisticsDueCount(): string {
    return "Due";
}

export function statisticsAverageAnswerTime(
    { averageSeconds, cardsPerMinute }: { averageSeconds: number; cardsPerMinute: number },
): string {
    return `${formatNumber(averageSeconds)}s (${formatNumber(cardsPerMinute)} cards/minute)`;
}

export function statisticsStudiedToday(
    { unit, secsPerCard, amount, cards }: { unit: string; secsPerCard: number; amount: number; cards: number },
): string {
    const singular = {
        seconds: "second",
        minutes: "minute",
        hours: "hour",
        days: "day",
        months: "month",
    }[unit] ?? "year";
    const span = `in ${formatNumber(amount)} ${singular}${isOne(amount) ? "" : "s"}`;
    return `Studied ${statisticsCards({ cards })}\n${span} today\n(${formatNumber(secsPerCard)}s/card)`;
}

export function statisticsCards({ cards }: { cards: number }): string {
    return isOne(cards) ? `${formatNumber(cards)} card` : `${formatNumber(cards)} cards`;
}

export function statisticsNotes({ notes }: { notes: number }): string {
    return isOne(notes) ? `${formatNumber(notes)} note` : `${formatNumber(notes)} notes`;
}

export function statisticsReviews({ reviews }: { reviews: number }): string {
    return isOne(reviews) ? `${formatNumber(reviews)} review` : `${formatNumber(reviews)} reviews`;
}

export function statisticsMemorized({ memorized }: { memorized: string }): string {
    return `${memorized} cards memorized`;
}

export function statisticsTodayTitle(): string {
    return "Today";
}

export function statisticsTodayAgainCount(): string {
    return "Again count:";
}

export function statisticsTodayTypeCounts(
    { learnCount, reviewCount, relearnCount, filteredCount }: {
        learnCount: number;
        reviewCount: number;
        relearnCount: number;
        filteredCount: number;
    },
): string {
    return `Learn: ${formatNumber(learnCount)}, Review: ${formatNumber(reviewCount)}, Relearn: ${
        formatNumber(relearnCount)
    }, Filtered: ${formatNumber(filteredCount)}`;
}

export function statisticsTodayNoCards(): string {
    return "No cards have been studied today.";
}

export function statisticsTodayNoMatureCards(): string {
    return "No mature cards were studied today.";
}

export function statisticsTodayCorrectMature(
    { correct, total, percent }: { correct: number; total: number; percent: number | string },
): string {
    return `Correct answers on mature cards: ${formatNumber(correct)}/${formatNumber(total)} (${formatArg(percent)}%)`;
}

export function statisticsCountsTotalCards(): string {
    return "Total";
}

export function statisticsCountsNewCards(): string {
    return "New";
}

export function statisticsCountsYoungCards(): string {
    return "Young";
}

export function statisticsCountsMatureCards(): string {
    return "Mature";
}

export function statisticsCountsSuspendedCards(): string {
    return "Suspended";
}

export function statisticsCountsFilteredCards(): string {
    return "Filtered";
}

export function statisticsCountsLearningCards(): string {
    return "Learning";
}

export function statisticsCountsRelearningCards(): string {
    return "Relearning";
}

export function statisticsCountsTitle(): string {
    return "Card Counts";
}

export function statisticsCountsSeparateSuspendedCards(): string {
    return "Separate suspended cards";
}

export function statisticsTrueRetentionTitle(): string {
    return "Retention";
}

export function statisticsTrueRetentionSubtitle(): string {
    return "Pass rate of cards with an interval ≥ 1 day.";
}

export function statisticsTrueRetentionTooltip(): string {
    return "Your retention is expected to be close to your desired retention. Please keep in mind that data for a single day is noisy, so it's better to look at monthly data.";
}

export function statisticsTrueRetentionPass(): string {
    return "Pass";
}

export function statisticsTrueRetentionFail(): string {
    return "Fail";
}

export function statisticsTrueRetentionTotal(): string {
    return "Total";
}

export function statisticsTrueRetentionCount(): string {
    return "Count";
}

export function statisticsTrueRetentionRetention(): string {
    return "Retention";
}

export function statisticsTrueRetentionYoung(): string {
    return "Young";
}

export function statisticsTrueRetentionMature(): string {
    return "Mature";
}

export function statisticsTrueRetentionAll(): string {
    return "All";
}

export function statisticsTrueRetentionToday(): string {
    return "Today";
}

export function statisticsTrueRetentionYesterday(): string {
    return "Yesterday";
}

export function statisticsTrueRetentionWeek(): string {
    return "Last week";
}

export function statisticsTrueRetentionMonth(): string {
    return "Last month";
}

export function statisticsTrueRetentionYear(): string {
    return "Last year";
}

export function statisticsTrueRetentionAllTime(): string {
    return "All time";
}

export function statisticsTrueRetentionNotApplicable(): string {
    return "N/A";
}

export function statisticsRangeAllTime(): string {
    return "all";
}

export function statisticsRange1YearHistory(): string {
    return "last 12 months";
}

export function statisticsRangeAllHistory(): string {
    return "all history";
}

export function statisticsRangeDeck(): string {
    return "deck";
}

export function statisticsRangeCollection(): string {
    return "collection";
}

export function statisticsRangeSearch(): string {
    return "Search";
}

export function statisticsCardDifficultyTitle(): string {
    return "Card Difficulty";
}

export function statisticsCardStabilityTitle(): string {
    return "Card Stability";
}

export function statisticsCardStabilitySubtitle(): string {
    return "The delay at which retrievability falls to 90%.";
}

export function statisticsMedianStability(): string {
    return "Median stability";
}

export function statisticsCardRetrievabilityTitle(): string {
    return "Card Retrievability";
}

export function statisticsCardDifficultySubtitle2(): string {
    return "The higher the difficulty, the slower stability will increase.";
}

export function statisticsRetrievabilitySubtitle(): string {
    return "The probability of recalling a card today.";
}

export function statisticsCardDifficultyTooltip(
    { cards, percent }: { cards: number; percent: number | string },
): string {
    return isOne(cards)
        ? `${formatNumber(cards)} card with ${formatArg(percent)} difficulty`
        : `${formatNumber(cards)} cards with ${formatArg(percent)} difficulty`;
}

export function statisticsRetrievabilityTooltip(
    { cards, percent }: { cards: number; percent: number | string },
): string {
    return isOne(cards)
        ? `${formatNumber(cards)} card with ${formatArg(percent)} retrievability`
        : `${formatNumber(cards)} cards with ${formatArg(percent)} retrievability`;
}

export function statisticsFutureDueTitle(): string {
    return "Future Due";
}

export function statisticsFutureDueSubtitle(): string {
    return "The number of reviews due in the future.";
}

export function statisticsAddedTitle(): string {
    return "Added";
}

export function statisticsAddedSubtitle(): string {
    return "The number of new cards you have added.";
}

export function statisticsReviewsCountSubtitle(): string {
    return "The number of questions you have answered.";
}

export function statisticsReviewsTimeSubtitle(): string {
    return "The time taken to answer the questions.";
}

export function statisticsAnswerButtonsTitle(): string {
    return "Answer Buttons";
}

export function statisticsAnswerButtonsButtonNumber(): string {
    return "Button";
}

export function statisticsAnswerButtonsButtonPressed(): string {
    return "Times pressed";
}

export function statisticsAnswerButtonsSubtitle(): string {
    return "The number of times you have pressed each button.";
}

export function statisticsReviewsTitle(): string {
    return "Reviews";
}

export function statisticsReviewsTimeCheckbox(): string {
    return "Time";
}

export function statisticsInDaysSingle({ days }: { days: number }): string {
    if (days === 0) {
        return "Today";
    }
    if (days === 1) {
        return "Tomorrow";
    }
    return `In ${formatNumber(days)} days`;
}

export function statisticsInDaysRange({ daysStart, daysEnd }: { daysStart: number; daysEnd: number }): string {
    return `In ${formatNumber(daysStart)}-${formatNumber(daysEnd)} days`;
}

export function statisticsDaysAgoSingle({ days }: { days: number }): string {
    return days === 1 ? "Yesterday" : `${formatNumber(days)} days ago`;
}

export function statisticsDaysAgoRange({ daysStart, daysEnd }: { daysStart: number; daysEnd: number }): string {
    return `${formatNumber(daysStart)}-${formatNumber(daysEnd)} days ago`;
}

export function statisticsRunningTotal(): string {
    return "Running total";
}

export function statisticsCardsDue({ cards }: { cards: number }): string {
    return isOne(cards) ? `${formatNumber(cards)} card due` : `${formatNumber(cards)} cards due`;
}

export function statisticsBacklogCheckbox(): string {
    return "Backlog";
}

export function statisticsIntervalsTitle(): string {
    return "Review Intervals";
}

export function statisticsIntervalsSubtitle(): string {
    return "Delays until review cards are shown again.";
}

export function statisticsIntervalsDayRange(
    { cards, daysStart, daysEnd }: { cards: number; daysStart: number; daysEnd: number },
): string {
    return isOne(cards)
        ? `${formatNumber(cards)} card with a ${formatNumber(daysStart)}~${formatNumber(daysEnd)} day interval`
        : `${formatNumber(cards)} cards with a ${formatNumber(daysStart)}~${formatNumber(daysEnd)} day interval`;
}

export function statisticsIntervalsDaySingle({ cards, day }: { cards: number; day: number | string }): string {
    return isOne(cards)
        ? `${formatNumber(cards)} card with a ${formatArg(day)} day interval`
        : `${formatNumber(cards)} cards with a ${formatArg(day)} day interval`;
}

export function statisticsStabilityDayRange(
    { cards, daysStart, daysEnd }: { cards: number; daysStart: number; daysEnd: number },
): string {
    return isOne(cards)
        ? `${formatNumber(cards)} card with a ${formatNumber(daysStart)}~${formatNumber(daysEnd)} day stability`
        : `${formatNumber(cards)} cards with a ${formatNumber(daysStart)}~${formatNumber(daysEnd)} day stability`;
}

export function statisticsStabilityDaySingle({ cards, day }: { cards: number; day: number | string }): string {
    return isOne(cards)
        ? `${formatNumber(cards)} card with a ${formatArg(day)} day stability`
        : `${formatNumber(cards)} cards with a ${formatArg(day)} day stability`;
}

export function statisticsHoursRange({ hourStart, hourEnd }: { hourStart: number; hourEnd: number }): string {
    return `From ${formatNumber(hourStart)}:00~${formatNumber(hourEnd)}:00`;
}

export function statisticsHoursCorrect(
    { correct, total, percent }: { correct: number; total: number; percent: number | string },
): string {
    return `${formatNumber(correct)}/${formatNumber(total)} correct (${formatArg(percent)}%)`;
}

export function statisticsHoursCorrectInfo(): string {
    return "→ (not 'Again')";
}

export function statisticsHoursReviews({ reviews }: { reviews: number }): string {
    return `📊 ${formatNumber(reviews)} reviews`;
}

export function statisticsHoursCorrectReviews(
    { percent, reviews }: { percent: number | string; reviews: number },
): string {
    return `📈 ${formatArg(percent)}% correct (${formatNumber(reviews)})`;
}

export function statisticsHoursTitle(): string {
    return "Hourly Breakdown";
}

export function statisticsHoursSubtitle(): string {
    return "Review success rate for each hour of the day.";
}

export function statisticsNoData(): string {
    return "NO DATA";
}

export function statisticsCalendarTitle(): string {
    return "Calendar";
}

export function statisticsElapsedTimeSeconds({ amount }: { amount: number }): string {
    return `${formatNumber(amount)}s`;
}

export function statisticsElapsedTimeMinutes({ amount }: { amount: number }): string {
    return `${formatNumber(amount)}m`;
}

export function statisticsElapsedTimeHours({ amount }: { amount: number }): string {
    return `${formatNumber(amount)}h`;
}

export function statisticsElapsedTimeDays({ amount }: { amount: number }): string {
    return `${formatNumber(amount)}d`;
}

export function statisticsElapsedTimeMonths({ amount }: { amount: number }): string {
    return `${formatNumber(amount)}mo`;
}

export function statisticsElapsedTimeYears({ amount }: { amount: number }): string {
    return `${formatNumber(amount)}y`;
}

export function statisticsAverageForDaysStudied(): string {
    return "Average for days studied";
}

export function statisticsTotal(): string {
    return "Total";
}

export function statisticsDaysStudied(): string {
    return "Days studied";
}

export function statisticsAverageAnswerTimeLabel(): string {
    return "Average answer time";
}

export function statisticsAverage(): string {
    return "Average";
}

export function statisticsMedianInterval(): string {
    return "Median interval";
}

export function statisticsDueTomorrow(): string {
    return "Due tomorrow";
}

export function statisticsDailyLoad(): string {
    return "Daily load";
}

export function statisticsAmountOfTotalWithPercentage(
    { amount, total, percent }: { amount: number; total: number; percent: number | string },
): string {
    return `${formatNumber(amount)} of ${formatNumber(total)} (${formatArg(percent)}%)`;
}

export function statisticsAverageOverPeriod(): string {
    return "Average over period";
}

export function statisticsReviewsPerDay({ count }: { count: number }): string {
    return isOne(count) ? `${formatNumber(count)} review/day` : `${formatNumber(count)} reviews/day`;
}

export function statisticsMinutesPerDay({ count }: { count: number }): string {
    return isOne(count) ? `${formatNumber(count)} minute/day` : `${formatNumber(count)} minutes/day`;
}

export function statisticsCardsPerDay({ count }: { count: number }): string {
    return isOne(count) ? `${formatNumber(count)} card/day` : `${formatNumber(count)} cards/day`;
}

export function statisticsMedianDifficulty(): string {
    return "Median difficulty";
}

export function statisticsAverageRetrievability(): string {
    return "Average retrievability";
}

export function statisticsEstimatedTotalKnowledge(): string {
    return "Estimated total knowledge";
}

export function studyingAgain(): string {
    return "Again";
}

export function studyingEasy(): string {
    return "Easy";
}

export function studyingGood(): string {
    return "Good";
}

export function studyingHard(): string {
    return "Hard";
}

export function studyingPleaseRunToolsemptyCards(): string {
    return "Please run Tools>Empty Cards";
}

export function studyingShowAnswer(): string {
    return "Show Answer";
}

export function studyingStudyNow(): string {
    return "Study Now";
}

export function studyingToReview(): string {
    return "To Review";
}

export function studyingTypeAnswerUnknownField({ val }: { val: number | string }): string {
    return `Type answer: unknown field ${formatArg(val)}`;
}

export function undoUndo(): string {
    return "Undo";
}

export function undoActionUndone({ action }: { action: string }): string {
    return `${action} undone`;
}
