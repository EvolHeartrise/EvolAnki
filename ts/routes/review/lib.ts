// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

// The parts of qt/aqt/reviewer.py and pylib's card rendering that aren't UI.

import type { RenderedTemplateNode } from "@generated/anki/card_rendering_pb";

/** A notice to show on the overview, if the reviewer finished before its time
    was up. Client-side navigation keeps this alive. */
export const pendingNotice = { text: "" };

/** apply_custom_filters() in pylib/anki/template.py, without add-on filters.
    Passing the question's text in means that its audio references stay as they
    are, so {{FrontSide}} doesn't turn them into the answer's. */
export function finishRendering(nodes: RenderedTemplateNode[], frontSide: string | null): string {
    let text = "";
    for (const node of nodes) {
        if (node.value.case === "text") {
            text += node.value.value;
        } else if (node.value.case === "replacement") {
            const replacement = node.value.value;
            const isFront = replacement.fieldName === "FrontSide" && frontSide !== null;
            text += isFront ? frontSide : replacement.currentText;
        }
    }
    return text;
}

/** question_and_style() in pylib/anki/template.py. */
export function withStyle(css: string, html: string): string {
    return `<style>${css}</style>${html}`;
}

const AV_REF = /\[anki:(play:([qa]):(\d+))\]/g;

/** av_refs_to_play_icons() in qt/aqt/sound.py. */
export function avRefsToPlayIcons(text: string): string {
    return text.replace(
        AV_REF,
        (_match, command: string) =>
            `<a class="replay-button soundLink" href="#" onclick="pycmd('${command}'); return false;" draggable="false">`
            + `<svg class="playImage" viewBox="0 0 64 64" version="1.1"><circle cx="32" cy="32" r="29" />`
            + `<path d="M56.502,32.301l-37.502,20.101l0.329,-40.804l37.173,20.703Z" /></svg></a>`,
    );
}

export function stripAvRefs(text: string): string {
    return text.replace(AV_REF, "");
}

const TYPE_ANSWER = /\[\[type:(.+?)\]\]/;
const TYPE_ANSWER_ALL = new RegExp(TYPE_ANSWER.source, "g");

export interface TypeAnswerField {
    /** Field name from the template. */
    field: string;
    cloze: boolean;
    combining: boolean;
    ignoreCase: boolean;
}

/** The first [[type:...]] in a card's question, with its modifiers split off. */
export function parseTypeAnswer(question: string): TypeAnswerField | null {
    const match = question.match(TYPE_ANSWER);
    if (!match) {
        return null;
    }
    const parts = match[1].split(":");
    const field = parts.pop()!;
    return {
        field,
        cloze: parts.includes("cloze"),
        combining: !parts.includes("nc"),
        ignoreCase: parts.includes("ci"),
    };
}

function escapeHtmlAttribute(text: string): string {
    return text.replace(/[&<>"']/g, (c) => `&#${c.charCodeAt(0)};`);
}

function fieldStyle(font: string, size: number): string {
    return escapeHtmlAttribute(`font-family: "${font}"; font-size: ${size}px;`);
}

export function replaceTypeAnswer(html: string, replacement: string): string {
    return html.replace(TYPE_ANSWER_ALL, () => replacement);
}

export function typeAnswerInput(font: string, size: number): string {
    return `<center><input type="text" id="typeans" style="${fieldStyle(font, size)}"></center>`;
}

/** typeAnsAnswerFilter() in qt/aqt/reviewer.py. `compared` is already HTML. */
export function fillInComparison(html: string, compared: string, font: string, size: number): string {
    const withoutRule = html.replace("<hr id=answer>", "");
    const hadRule = withoutRule !== html;
    if (hadRule && !TYPE_ANSWER.test(withoutRule)) {
        return html;
    }
    const div = `<div style="${fieldStyle(font, size)}">${compared}</div>`;
    return replaceTypeAnswer(withoutRule, hadRule ? `<hr id=answer>${div}` : div);
}

/** Time spent on a card, for the review log. Like Card.time_taken(capped=False)
    in Python: the backend applies the deck's limit. */
export class AnswerTimer {
    private started: number;

    constructor(private now: () => number = () => performance.now()) {
        this.started = now();
    }

    restart(): void {
        this.started = this.now();
    }

    /** Whole milliseconds, within the range of the uint32 it's sent as. */
    elapsedMillis(): number {
        const elapsed = Math.round(this.now() - this.started);
        return Math.min(Math.max(elapsed, 0), 0xffffffff);
    }
}

/** What 'r' plays; replay_audio() in qt/aqt/reviewer.py. */
export function tagsToReplay<T>(side: "q" | "a", question: T[], answer: T[], includeQuestion: boolean): T[] {
    if (side === "q") {
        return question;
    }
    return includeQuestion ? [...question, ...answer] : answer;
}

/** One plain path segment per file; media names can contain anything. */
export function mediaUrl(frameBase: string, filename: string): string {
    return `${frameBase}m/${filename.split("/").map(encodeURIComponent).join("/")}`;
}
