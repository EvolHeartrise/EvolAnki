<!--
Copyright: Ankitects Pty Ltd and contributors
License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html
-->
<script lang="ts">
    import { goto } from "$app/navigation";
    import type { AVTag } from "@generated/anki/card_rendering_pb";
    import {
        BuryOrSuspendCardsRequest_Mode,
        CardAnswer_Rating,
        type QueuedCards,
        QueuedCards_Queue,
        type QueuedCards_QueuedCard,
    } from "@generated/anki/scheduler_pb";
    import {
        answerCard,
        buryOrSuspendCards,
        compareAnswer,
        describeNextStates,
        encodeIriPaths,
        extractAvTags,
        extractClozeForTyping,
        getCard,
        getCurrentDeck,
        getDeck,
        getDeckConfig,
        getNote,
        getNotetype,
        getQueuedCards,
        getUndoStatus,
        renderExistingCard,
        stateIsLeech,
        undo,
    } from "@generated/backend";
    import * as tr from "@generated/ftl";
    import type { FrameMessage } from "@tslib/frame-protocol";
    import { onDestroy, onMount } from "svelte";

    import SandboxedFrame from "$lib/components/SandboxedFrame.svelte";
    import { pageTheme } from "$lib/sveltelib/theme";

    import type { PageData } from "./$types";
    import {
        AnswerTimer,
        avRefsToPlayIcons,
        fillInComparison,
        finishRendering,
        mediaUrl,
        parseTypeAnswer,
        pendingNotice,
        replaceTypeAnswer,
        stripAvRefs,
        tagsToReplay,
        typeAnswerInput,
        withStyle,
    } from "./lib";

    export let data: PageData;

    const NOTICE_MILLIS = 4000;
    /** A card that keeps navigating its frame shouldn't spin forever. */
    const MAX_NAVIGATIONS = 3;

    /** "transition" ignores input while a request is running. */
    type Side = "question" | "answer" | "transition";

    interface TypeAnswer {
        expected: string;
        combining: boolean;
        ignoreCase: boolean;
        font: string;
        size: number;
    }

    interface Rendered {
        question: string;
        answer: string;
        templateIdx: number;
        questionTags: AVTag[];
        answerTags: AVTag[];
        typeAnswer: TypeAnswer | null;
        autoplay: boolean;
        replayQuestionOnAnswer: boolean;
    }

    let frame: SandboxedFrame;
    let frameReady = false;
    let side: Side = "transition";
    let shownSide: "q" | "a" = "q";
    let queued: QueuedCards | null = null;
    let current: QueuedCards_QueuedCard | null = null;
    let rendered: Rendered | null = null;
    let shownAnswerHtml = "";
    let labels: string[] = [];
    let undoLabel = "";
    let notice = "";
    let error = "";
    let noticeTimer: ReturnType<typeof setTimeout> | undefined;
    let navigations = 0;
    let frameSeq = 0;
    let timerSeq = -1;
    let timer = new AnswerTimer();
    let resolveTyped: ((value: string) => void) | null = null;
    let audioQueue: string[] = [];
    let audio: HTMLAudioElement;

    const ratingButtons: [CardAnswer_Rating, string, string][] = [
        [CardAnswer_Rating.AGAIN, tr.studyingAgain(), "again"],
        [CardAnswer_Rating.HARD, tr.studyingHard(), "hard"],
        [CardAnswer_Rating.GOOD, tr.studyingGood(), "good"],
        [CardAnswer_Rating.EASY, tr.studyingEasy(), "easy"],
    ];

    function bodyClassFor(templateIdx: number, dark: boolean): string {
        const night = dark ? " nightMode night_mode" : "";
        return `card card${templateIdx + 1}${night}`;
    }

    function showNotice(text: string): void {
        notice = text;
        clearTimeout(noticeTimer);
        noticeTimer = setTimeout(() => (notice = ""), NOTICE_MILLIS);
    }

    function showFailure(err: unknown): void {
        error = err instanceof Error ? err.message : String(err);
    }

    async function refreshUndo(): Promise<void> {
        undoLabel = (await getUndoStatus({}, { alertOnError: false })).undo;
    }

    /** card.question(), card.answer() and _mungeQA(), except type answers. */
    async function renderCard(card: QueuedCards_QueuedCard): Promise<Rendered> {
        const { id, templateIdx, noteId } = card.card!;
        const out = await renderExistingCard(
            { cardId: id, browser: false, partialRender: true },
            { alertOnError: false },
        );
        const qtext = finishRendering(out.questionNodes, null);
        const qout = await extractAvTags({ text: qtext, questionSide: true });
        const atext = finishRendering(out.answerNodes, qout.text);
        const aout = await extractAvTags({ text: atext, questionSide: false });

        const prepare = async (text: string): Promise<string> => {
            const encoded = (await encodeIriPaths({ val: text })).val;
            const withAudio = data.hidePlayButtons
                ? stripAvRefs(encoded)
                : avRefsToPlayIcons(encoded);
            return withStyle(out.css, withAudio);
        };
        const [question, answer] = await Promise.all([
            prepare(qout.text),
            prepare(aout.text),
        ]);
        const config = await deckConfigFor(card);
        return {
            question,
            answer,
            templateIdx,
            questionTags: qout.avTags,
            answerTags: aout.avTags,
            typeAnswer: await typeAnswerFor(question, noteId, templateIdx),
            autoplay: !config?.disableAutoplay,
            replayQuestionOnAnswer: !config?.skipQuestionWhenReplayingAnswer,
        };
    }

    async function deckConfigFor(card: QueuedCards_QueuedCard) {
        const { originalDeckId, deckId } = card.card!;
        const deck = await getDeck({ did: originalDeckId || deckId });
        if (deck.kind.case !== "normal") {
            return undefined;
        }
        const dcid = deck.kind.value.configId;
        return (await getDeckConfig({ dcid })).config;
    }

    /** typeAnsQuestionFilter() in qt/aqt/reviewer.py, up to the substitution. */
    async function typeAnswerFor(
        question: string,
        noteId: bigint,
        templateIdx: number,
    ): Promise<TypeAnswer | null> {
        const parsed = parseTypeAnswer(question);
        if (!parsed) {
            return null;
        }
        const note = await getNote({ nid: noteId });
        const notetype = await getNotetype({ ntid: note.notetypeId });
        const idx = notetype.fields.findIndex((f) => f.name === parsed.field);
        if (idx < 0) {
            return { ...parsed, expected: "", font: "", size: 0 };
        }
        let expected = note.fields[idx] ?? "";
        if (parsed.cloze) {
            const cloze = await extractClozeForTyping({
                text: expected,
                ordinal: templateIdx + 1,
            });
            expected = cloze.val;
        }
        return {
            ...parsed,
            expected,
            font: notetype.fields[idx].config?.fontName ?? "",
            size: notetype.fields[idx].config?.fontSize ?? 20,
        };
    }

    function questionHtml(card: Rendered): string {
        const parsed = parseTypeAnswer(card.question);
        if (!parsed) {
            return card.question;
        }
        if (!card.typeAnswer?.font) {
            const warning = parsed.cloze
                ? tr.studyingPleaseRunToolsemptyCards()
                : tr.studyingTypeAnswerUnknownField({ val: parsed.field });
            return replaceTypeAnswer(card.question, warning);
        }
        if (!card.typeAnswer.expected) {
            return replaceTypeAnswer(card.question, "");
        }
        return replaceTypeAnswer(
            card.question,
            typeAnswerInput(card.typeAnswer.font, card.typeAnswer.size),
        );
    }

    async function answerHtml(card: Rendered, typed: string): Promise<string> {
        const type = card.typeAnswer;
        if (!type?.expected) {
            return replaceTypeAnswer(card.answer, "");
        }
        const compared = await compareAnswer({
            expected: type.expected,
            provided: typed,
            combining: type.combining,
            ignoreCase: type.ignoreCase,
        });
        return fillInComparison(card.answer, compared.val, type.font, type.size);
    }

    function sendQuestion(): void {
        frameSeq += 1;
        timerSeq = frameSeq;
        frame.send({
            type: "question",
            html: questionHtml(rendered!),
            answer: rendered!.answer,
            bodyClass: bodyClassFor(rendered!.templateIdx, $pageTheme.isDark),
            dark: $pageTheme.isDark,
            seq: frameSeq,
        });
        shownSide = "q";
    }

    function sendAnswer(): void {
        frameSeq += 1;
        frame.send({
            type: "answer",
            html: shownAnswerHtml,
            bodyClass: bodyClassFor(rendered!.templateIdx, $pageTheme.isDark),
            dark: $pageTheme.isDark,
            seq: frameSeq,
        });
        shownSide = "a";
    }

    async function nextCard(): Promise<void> {
        side = "transition";
        stopAudio();
        error = "";
        try {
            queued = await getQueuedCards(
                { fetchLimit: 1, intradayLearningOnly: false },
                { alertOnError: false },
            );
            await refreshUndo();
            current = queued.cards[0] ?? null;
            if (!current) {
                pendingNotice.text = notice;
                await goto(`/overview/${data.deckId}`);
                return;
            }
            navigations = 0;
            labels = (await describeNextStates(current.states!, { alertOnError: false }))
                .vals;
            try {
                rendered = await renderCard(current);
            } catch (err) {
                // eg a template error, which shouldn't stop the user burying it
                rendered = {
                    question: `<div>${String(err).replace(/</g, "&lt;")}</div>`,
                    answer: "",
                    templateIdx: 0,
                    questionTags: [],
                    answerTags: [],
                    typeAnswer: null,
                    autoplay: false,
                    replayQuestionOnAnswer: false,
                };
            }
            timer = new AnswerTimer();
            shownAnswerHtml = "";
            if (frameReady) {
                sendQuestion();
            }
            side = "question";
            if (rendered.autoplay) {
                playTags(rendered.questionTags);
            }
        } catch (err) {
            showFailure(err);
        }
    }

    function askTyped(): Promise<string> {
        return new Promise((resolve) => {
            resolveTyped = resolve;
            frame.send({ type: "getTyped" });
        });
    }

    async function reveal(): Promise<void> {
        if (side !== "question" || !rendered) {
            return;
        }
        side = "transition";
        try {
            const typed = rendered.typeAnswer?.expected ? await askTyped() : "";
            shownAnswerHtml = await answerHtml(rendered, typed);
            sendAnswer();
            side = "answer";
            if (rendered.autoplay) {
                playTags(rendered.answerTags);
            }
        } catch (err) {
            showFailure(err);
        }
    }

    async function answer(rating: CardAnswer_Rating): Promise<void> {
        if (side !== "answer" || !current) {
            return;
        }
        const states = current.states!;
        // Sent back as received: the backend refuses a current state that differs
        // from its own, and these carry the FSRS memory state and any custom data.
        const newState = [states.again, states.hard, states.good, states.easy][rating]!;
        const cardId = current.card!.id;
        side = "transition";
        try {
            await answerCard(
                {
                    cardId,
                    currentState: states.current,
                    newState,
                    rating,
                    answeredAtMillis: BigInt(Date.now()),
                    millisecondsTaken: timer.elapsedMillis(),
                },
                { alertOnError: false },
            );
            const leech = await stateIsLeech(newState, { alertOnError: false });
            if (leech.val) {
                await showLeechNotice(cardId);
            }
        } catch (err) {
            // eg answered in another tab; carry on from what the backend has
            showNotice(err instanceof Error ? err.message : String(err));
        }
        await nextCard();
    }

    async function showLeechNotice(cardId: bigint): Promise<void> {
        const card = await getCard({ cid: cardId }, { alertOnError: false });
        let text = tr.studyingCardWasALeech();
        if (card.queue < 0) {
            text += ` ${tr.studyingItHasBeenSuspended()}`;
        }
        showNotice(text);
    }

    async function performUndo(): Promise<void> {
        if (side === "transition" || !undoLabel) {
            return;
        }
        side = "transition";
        try {
            const result = await undo({}, { alertOnError: false });
            showNotice(tr.undoActionUndone({ action: result.operation }));
            // selecting a deck is itself undoable, so this may have been in another
            const deck = await getCurrentDeck({}, { alertOnError: false });
            if (deck.id !== data.deckId) {
                pendingNotice.text = notice;
                await goto(`/overview/${deck.id}`);
                return;
            }
        } catch (err) {
            showNotice(err instanceof Error ? err.message : String(err));
        }
        await nextCard();
    }

    async function hide(
        mode: BuryOrSuspendCardsRequest_Mode,
        wholeNote: boolean,
    ): Promise<void> {
        if (side === "transition" || !current) {
            return;
        }
        const card = current.card!;
        side = "transition";
        try {
            await buryOrSuspendCards(
                {
                    cardIds: wholeNote ? [] : [card.id],
                    noteIds: wholeNote ? [card.noteId] : [],
                    mode,
                },
                { alertOnError: false },
            );
        } catch (err) {
            showNotice(err instanceof Error ? err.message : String(err));
        }
        await nextCard();
    }

    // Audio
    ///////////////////////////////////////////////////////////

    function stopAudio(): void {
        audioQueue = [];
        audio?.pause();
    }

    /** Replaces what's playing. TTS tags have nothing to play them yet. */
    function playTags(tags: AVTag[]): void {
        audioQueue = tags.flatMap((tag) =>
            tag.value.case === "soundOrVideo" ? [tag.value.value] : [],
        );
        playNext();
    }

    function playNext(): void {
        const filename = audioQueue.shift();
        if (filename === undefined) {
            return;
        }
        audio.src = mediaUrl(data.frameBase, filename);
        // Browsers may refuse until the page has had some interaction. There's
        // nothing to do about that but let the user press replay.
        audio.play().catch((err) => {
            if (err.name !== "AbortError") {
                console.debug(err);
            }
        });
    }

    function replay(): void {
        if (rendered && side !== "transition") {
            playTags(
                tagsToReplay(
                    shownSide,
                    rendered.questionTags,
                    rendered.answerTags,
                    rendered.replayQuestionOnAnswer,
                ),
            );
        }
    }

    function playFromCard(tagSide: "q" | "a", index: number): void {
        const tags = tagSide === "q" ? rendered?.questionTags : rendered?.answerTags;
        const tag = tags?.[index];
        if (tag) {
            playTags([tag]);
        }
    }

    // Input
    ///////////////////////////////////////////////////////////

    /** Keys from the frame only ever reach here if the user really pressed them. */
    function handleKey(key: string): boolean {
        switch (key) {
            case " ":
            case "Enter":
                if (side === "question") {
                    reveal();
                } else if (side === "answer") {
                    // as with Qt's default, these then rate the card as good
                    answer(CardAnswer_Rating.GOOD);
                }
                return true;
            case "1":
            case "2":
            case "3":
            case "4":
                answer(ratingButtons[Number(key) - 1][0]);
                return true;
            case "u":
                performUndo();
                return true;
            case "r":
            case "F5":
                replay();
                return true;
            case "-":
                hide(BuryOrSuspendCardsRequest_Mode.BURY_USER, false);
                return true;
            case "=":
                hide(BuryOrSuspendCardsRequest_Mode.BURY_USER, true);
                return true;
            case "@":
                hide(BuryOrSuspendCardsRequest_Mode.SUSPEND, false);
                return true;
            case "!":
                hide(BuryOrSuspendCardsRequest_Mode.SUSPEND, true);
                return true;
            default:
                return false;
        }
    }

    function onKeyDown(event: KeyboardEvent): void {
        if (event.ctrlKey || event.metaKey || event.altKey || event.repeat) {
            return;
        }
        if (handleKey(event.key)) {
            // space on a focused button would otherwise press it as well
            event.preventDefault();
        }
    }

    function onFrameMessage(message: FrameMessage): void {
        switch (message.type) {
            case "ready":
                frameReady = true;
                if (rendered && side !== "transition") {
                    const wasAnswer = side === "answer";
                    sendQuestion();
                    if (wasAnswer) {
                        sendAnswer();
                    }
                }
                break;
            case "shown":
                // excludes the time to load and render the card
                if (message.seq === timerSeq) {
                    timer.restart();
                }
                break;
            case "key":
                if (!message.repeat) {
                    handleKey(message.key);
                }
                break;
            case "play":
                playFromCard(message.side, message.index);
                break;
            case "reveal":
                reveal();
                break;
            case "typed":
                resolveTyped?.(message.value);
                resolveTyped = null;
                break;
            case "open":
                window.open(message.url, "_blank", "noopener,noreferrer");
                break;
        }
    }

    function onFrameNavigated(): void {
        frameReady = false;
        navigations += 1;
        resolveTyped?.("");
        resolveTyped = null;
        if (navigations >= MAX_NAVIGATIONS && rendered) {
            // it has had its chances; show the card without what's doing this
            rendered = {
                ...rendered,
                question: "<p>This card kept trying to leave the page, so it isn't shown.</p>",
                answer: "<p>This card kept trying to leave the page, so it isn't shown.</p>",
                typeAnswer: null,
            };
            shownAnswerHtml = rendered.answer;
        }
    }

    $: if (frameReady && rendered) {
        frame.send({ type: "theme", dark: $pageTheme.isDark });
    }

    $: currentQueue = current?.queue;

    onMount(() => {
        nextCard();
    });

    onDestroy(() => {
        clearTimeout(noticeTimer);
        stopAudio();
    });
</script>

<svelte:window on:keydown={onKeyDown} />

<div class="reviewer">
    <div class="card-area">
        <SandboxedFrame
            bind:this={frame}
            base={data.frameBase}
            onMessage={onFrameMessage}
            onNavigated={onFrameNavigated}
        />
    </div>

    {#if notice}
        <div class="notice" role="status">{notice}</div>
    {/if}
    {#if error}
        <div class="notice error" role="alert">{error}</div>
    {/if}

    <audio bind:this={audio} on:ended={playNext} on:error={playNext} hidden></audio>

    <footer>
        <div class="tools">
            <button disabled={!undoLabel} title={undoLabel} on:click={performUndo}>
                {tr.undoUndo()}
            </button>
            <button on:click={replay}>{tr.actionsReplayAudio()}</button>
            <button on:click={() => hide(BuryOrSuspendCardsRequest_Mode.BURY_USER, false)}>
                {tr.studyingBuryCard()}
            </button>
            <button on:click={() => hide(BuryOrSuspendCardsRequest_Mode.BURY_USER, true)}>
                {tr.studyingBuryNote()}
            </button>
            <button on:click={() => hide(BuryOrSuspendCardsRequest_Mode.SUSPEND, false)}>
                {tr.actionsSuspendCard()}
            </button>
            <button on:click={() => hide(BuryOrSuspendCardsRequest_Mode.SUSPEND, true)}>
                {tr.studyingSuspendNote()}
            </button>
        </div>

        <div class="counts">
            {#if queued}
                <span class="new-count" class:current={currentQueue === QueuedCards_Queue.NEW}>
                    {queued.newCount}
                </span>
                +
                <span
                    class="learn-count"
                    class:current={currentQueue === QueuedCards_Queue.LEARNING}
                >
                    {queued.learningCount}
                </span>
                +
                <span
                    class="review-count"
                    class:current={currentQueue === QueuedCards_Queue.REVIEW}
                >
                    {queued.reviewCount}
                </span>
            {/if}
        </div>

        <div class="actions">
            {#if shownSide === "q" || side === "question"}
                <button class="btn btn-primary show-answer" on:click={reveal}>
                    {tr.studyingShowAnswer()}
                </button>
            {:else}
                {#each ratingButtons as [rating, label, name], idx}
                    <button
                        class="btn btn-secondary rating {name}"
                        data-rating={idx + 1}
                        on:click={() => answer(rating)}
                    >
                        <span class="interval">{labels[idx] ?? ""}</span>
                        <span>{label}</span>
                    </button>
                {/each}
            {/if}
        </div>
    </footer>
</div>

<style lang="scss">
    .reviewer {
        display: flex;
        flex-direction: column;
        // less the height of the navigation in +layout.svelte
        height: calc(100dvh - 2.5rem);
    }

    .card-area {
        flex: 1;
        min-height: 0;
    }

    .notice {
        padding: 0.4em 1em;
        text-align: center;
        background: var(--canvas-inset);
        border-top: 1px solid var(--border-subtle);

        &.error {
            color: var(--accent-danger);
        }
    }

    footer {
        display: grid;
        grid-template-columns: 1fr auto 1fr;
        grid-template-areas: "counts actions tools";
        align-items: center;
        gap: 0.5em 1em;
        padding: 0.6em 1em;
        border-top: 1px solid var(--border-subtle);
        background: var(--canvas-elevated);
    }

    .counts {
        grid-area: counts;
        font-variant-numeric: tabular-nums;

        .current {
            text-decoration: underline;
            font-weight: bold;
        }
    }

    .actions {
        grid-area: actions;
        display: flex;
        gap: 0.5em;
        justify-content: center;
        min-height: 3.2em;
        align-items: center;
    }

    .show-answer {
        min-width: 12em;
    }

    .rating {
        display: flex;
        flex-direction: column;
        align-items: center;
        min-width: 5.5em;
        line-height: 1.2;
    }

    .interval {
        font-size: 0.8em;
        min-height: 1.2em;
        opacity: 0.8;
    }

    .tools {
        grid-area: tools;
        display: flex;
        flex-wrap: wrap;
        justify-content: flex-end;
        gap: 0.25em;

        button {
            padding: 0.1em 0.5em;
            font-size: 0.8em;
            color: var(--fg-subtle);
            background: none;
            border: 1px solid var(--border-subtle);
            border-radius: var(--border-radius);
            cursor: pointer;

            &:hover:not(:disabled) {
                color: var(--fg);
                background: var(--canvas-inset);
            }

            &:disabled {
                opacity: 0.5;
                cursor: default;
            }
        }
    }

    @media (max-width: 52em) {
        footer {
            grid-template-columns: 1fr;
            grid-template-areas: "counts" "actions" "tools";
            justify-items: center;
        }

        .tools {
            justify-content: center;
        }
    }
</style>
