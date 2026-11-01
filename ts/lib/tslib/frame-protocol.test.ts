// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

import { expect, test } from "vitest";

import { parseFrameMessage } from "./frame-protocol";

test("accepts well-formed messages, without extra properties", () => {
    expect(parseFrameMessage({ type: "ready", evil: 1 })).toEqual({ type: "ready" });
    expect(parseFrameMessage({ type: "key", key: "1", repeat: true }))
        .toEqual({ type: "key", key: "1", repeat: true });
    expect(parseFrameMessage({ type: "play", side: "a", index: 2 }))
        .toEqual({ type: "play", side: "a", index: 2 });
    expect(parseFrameMessage({ type: "open", url: "https://example.com/" }))
        .toEqual({ type: "open", url: "https://example.com/" });
});

test("refuses anything else", () => {
    for (
        const bad of [
            null,
            "ready",
            1,
            {},
            { type: "answerCard" },
            { type: "toString" },
            { type: "key", key: 1 },
            { type: "key", key: "x".repeat(33) },
            { type: "play", side: "x", index: 0 },
            { type: "play", side: "q", index: -1 },
            { type: "play", side: "q", index: 0.5 },
            { type: "play", side: "q", index: "0" },
            { type: "shown", seq: NaN },
            { type: "resize", height: Infinity },
            { type: "typed", value: 1 },
            { type: "typed", value: "x".repeat(100_001) },
            { type: "open", url: "javascript:alert(1)" },
            { type: "open", url: "data:text/html,x" },
            { type: "open", url: "/_anki/undo" },
        ]
    ) {
        expect(parseFrameMessage(bad)).toBeNull();
    }
});
