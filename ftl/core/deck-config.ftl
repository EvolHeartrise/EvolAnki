### Text shown on the "Deck Options" screen


## Top section

deck-config-default-name = Default
deck-config-title = Deck Options

## Daily limits section

deck-config-daily-limits = Daily Limits
deck-config-new-limit-tooltip =
    The maximum number of new cards to introduce in a day, if new cards are available.
    Because new material will increase your short-term review workload, this should typically
    be at least 10x smaller than your review limit.
deck-config-review-limit-tooltip =
    The maximum number of review cards to show in a day,
    if cards are ready for review.
deck-config-limit-deck-v3 =
    When studying a deck that has subdecks inside it, the limits set on each
    subdeck control the maximum number of cards gathered from that particular deck.
    The selected deck's limits control the total cards that will be shown.
deck-config-limit-interday-bound-by-reviews =
    The review limit also affects interday learning cards. When applying the limit,
    interday learning cards are gathered first, then review cards.
deck-config-tab-description =
    - `Every day`: The limit for this deck.
    - `Today only`: Make a temporary change to this deck's limit.
deck-config-new-cards-ignore-review-limit = New cards ignore review limit
deck-config-new-cards-ignore-review-limit-tooltip =
    By default, the review limit also applies to new cards, and no new cards will be
    shown when the review limit has been reached. If this option is enabled, new cards
    will be shown regardless of the review limit.
deck-config-apply-all-parent-limits = Limits start from top
deck-config-apply-all-parent-limits-tooltip =
    By default, the daily limits of a higher-level deck do not apply if you're studying from its subdeck.
    If this option is enabled, the limits will
    start from the top-level deck instead, which can be useful if you wish to study individual
    subdecks, while enforcing a total limit on cards for the deck tree.
deck-config-affects-entire-collection = Affects the entire collection.

## Daily limit tabs: please try to keep these as short as the English version,
## as longer text will not fit on small screens.

deck-config-every-day = Every day
deck-config-today-only = Today only

## New Cards section

deck-config-learning-steps = Learning steps
# Please don't translate `1m`, `2d`
-deck-config-delay-hint = Delays are typically minutes (e.g. `1m`) or days (e.g. `2d`), but hours (e.g. `1h`) and seconds (e.g. `30s`) are also supported.
deck-config-learning-steps-tooltip =
    One or more delays, separated by spaces. The first delay will be used
    when you press the `Again` button on a new card, and is 1 minute by default.
    The `Good` button will advance to the next step, which is 10 minutes by default.
    Once all steps have been passed, the card will become a review card, and
    will appear on a different day. { -deck-config-delay-hint }
deck-config-new-insertion-order = Insertion order
deck-config-new-insertion-order-tooltip =
    Controls the position (due #) new cards are assigned when you add new cards.
    Cards with a lower due number will be shown first when studying. Changing
    this option will automatically update the existing position of new cards.
deck-config-new-insertion-order-sequential = Sequential (oldest cards first)
deck-config-new-insertion-order-random = Random
deck-config-new-insertion-order-random-with-v3 =
    With the v3 scheduler, it is better to leave this set to sequential, and
    adjust the new card gather order instead.

## Lapses section

deck-config-relearning-steps = Relearning steps
deck-config-relearning-steps-tooltip =
    Zero or more delays, separated by spaces. By default, pressing the `Again`
    button on a review card will show it again 10 minutes later. If no delays
    are provided, the card will have its interval changed, without entering
    relearning. { -deck-config-delay-hint }

## Gather order and sort order of cards

deck-config-ordering-title = Display Order
deck-config-new-gather-priority = New card gather order
deck-config-new-gather-priority-tooltip-3 =
    `Deck`: Gathers cards from each subdeck in order, starting from the top. Cards from each subdeck are
    gathered in ascending position. If the daily limit of the selected deck is reached, gathering
    can stop before all subdecks have been checked. This order is fastest in large collections, and
    allows you to prioritize subdecks that are closer to the top.
    
    `Deck, then random notes`: Like `Random notes`, but goes through subdecks in order,
    as `Deck` does, picking notes at random within each subdeck and gathering all of their cards.
    
    `Ascending position`: Gathers cards by ascending position (due #), which is typically
    the oldest-added first.
    
    `Descending position`: Gathers cards by descending position (due #), which is typically
    the latest-added first.
    
    `Random notes`: Picks notes at random, then gathers all of its cards.
    
    `Random cards`: Gathers cards in a random order.
deck-config-new-card-sort-order = New card sort order
deck-config-new-card-sort-order-tooltip-2 =
    `Card type, then order gathered`: Shows cards in order of card type number.
    Cards of each card type number are shown in the order they were gathered. 
    This will ensure all front→back cards are seen before any back→front cards.
    This is useful to have all cards of the same note shown in the same session, but not
    too close to one another.
    
    `Order gathered`: Shows cards exactly as they were gathered. This will typically result in all cards of a note being seen one after the other.
    
    `Card type, then random`: Shows cards in order of card type number. Cards of each card
    type number are shown in a random order. This order is useful if you don't want sibling cards
    to appear too close to each other, but still want the cards to appear in a random order.
    
    `Random note, then card type`: Picks notes at random, then shows all of its cards
    in order.
    
    `Random`: Shows cards in a random order.
deck-config-new-review-priority = New/review order
deck-config-new-review-priority-tooltip = When to show new cards in relation to review cards.
deck-config-interday-step-priority = Interday learning/review order
deck-config-interday-step-priority-tooltip =
    When to show (re)learning cards that cross a day boundary.
    
    The review limit is always applied first to interday learning cards, and
    then review cards. This option will control the order the gathered cards are shown in,
    but interday learning cards will always be gathered first.
deck-config-review-sort-order = Review sort order
deck-config-review-sort-order-tooltip =
    The default order prioritizes cards that have been waiting longest, so that
    if you have a backlog of reviews, the longest-waiting ones will appear
    first. If you have a large backlog that will take more than a few days to
    clear, or wish to see cards in subdeck order, you may find the alternate
    sort orders preferable.
deck-config-display-order-will-use-current-deck =
    Anki will use the display order from the deck you 
    select to study, and not any subdecks it may have.

## Gather order and sort order of cards – Combobox entries

# Gather new cards ordered by deck.
deck-config-new-gather-priority-deck = Deck
# Gather new cards ordered by deck, then ordered by random notes, ensuring all cards of the same note are grouped together.
deck-config-new-gather-priority-deck-then-random-notes = Deck, then random notes
# Gather new cards ordered by position number, ascending (lowest to highest).
deck-config-new-gather-priority-position-lowest-first = Ascending position
# Gather new cards ordered by position number, descending (highest to lowest).
deck-config-new-gather-priority-position-highest-first = Descending position
# Gather the cards ordered by random notes, ensuring all cards of the same note are grouped together.
deck-config-new-gather-priority-random-notes = Random notes
# Gather new cards randomly.
deck-config-new-gather-priority-random-cards = Random cards
# Sort the cards first by their type, in ascending order (alphabetically), then randomized within each type.
deck-config-sort-order-card-template-then-random = Card type, then random
# Sort the notes first randomly, then the cards by their type, in ascending order (alphabetically), within each note.
deck-config-sort-order-random-note-then-template = Random note, then card type
# Sort the cards randomly.
deck-config-sort-order-random = Random
# Sort the cards first by their type, in ascending order (alphabetically), then by the order they were gathered, in ascending order (oldest to newest).
deck-config-sort-order-template-then-gather = Card type, then order gathered
# Sort the cards by the order they were gathered, in ascending order (oldest to newest).
deck-config-sort-order-gather = Order gathered
# How new cards or interday learning cards are mixed with review cards.
deck-config-review-mix-mix-with-reviews = Mix with reviews
# How new cards or interday learning cards are mixed with review cards.
deck-config-review-mix-show-after-reviews = Show after reviews
# How new cards or interday learning cards are mixed with review cards.
deck-config-review-mix-show-before-reviews = Show before reviews
# Sort the cards first by due date, in ascending order (oldest due date to newest), then randomly within the same due date.
deck-config-sort-order-due-date-then-random = Due date, then random
# Sort the cards first by due date, in ascending order (oldest due date to newest), then by deck within the same due date.
deck-config-sort-order-due-date-then-deck = Due date, then deck
# Sort the cards first by deck, then by due date in ascending order (oldest due date to newest) within the same deck.
deck-config-sort-order-deck-then-due-date = Deck, then due date
# Sort the cards by the interval, in ascending order (shortest to longest).
deck-config-sort-order-ascending-intervals = Ascending intervals
# Sort the cards by the interval, in descending order (longest to shortest).
deck-config-sort-order-descending-intervals = Descending intervals
# Sort the cards by difficulty, in ascending order (easiest to hardest).
deck-config-sort-order-ascending-difficulty = Easy cards first
# Sort the cards by difficulty, in descending order (hardest to easiest).
deck-config-sort-order-descending-difficulty = Difficult cards first
# Sort the cards by retrievability percentage, in ascending order (0% to 100%, least retrievable to most easily retrievable).
deck-config-sort-order-retrievability-ascending = Ascending retrievability
# Sort the cards by retrievability percentage, in descending order (100% to 0%, most easily retrievable to least retrievable).
deck-config-sort-order-retrievability-descending = Descending retrievability

## Timer section

deck-config-timer-title = Timers
deck-config-maximum-answer-secs = Maximum answer seconds
deck-config-maximum-answer-secs-tooltip =
    The maximum number of seconds to record for a single review. If an answer
    exceeds this time (because you stepped away from the screen for example),
    the time taken will be recorded as the limit you have set.
deck-config-show-answer-timer-tooltip =
    On the Study screen, show a timer that counts the time you're
    taking to study each card.
deck-config-stop-timer-on-answer = Stop on-screen timer on answer
deck-config-stop-timer-on-answer-tooltip =
    Whether to stop the on-screen timer when the answer is revealed.
    This doesn't affect statistics.

## Auto Advance section

deck-config-seconds-to-show-question = Seconds to show question for
deck-config-seconds-to-show-question-tooltip-3 = When auto advance is activated, the number of seconds to wait before applying the question action. Set to 0 to disable.
deck-config-seconds-to-show-answer = Seconds to show answer for
deck-config-seconds-to-show-answer-tooltip-2 = When auto advance is activated, the number of seconds to wait before applying the answer action. Set to 0 to disable.
deck-config-question-action-show-answer = Show Answer
deck-config-question-action-show-reminder = Show Reminder
deck-config-question-action = Question action
deck-config-question-action-tool-tip = The action to perform after the question is shown, and time has elapsed.
deck-config-answer-action = Answer action
deck-config-answer-action-tooltip-2 = The action to perform after the answer is shown, and time has elapsed.
deck-config-wait-for-audio-tooltip-2 = Wait for audio to finish before automatically applying the question action or answer action.

## Audio section

deck-config-audio-title = Audio
deck-config-disable-autoplay = Don't play audio automatically
deck-config-disable-autoplay-tooltip =
    When enabled, Anki will not play audio automatically.
    It can be played manually by clicking/tapping on an audio icon, or by using the Replay action.
deck-config-skip-question-when-replaying = Skip question when replaying answer
deck-config-always-include-question-audio-tooltip =
    Whether the question audio should be included when the Replay action is
    used while looking at the answer side of a card.

## Advanced section

deck-config-advanced-title = Advanced
deck-config-maximum-interval-tooltip =
    The maximum number of days a review card will wait. When reviews have
    reached the limit, `Hard`, `Good` and `Easy` will all give the same delay.
    The shorter you set this, the greater your workload will be.

## Easy Days section.

deck-config-easy-days-normal = Normal


## Other Buttons

deck-config-save-button = Save
deck-config-save-and-optimize = Optimize All Decks
deck-config-revert-button-tooltip = Restore this setting to its default value?

## These strings are shown via the Description button at the bottom of the
## overview screen.


## Warnings shown to the user

deck-config-reviews-too-low =
    If adding { $cards ->
        [one] { $cards } new card each day
       *[other] { $cards } new cards each day
    }, your review limit should be at least { $expected }.
deck-config-maximum-answer-secs-above-recommended = Anki can schedule your reviews more efficiently when you keep each question short.
deck-config-too-short-maximum-interval = A maximum interval less than 6 months is not recommended.
deck-config-ignore-before-info = (Approximately) { $included }/{ $totalCards } cards will be used to optimize the FSRS parameters.

## Selecting a deck


## Messages related to the FSRS scheduler

deck-config-updating-cards = Updating cards: { $current_cards_count }/{ $total_cards_count }...
deck-config-invalid-parameters = The provided FSRS parameters are invalid. Leave them blank to use the default values.
deck-config-placeholder-parameters =
    Default parameters
    (Press "{ deck-config-optimize-button }" periodically to allow FSRS to better adjust to your memory)
deck-config-manual-parameter-edit-warning = The parameters should only be modified using the optimize button. Manually editing them is heavily advised against.
deck-config-not-enough-history = Insufficient review history to perform this operation.
deck-config-must-have-400-reviews =
    { $count ->
        [one] Only { $count } review was found.
       *[other] Only { $count } reviews were found.
    } You must have at least 400 reviews for this operation.
# Numbers that control how aggressively the FSRS algorithm schedules cards
deck-config-weights = FSRS parameters
deck-config-optimize-button = Optimize This Deck
# Indicates that a given function or label, provided via the "text" variable, operates slowly.
deck-config-slow-suffix = { $text } (slow)
deck-config-compute-button = Compute
deck-config-ignore-before = Ignore cards reviewed before
deck-config-time-to-optimize = It's been a while - using the Optimize All Decks button is recommended.
deck-config-evaluate-button = Evaluate
deck-config-desired-retention = Desired retention
deck-config-historical-retention = Historical retention
deck-config-smaller-is-better = Smaller numbers indicate a better fit to your review history.
deck-config-steps-too-large-for-fsrs = When FSRS is enabled, steps of 1 day or more are not recommended.
deck-config-iterations = Iteration: { $count }...
deck-config-reschedule-cards-on-change = Reschedule cards on change
deck-config-fsrs-tooltip =
    Affects the entire collection.
    
    The Free Spaced Repetition Scheduler (FSRS) is an alternative to Anki's legacy SuperMemo 2 (SM-2) algorithm.
    By more accurately determining how likely you are to forget a card, it can help you remember
    more material in the same amount of time.
deck-config-desired-retention-tooltip =
    By default, Anki schedules cards so that you have a 90% chance of remembering them when
    they come up for review again. If you increase this value, Anki will show cards more frequently
    to increase the chances of you remembering them. If you decrease the value, Anki will show cards
    less frequently, and you will forget more of them. Be conservative when adjusting this - higher
    values will greatly increase your workload, and lower values can be demoralizing when you forget
    a lot of material.
deck-config-desired-retention-tooltip2 = The workload values provided by the info box are a rough approximation. For a greater level of accuracy, use the simulator.
deck-config-historical-retention-tooltip =
    When some of your review history is missing, FSRS needs to fill in the gaps. By default, it will
    assume that when you did those old reviews, you remembered 90% of the material. If your old retention
    was appreciably higher or lower than 90%, adjusting this option will allow FSRS to better approximate
    the missing reviews.
    
    Your review history may be incomplete for two reasons:
    1. Because you're using the 'ignore cards reviewed before' option.
    2. Because you previously deleted review logs to free up space, or imported material from a different
    SRS program.
    
    The latter is quite rare, so unless you're using the former option, you probably don't need to adjust
    this option.
deck-config-weights-tooltip2 =
    FSRS parameters affect how cards are scheduled. Anki will start with default parameters. You can use 
    the option below to optimize the parameters to best match your performance in this deck.
deck-config-reschedule-cards-on-change-tooltip =
    Affects the entire collection, and is not saved.
    
    This option controls whether the due dates of cards will be changed when you enable FSRS, or optimize
    the parameters. The default is not to reschedule cards: future reviews will use the new scheduling, but
    there will be no immediate change to your workload. If rescheduling is enabled, the due dates of cards
    will be changed.
deck-config-reschedule-cards-warning =
    Depending on your desired retention, this can result in a large number of cards becoming
    due, so is not recommended when first switching from SM-2.
    
    Use this option sparingly, as it will add a review entry to each of your cards, and
    increase the size of your collection.
deck-config-ignore-before-tooltip-2 =
    If set, cards reviewed before the provided date will be ignored when optimizing FSRS parameters.
    This can be useful if you imported someone else's scheduling data, or have changed the way you use the answer buttons.
deck-config-compute-optimal-weights-tooltip2 =
    When you click the Optimize button, FSRS will analyze your review history, and generate parameters that are 
    optimal for your memory and the content you're studying. Each deck has its own parameters.
    You don't need to optimize your parameters frequently - once every few months is sufficient.
    
    By default, parameters will be calculated from the review history of this deck and its subdecks. With too few
    reviews, a deck keeps the parameters it has. You can
    optionally adjust the search before calculating the parameters, if you'd like to alter which cards are used for
    optimizing the parameters.
deck-config-please-save-your-changes-first = Please save your changes first.
deck-config-workload-factor-change =
    Approximate workload: { $factor }x
    (compared to { $previousDR }% desired retention)
deck-config-workload-factor-unchanged = The higher your desired retention, the more frequently cards will be shown to you.
deck-config-desired-retention-too-low = Your desired retention is very low, which can lead to very long intervals.
deck-config-desired-retention-too-high = Your desired retention is very high, which can lead to very short intervals.
deck-config-percent-of-reviews =
    { $reviews ->
        [one] { $pct }% of { $reviews } review
       *[other] { $pct }% of { $reviews } reviews
    }
deck-config-percent-input = { $pct }%
# This message appears during FSRS parameter optimization.
deck-config-checking-for-improvement = Checking for improvement...
deck-config-fsrs-params-optimal = The FSRS parameters are currently optimal. This is expected once parameters are well-fitted to your review history. You can try optimizing again after doing more reviews.
deck-config-fsrs-confirm-save-and-optimize = This will save any other changes you have made. Are you sure?
deck-config-fsrs-params-no-reviews = No reviews found in this deck or its subdecks.
deck-config-wait-for-audio = Wait for audio
deck-config-show-reminder = Show Reminder
deck-config-answer-again = Answer Again
deck-config-answer-hard = Answer Hard
deck-config-answer-good = Answer Good
deck-config-days-to-simulate = Days to simulate
deck-config-desired-retention-below-optimal = Your desired retention is below optimal. Increasing it is recommended.
# Description of the y axis in the FSRS simulation
# diagram (Deck options -> FSRS) showing the total number of
# cards that can be recalled or retrieved on a specific date.
deck-config-fsrs-simulator-experimental = FSRS Simulator (Experimental)
deck-config-fsrs-simulate-desired-retention-experimental = FSRS Desired Retention Simulator (Experimental)
deck-config-fsrs-simulate-save-first = After optimizing, please save your deck options before running the simulator.
deck-config-fsrs-desired-retention-help-me-decide-experimental = Help Me Decide (Experimental)
deck-config-additional-new-cards-to-simulate = Additional new cards to simulate
deck-config-simulate = Simulate
deck-config-clear-last-simulate = Clear Last Simulation
deck-config-fsrs-simulator-radio-count = Reviews
deck-config-advanced-settings = Advanced Settings
deck-config-smooth-graph = Smooth graph
deck-config-save-options-to-deck = Save Changes to Deck
deck-config-save-options-to-deck-confirm = Overwrite the options of this deck with the options that are currently set in the simulator?
# Radio button in the FSRS simulation diagram (Deck options -> FSRS) selecting
# to show the total number of cards that can be recalled or retrieved on a
# specific date.
deck-config-fsrs-simulator-radio-memorized = Memorized
deck-config-fsrs-simulator-radio-efficiency = Efficiency
deck-config-fsrs-simulator-ratio-tooltip2 = { $time } memorized cards per hour

## Messages related to the FSRS scheduler’s health check. The health check determines whether the correlation between FSRS predictions and your memory is good or bad. It can be optionally triggered as part of the "Optimize" function.

# Checkbox
deck-config-health-check = Check health when optimizing
# Message box showing the result of the health check
deck-config-fsrs-bad-fit-warning =
    Health Check:
    Your memory is difficult for FSRS to predict. Recommendations:
    
    - Suspend or reformulate any cards you constantly forget.
    - Use the answer buttons consistently. Keep in mind that "Hard" is a passing grade, not a failing grade.
    - Understand before you memorize.
    
    If you follow these suggestions, performance will usually improve over the next few months.
# Message box showing the result of the health check
deck-config-fsrs-good-fit =
    Health Check:
    FSRS can adapt to your memory well.

## NO NEED TO TRANSLATE. This text is no longer used by Anki, and will be removed in the future.

deck-config-unable-to-determine-desired-retention = Unable to determine a minimum recommended retention.
deck-config-health-check-tooltip1 = This will show a warning if FSRS struggles to adapt to your memory.
deck-config-health-check-tooltip2 = Health check is performed only when using Optimize This Deck.
deck-config-compute-optimal-retention = Compute minimum recommended retention
deck-config-predicted-optimal-retention = Minimum recommended retention: { $num }
