// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

/** The path prefix for what sandboxed frames can load, from evolanki-server. */
export async function getFrameBase(): Promise<string> {
    const response = await fetch("/_anki/frameBase");
    if (!response.ok) {
        throw new Error(`frameBase: ${response.status}`);
    }
    return await response.text();
}
