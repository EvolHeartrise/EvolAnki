// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

/** What the sandboxed frame (rslib/web-server/src/card_frame.html) may tell the
    page. Untrusted content shares that frame, and although nothing in it can run
    or reach the port these arrive on, everything is checked as if it could. */
export type FrameMessage =
    | { type: "ready" }
    | { type: "shown"; seq: number }
    | { type: "resize"; height: number }
    | { type: "key"; key: string; repeat: boolean }
    | { type: "play"; side: "q" | "a"; index: number }
    | { type: "typed"; value: string }
    | { type: "open"; url: string };

const MAX_TYPED_LENGTH = 100_000;

function isCount(value: unknown): value is number {
    return typeof value === "number" && Number.isSafeInteger(value) && value >= 0;
}

export function parseFrameMessage(data: unknown): FrameMessage | null {
    if (typeof data !== "object" || data === null) {
        return null;
    }
    const msg = data as Record<string, unknown>;
    switch (msg.type) {
        case "ready":
            return { type: "ready" };
        case "shown":
            return isCount(msg.seq) ? { type: "shown", seq: msg.seq } : null;
        case "resize":
            return isCount(msg.height) ? { type: "resize", height: msg.height } : null;
        case "key":
            return typeof msg.key === "string" && msg.key.length <= 32
                ? { type: "key", key: msg.key, repeat: msg.repeat === true }
                : null;
        case "play":
            return (msg.side === "q" || msg.side === "a") && isCount(msg.index)
                ? { type: "play", side: msg.side, index: msg.index }
                : null;
        case "typed":
            return typeof msg.value === "string" && msg.value.length <= MAX_TYPED_LENGTH
                ? { type: "typed", value: msg.value }
                : null;
        case "open":
            return typeof msg.url === "string" && isWebUrl(msg.url)
                ? { type: "open", url: msg.url }
                : null;
        default:
            return null;
    }
}

function isWebUrl(url: string): boolean {
    try {
        return ["http:", "https:"].includes(new URL(url).protocol);
    } catch {
        return false;
    }
}
