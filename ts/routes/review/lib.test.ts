// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

import { RenderedTemplateNode } from "@generated/anki/card_rendering_pb";
import { expect, test } from "vitest";

import {
    AnswerTimer,
    avRefsToPlayIcons,
    fillInComparison,
    finishRendering,
    mediaUrl,
    parseTypeAnswer,
    replaceTypeAnswer,
    stripAvRefs,
    tagsToReplay,
    typeAnswerInput,
} from "./lib";
import { isVideoFile } from "./media";

test("finishRendering injects the front side only into the answer", () => {
    const nodes = [
        new RenderedTemplateNode({ value: { case: "text", value: "a " } }),
        new RenderedTemplateNode({
            value: { case: "replacement", value: { fieldName: "FrontSide", currentText: "", filters: [] } },
        }),
        new RenderedTemplateNode({
            value: { case: "replacement", value: { fieldName: "Back", currentText: "b", filters: ["x"] } },
        }),
    ];
    expect(finishRendering(nodes, "[anki:play:q:0]")).toBe("a [anki:play:q:0]b");
    expect(finishRendering(nodes, null)).toBe("a b");
});

test("audio references", () => {
    const html = avRefsToPlayIcons("x[anki:play:q:12]y[anki:play:z:1]");
    expect(html).toContain(`data-play="q:12"`);
    expect(html).not.toContain("onclick");
    expect(html).toContain("[anki:play:z:1]");
    expect(stripAvRefs("x[anki:play:a:0]y")).toBe("xy");
});

test("type answer modifiers", () => {
    expect(parseTypeAnswer("none")).toBeNull();
    expect(parseTypeAnswer("a[[type:Front]]")).toEqual({
        field: "Front",
        cloze: false,
        combining: true,
        ignoreCase: false,
    });
    expect(parseTypeAnswer("[[type:nc:cloze:ci:Text]]")).toEqual({
        field: "Text",
        cloze: true,
        combining: false,
        ignoreCase: true,
    });
});

test("type answer replacement treats its text literally", () => {
    expect(replaceTypeAnswer("[[type:A]]-[[type:B]]", "$&$1")).toBe("$&$1-$&$1");
    expect(typeAnswerInput(`a"><script>`, 20)).not.toContain("<script>");
});

test("comparison goes after the rule that {{FrontSide}} brought along", () => {
    const html = "q [[type:F]]<hr id=answer>a";
    expect(fillInComparison(html, "<b>c</b>", "Arial", 20)).toBe(
        `q <hr id=answer><div style="font-family: &#34;Arial&#34;; font-size: 20px;"><b>c</b></div>a`,
    );
    expect(fillInComparison("<hr id=answer>a", "c", "Arial", 20)).toBe("<hr id=answer>a");
});

test("timer", () => {
    let now = 1000;
    const timer = new AnswerTimer(() => now);
    now = 3500.4;
    expect(timer.elapsedMillis()).toBe(2500);
    timer.restart();
    now -= 1;
    expect(timer.elapsedMillis()).toBe(0);
    now += 1e12;
    expect(timer.elapsedMillis()).toBe(0xffffffff);
});

test("replay", () => {
    expect(tagsToReplay("q", [1], [2], true)).toEqual([1]);
    expect(tagsToReplay("a", [1], [2], true)).toEqual([1, 2]);
    expect(tagsToReplay("a", [1], [2], false)).toEqual([2]);
});

test("media urls can't leave the media path", () => {
    expect(mediaUrl("/_frame/t/", "a b#?.mp3")).toBe("/_frame/t/m/a%20b%23%3F.mp3");
});

test("video files are told from audio by extension", () => {
    expect(isVideoFile("a.mp4")).toBe(true);
    expect(isVideoFile("clip.WEBM")).toBe(true);
    expect(isVideoFile("a.mp3")).toBe(false);
    expect(isVideoFile("a.ogg")).toBe(false);
    expect(isVideoFile("noextension")).toBe(false);
});
