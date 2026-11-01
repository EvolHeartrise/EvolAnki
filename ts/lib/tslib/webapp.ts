// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

/** True when served by evolanki-server, which marks its pages, rather than
    shown in a Qt webview. */
export function isWebApp(): boolean {
    return !!document.querySelector("meta[name=evolanki-web]");
}

/** The path prefix for what sandboxed frames can load, from evolanki-server. */
export async function getFrameBase(): Promise<string> {
    const response = await fetch("/_anki/frameBase");
    if (!response.ok) {
        throw new Error(`frameBase: ${response.status}`);
    }
    return await response.text();
}
