<!--
Copyright: Ankitects Pty Ltd and contributors
License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html
-->
<script lang="ts">
    import { goto } from "$app/navigation";
    import type { AVTag } from "@generated/anki/card_rendering_pb";
    import {
        CardAnswer_Rating,
        type QueuedCards,
        QueuedCards_Queue,
        type QueuedCards_QueuedCard,
    } from "@generated/anki/scheduler_pb";
    import {
        answerCard,
        compareAnswer,
        encodeIriPaths,
        extractAvTags,
        extractClozeForTyping,
        getCurrentDeck,
        getDeck,
        getDeckConfig,
        getNote,
        getNotetype,
        getQueuedCards,
        getUndoStatus,
        renderExistingCard,
        undo,
    } from "@generated/backend";
    import * as tr from "@tslib/tr";
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
        parseTypeAnswer,
        pendingNotice,
        replaceTypeAnswer,
        stripAvRefs,
        tagsToReplay,
        typeAnswerInput,
        withStyle,
    } from "./lib";
    import { MediaPlayer, unlockMedia } from "./media";

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
    let mediaBlocked = false;
    let mediaContainer: HTMLElement;
    const player = new MediaPlayer(data.frameBase, {
        onBlocked: (blocked) => (mediaBlocked = blocked),
        onNotAudio: (filename) =>
            showNotice(`Can't play ${filename}: only audio files are supported.`),
    });

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
        return (await getDeckConfig({ dcid: deck.id })).config;
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
        player.stop();
        error = "";
        try {
            queued = await getQueuedCards(
                { fetchLimit: 1, intradayLearningOnly: false },
                { alertOnError: false },
            );
            await refreshUndo();
            current = queued.cards[0] ?? null;
            if (!current) {
                // Replacing this page, as going back to it would only come straight back.
                pendingNotice.text = notice;
                await goto("/", { replaceState: true });
                return;
            }
            navigations = 0;
            labels = current.labels;
            try {
                rendered = await renderCard(current);
            } catch (err) {
                // eg a template error, which shouldn't stop the session
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
                player.play(rendered.questionTags);
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
                player.play(rendered.answerTags);
            }
        } catch (err) {
            showFailure(err);
        }
    }

    async function answer(rating: CardAnswer_Rating): Promise<void> {
        if (side !== "answer" || !current) {
            return;
        }
        const card = current.card!;
        side = "transition";
        try {
            // The backend works out what the rating does, and refuses it if the
            // card has changed since it was shown.
            await answerCard(
                {
                    cardId: card.id,
                    cardMtimeSecs: card.mtimeSecs,
                    cardReps: card.reps,
                    rating,
                    answeredAtMillis: BigInt(Date.now()),
                    millisecondsTaken: timer.elapsedMillis(),
                },
                { alertOnError: false },
            );
        } catch (err) {
            // eg answered in another tab; carry on from what the backend has
            showNotice(err instanceof Error ? err.message : String(err));
        }
        await nextCard();
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

    // Audio
    // ---------------------------------------------------------

    function replay(): void {
        if (rendered && side !== "transition") {
            player.play(
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
            player.play([tag]);
        }
    }

    // Input
    // ---------------------------------------------------------

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
            default:
                return false;
        }
    }

    function onKeyDown(event: KeyboardEvent): void {
        unlockMedia();
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
                question:
                    "<p>This card kept trying to leave the page, so it isn't shown.</p>",
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
    $: hasMedia =
        !!rendered &&
        (rendered.questionTags.length > 0 || rendered.answerTags.length > 0);

    onMount(() => {
        player.mount(mediaContainer);
        nextCard();
    });

    onDestroy(() => {
        clearTimeout(noticeTimer);
        player.unmount();
    });
</script>

<svelte:window on:keydown={onKeyDown} on:click={unlockMedia} />

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

    <div class="media" bind:this={mediaContainer}></div>

    {#if mediaBlocked}
        <button class="btn btn-primary play-blocked" on:click={() => player.resume()}>
            ▶ Tap to play sound
        </button>
    {/if}

    <footer>
        <div class="tools">
            <button disabled={!undoLabel} title={undoLabel} on:click={performUndo}>
                {tr.undoUndo()}
            </button>
            {#if hasMedia}
                <button on:click={replay}>{tr.actionsReplayAudio()}</button>
            {/if}
        </div>

        <div class="counts">
            {#if queued}
                <span
                    class="new-count"
                    class:current={currentQueue === QueuedCards_Queue.NEW}
                >
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
        // less the navigation in +layout.svelte; dvh excludes a phone's browser bars
        height: calc(100vh - var(--app-nav-height));
        height: calc(100dvh - var(--app-nav-height));
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

    .media {
        display: none;
    }

    .play-blocked {
        min-height: 3rem;
        border-radius: 0;
    }

    footer {
        display: grid;
        grid-template-columns: 1fr auto 1fr;
        grid-template-areas: "counts actions tools";
        align-items: center;
        gap: 0.5em 1em;
        padding: 0.6em 1em;
        padding-bottom: max(0.6em, env(safe-area-inset-bottom));
        padding-left: max(1em, env(safe-area-inset-left));
        padding-right: max(1em, env(safe-area-inset-right));
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

    // A phone, or a narrow window: the answer buttons go along the bottom edge,
    // in reach of a thumb, with the rest above them.
    @media (max-width: 52em) {
        footer {
            grid-template-columns: 1fr auto;
            grid-template-areas: "counts tools" "actions actions";
            padding-top: 0.25em;
        }

        .tools button {
            min-height: 44px;
            padding: 0 0.9em;
            font-size: 0.9em;
        }

        .actions {
            align-items: stretch;
            gap: 0.4em;
        }

        .show-answer,
        .rating {
            flex: 1 1 0;
            min-width: 0;
            min-height: 3.75rem;
            padding: 0.25em;
            justify-content: center;
        }
    }
</style>
