// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

import { marked } from "marked";

export function renderMarkdown(text: string): string {
    return marked(text, { mangle: false, headerIds: false });
}
